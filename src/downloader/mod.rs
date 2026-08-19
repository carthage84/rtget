mod ftp;
mod http;
mod range;

use std::path::PathBuf;

use crate::checksum;
use crate::config::DownloadConfig;
use crate::error::AppError;
use crate::progress::ProgressManager;

#[derive(Debug)]
pub enum TransferResult {
    Saved(PathBuf),
    Skipped(PathBuf),
    Spider,
}

/// Dispatch a download to the HTTP or FTP implementation.
pub async fn download(
    config: &DownloadConfig,
    progress: &ProgressManager,
) -> Result<TransferResult, AppError> {
    let result = match config.url.scheme() {
        "http" | "https" => http::download(config, progress).await?,
        "ftp" | "ftps" => ftp::download(config, progress).await?,
        _ => return Err(AppError::UnsupportedProtocol),
    };
    if let TransferResult::Saved(path) = &result
        && let Some(cs) = &config.checksum
    {
        checksum::verify_file(path, cs)?;
    }
    Ok(result)
}
