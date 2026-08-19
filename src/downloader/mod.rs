mod ftp;
mod http;
pub mod range;

use crate::config::DownloadConfig;
use crate::error::AppError;
use crate::progress::ProgressManager;

/// Dispatch a download to the HTTP or FTP implementation.
pub async fn download(config: &DownloadConfig, progress: &ProgressManager) -> Result<(), AppError> {
    match config.url.scheme() {
        "http" | "https" => http::download(config, progress).await,
        "ftp" | "ftps" => ftp::download(config, progress).await,
        _ => Err(AppError::UnsupportedProtocol),
    }
}
