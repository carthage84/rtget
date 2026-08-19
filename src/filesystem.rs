use std::path::{Path, PathBuf};

use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::AppError;

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
}
