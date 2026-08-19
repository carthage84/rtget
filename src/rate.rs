use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::error::AppError;

/// Shared token-bucket limiter (bytes per second).
#[derive(Debug)]
pub struct RateLimiter {
    bytes_per_sec: u64,
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(bytes_per_sec: u64) -> Self {
        Self {
            bytes_per_sec,
            inner: Mutex::new(Inner {
                tokens: bytes_per_sec as f64,
                last: Instant::now(),
            }),
        }
    }

    pub async fn consume(&self, n: usize) {
        let bps = self.bytes_per_sec as f64;
        if bps <= 0.0 || n == 0 {
            return;
        }
        let need = n as f64;
        let sleep_for = {
            let mut g = self.inner.lock().await;
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(g.last).as_secs_f64();
            g.tokens = (g.tokens + elapsed * bps).min(bps * 2.0);
            g.last = now;
            if g.tokens >= need {
                g.tokens -= need;
                None
            } else {
                let deficit = need - g.tokens;
                g.tokens = 0.0;
                let wait = Duration::from_secs_f64(deficit / bps);
                g.last = now + wait;
                Some(wait)
            }
        };
        if let Some(d) = sleep_for {
            tokio::time::sleep(d).await;
        }
    }
}

/// Parse wget-style rates: `100`, `100k`, `2M`, `1g`.
pub fn parse_rate(raw: &str) -> Result<u64, AppError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(AppError::InvalidArgument(
            "limit-rate must be a positive size (e.g. 100k, 2M)".into(),
        ));
    }
    let (num, mul) = match s.as_bytes().last().copied() {
        Some(b) if b.is_ascii_alphabetic() => {
            let mul = match b.to_ascii_lowercase() {
                b'k' => 1024u64,
                b'm' => 1024 * 1024,
                b'g' => 1024 * 1024 * 1024,
                _ => {
                    return Err(AppError::InvalidArgument(format!(
                        "unknown rate suffix in `{raw}` (use k, M, or G)"
                    )));
                }
            };
            (&s[..s.len() - 1], mul)
        }
        _ => (s, 1u64),
    };
    let n: u64 = num
        .trim()
        .parse()
        .map_err(|_| AppError::InvalidArgument(format!("invalid limit-rate `{raw}`")))?;
    n.checked_mul(mul)
        .filter(|v| *v > 0)
        .ok_or_else(|| AppError::InvalidArgument(format!("invalid limit-rate `{raw}`")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_suffixes() {
        assert_eq!(parse_rate("100").unwrap(), 100);
        assert_eq!(parse_rate("100k").unwrap(), 100 * 1024);
        assert_eq!(parse_rate("2M").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_rate("1g").unwrap(), 1024 * 1024 * 1024);
        assert!(parse_rate("0").is_err());
        assert!(parse_rate("10x").is_err());
    }
}
