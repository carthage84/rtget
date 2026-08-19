use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::AppError;
use crate::rate::RateLimiter;

const COPY_BUF: usize = 64 * 1024;

/// Path of the Nth temporary part file next to `output`.
///
/// Uses only the file name so Windows paths with drive letters cannot leak
/// into the part file name (`C:\foo` is not a legal filename).
pub fn part_path(output: &Path, index: usize) -> PathBuf {
    let name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "download".to_string());
    output.with_file_name(format!("{name}.part.{index}"))
}

pub fn existing_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

pub async fn ensure_parent_dir(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).await?;
    }
    Ok(())
}

pub async fn open_append(path: &Path) -> Result<File, AppError> {
    ensure_parent_dir(path).await?;
    Ok(OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(path)
        .await?)
}

pub async fn open_write_truncate(path: &Path) -> Result<File, AppError> {
    ensure_parent_dir(path).await?;
    Ok(OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .await?)
}

pub async fn write_limited(
    file: &mut File,
    data: &[u8],
    limiter: Option<&RateLimiter>,
) -> Result<(), AppError> {
    file.write_all(data).await?;
    if let Some(limiter) = limiter {
        limiter.consume(data.len()).await;
    }
    Ok(())
}

/// Whether `path` should be treated as a directory (existing dir or trailing slash).
pub fn dir_hint(path: &Path) -> bool {
    path.is_dir() || path.as_os_str().to_string_lossy().ends_with(['/', '\\'])
}

pub enum OutputResolve {
    Use(PathBuf),
    Skip(PathBuf),
}

/// Skip, overwrite, resume, or auto-rename when the destination already exists.
pub fn resolve_output(
    path: &Path,
    resume: bool,
    no_clobber: bool,
    explicit_file: bool,
) -> OutputResolve {
    if !path.exists() {
        return OutputResolve::Use(path.to_path_buf());
    }
    if resume {
        return OutputResolve::Use(path.to_path_buf());
    }
    if no_clobber {
        return OutputResolve::Skip(path.to_path_buf());
    }
    if explicit_file {
        return OutputResolve::Use(path.to_path_buf());
    }
    OutputResolve::Use(unique_path(path))
}

/// `file.bin` → `file.1.bin` → `file.2.bin` …
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 1..10_000 {
        let name = match &ext {
            Some(ext) => format!("{stem}.{n}.{ext}"),
            None => format!("{stem}.{n}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}.9999"))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResumeSidecar {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

pub fn sidecar_path(output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    output.with_file_name(format!("{name}.rtget.json"))
}

pub fn load_sidecar(output: &Path) -> Option<ResumeSidecar> {
    let path = sidecar_path(output);
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn write_sidecar(output: &Path, sidecar: &ResumeSidecar) -> Result<(), AppError> {
    let path = sidecar_path(output);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(sidecar)
        .map_err(|e| AppError::Download(format!("sidecar serialize: {e}")))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// True when the remote resource still matches what we started downloading.
pub fn sidecar_matches(sidecar: &ResumeSidecar, remote: &ResumeSidecar) -> bool {
    if let (Some(a), Some(b)) = (&sidecar.etag, &remote.etag)
        && !a.is_empty()
        && !b.is_empty()
    {
        return a == b;
    }
    if let (Some(a), Some(b)) = (&sidecar.last_modified, &remote.last_modified)
        && !a.is_empty()
        && !b.is_empty()
    {
        return a == b;
    }
    if let (Some(a), Some(b)) = (sidecar.size, remote.size) {
        return a == b;
    }
    true
}

/// Stream-copy part files into `output` and delete them afterwards.
pub async fn merge_parts(output: &Path, num_parts: usize) -> Result<(), AppError> {
    let mut out = open_write_truncate(output).await?;
    let mut buf = vec![0u8; COPY_BUF];

    for i in 0..num_parts {
        let part = part_path(output, i);
        if !part.exists() {
            return Err(AppError::MissingPart(part));
        }
        let mut src = File::open(&part)
            .await
            .map_err(|e| AppError::Download(format!("failed to open {}: {e}", part.display())))?;
        loop {
            let n = src.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).await?;
        }
    }

    out.flush().await?;
    drop(out);

    for i in 0..num_parts {
        let part = part_path(output, i);
        if part.exists() {
            fs::remove_file(&part).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn part_path_uses_file_name_only() {
        let output = Path::new(r"C:\Users\carth\file.iso");
        let part = part_path(output, 0);
        let name = part.file_name().unwrap().to_string_lossy();
        assert_eq!(name, "file.iso.part.0");
        assert!(!name.contains(':'));
        assert!(!name.contains('\\'));
    }

    #[tokio::test]
    async fn merge_parts_concatenates_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.bin");
        for (i, chunk) in [b"AAA".as_slice(), b"BBB", b"CCC"].iter().enumerate() {
            let mut f = std::fs::File::create(part_path(&output, i)).unwrap();
            f.write_all(chunk).unwrap();
        }
        merge_parts(&output, 3).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"AAABBBCCC");
        assert!(!part_path(&output, 0).exists());
        assert!(!part_path(&output, 1).exists());
        assert!(!part_path(&output, 2).exists());
    }

    #[test]
    fn unique_path_inserts_number_before_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        std::fs::write(&path, b"x").unwrap();
        let next = unique_path(&path);
        assert_eq!(next.file_name().unwrap(), "file.1.bin");
        std::fs::write(&next, b"y").unwrap();
        let third = unique_path(&path);
        assert_eq!(third.file_name().unwrap(), "file.2.bin");
    }

    #[test]
    fn no_clobber_skips_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keep.bin");
        std::fs::write(&path, b"x").unwrap();
        match resolve_output(&path, false, true, false) {
            OutputResolve::Skip(p) => assert_eq!(p, path),
            _ => panic!("expected skip"),
        }
    }

    #[test]
    fn sidecar_roundtrip_and_etag_match() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("blob.bin");
        let saved = ResumeSidecar {
            url: "http://example.com/blob.bin".into(),
            etag: Some("\"abc\"".into()),
            last_modified: None,
            size: Some(10),
        };
        write_sidecar(&output, &saved).unwrap();
        let loaded = load_sidecar(&output).unwrap();
        assert_eq!(loaded, saved);
        let remote = ResumeSidecar {
            etag: Some("\"abc\"".into()),
            size: Some(99),
            ..ResumeSidecar::default()
        };
        assert!(sidecar_matches(&saved, &remote));
        let changed = ResumeSidecar {
            etag: Some("\"zzz\"".into()),
            ..ResumeSidecar::default()
        };
        assert!(!sidecar_matches(&saved, &changed));
    }
}
