use std::time::Duration;

use log::warn;

use crate::error::AppError;

/// Run `op` up to `tries` times with exponential backoff.
pub async fn with_retry<F, Fut, T>(tries: u32, mut op: F) -> Result<T, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    let tries = tries.max(1);
    let mut last = None;
    for attempt in 1..=tries {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                if attempt < tries {
                    let delay = Duration::from_millis(400 * (1 << (attempt.min(6) - 1)));
                    warn!("attempt {attempt}/{tries} failed: {err}; retrying in {delay:?}");
                    tokio::time::sleep(delay).await;
                } else {
                    warn!("attempt {attempt}/{tries} failed: {err}");
                }
                last = Some(err);
            }
        }
    }
    Err(last.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn succeeds_after_retries() {
        let n = AtomicU32::new(0);
        let result = with_retry(3, || async {
            let v = n.fetch_add(1, Ordering::SeqCst);
            if v < 2 {
                Err(AppError::Download("not yet".into()))
            } else {
                Ok(7)
            }
        })
        .await
        .unwrap();
        assert_eq!(result, 7);
        assert_eq!(n.load(Ordering::SeqCst), 3);
    }
}
