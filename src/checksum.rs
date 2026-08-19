use std::io::Read;
use std::path::Path;

use md5::Md5;
use sha2::{Digest, Sha256, Sha512};

use crate::error::AppError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checksum {
    pub algo: HashAlgo,
    pub expected: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashAlgo {
    Md5,
    Sha256,
    Sha512,
}

impl HashAlgo {
    fn as_str(self) -> &'static str {
        match self {
            HashAlgo::Md5 => "md5",
            HashAlgo::Sha256 => "sha256",
            HashAlgo::Sha512 => "sha512",
        }
    }

    fn digest_len(self) -> usize {
        match self {
            HashAlgo::Md5 => 16,
            HashAlgo::Sha256 => 32,
            HashAlgo::Sha512 => 64,
        }
    }
}

/// Parse `sha256:deadbeef…`, `md5:…`, or `sha512:…`.
pub fn parse_checksum(raw: &str) -> Result<Checksum, AppError> {
    let (algo_s, hex) = raw
        .split_once(':')
        .ok_or_else(|| AppError::InvalidArgument("checksum must be `algo:hex`".into()))?;
    let algo = match algo_s.trim().to_ascii_lowercase().as_str() {
        "md5" => HashAlgo::Md5,
        "sha256" | "sha-256" => HashAlgo::Sha256,
        "sha512" | "sha-512" => HashAlgo::Sha512,
        other => {
            return Err(AppError::InvalidArgument(format!(
                "unsupported checksum algorithm `{other}` (md5, sha256, sha512)"
            )));
        }
    };
    let expected = parse_hex(hex.trim())?;
    if expected.len() != algo.digest_len() {
        return Err(AppError::InvalidArgument(format!(
            "{} digest must be {} hex bytes, got {}",
            algo.as_str(),
            algo.digest_len(),
            expected.len()
        )));
    }
    Ok(Checksum { algo, expected })
}

pub fn verify_file(path: &Path, checksum: &Checksum) -> Result<(), AppError> {
    let actual = hash_file(path, checksum.algo)?;
    if actual == checksum.expected {
        Ok(())
    } else {
        Err(AppError::ChecksumMismatch {
            expected: format!("{}:{}", checksum.algo.as_str(), to_hex(&checksum.expected)),
            actual: format!("{}:{}", checksum.algo.as_str(), to_hex(&actual)),
        })
    }
}

fn hash_file(path: &Path, algo: HashAlgo) -> Result<Vec<u8>, AppError> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    match algo {
        HashAlgo::Md5 => {
            let mut h = Md5::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(h.finalize().to_vec())
        }
        HashAlgo::Sha256 => {
            let mut h = Sha256::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(h.finalize().to_vec())
        }
        HashAlgo::Sha512 => {
            let mut h = Sha512::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(h.finalize().to_vec())
        }
    }
}

fn parse_hex(s: &str) -> Result<Vec<u8>, AppError> {
    let s: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        return Err(AppError::InvalidArgument(
            "checksum hex must have an even length".into(),
        ));
    }
    (0..s.len() / 2)
        .map(|i| {
            u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| {
                AppError::InvalidArgument(format!("invalid hex in checksum at byte {i}"))
            })
        })
        .collect()
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_and_verifies_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"hello").unwrap();
        drop(f);
        let cs = parse_checksum(
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        )
        .unwrap();
        verify_file(&path, &cs).unwrap();
        let bad = parse_checksum(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        assert!(verify_file(&path, &bad).is_err());
    }

    #[test]
    fn rejects_bad_spec() {
        assert!(parse_checksum("sha256").is_err());
        assert!(parse_checksum("blake3:aa").is_err());
        assert!(parse_checksum("md5:00").is_err());
    }
}
