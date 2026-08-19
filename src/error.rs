use std::path::PathBuf;

/// Application error type.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid URL: {0}")]
    UrlParse(String),
    #[error("invalid URL scheme (supported: http, https, ftp, ftps)")]
    InvalidScheme,
    #[error("hostname is either missing or invalid")]
    InvalidHostname,
    #[error("could not connect: {0}")]
    Connect(String),
    #[error("unsupported protocol")]
    UnsupportedProtocol,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("FTP error: {0}")]
    Ftp(#[from] suppaftp::FtpError),
    #[error("download failed: {0}")]
    Download(String),
    #[error("proxy error: {0}")]
    Proxy(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("task error: {0}")]
    Task(String),
    #[error("partial file missing: {0}")]
    MissingPart(PathBuf),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

impl From<url::ParseError> for AppError {
    fn from(err: url::ParseError) -> Self {
        AppError::UrlParse(err.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        AppError::Task(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_parse_error_message() {
        let error = AppError::UrlParse("Invalid format".to_string());
        assert_eq!(format!("{error}"), "invalid URL: Invalid format");
    }

    #[test]
    fn invalid_scheme_error_message() {
        let error = AppError::InvalidScheme;
        assert_eq!(
            format!("{error}"),
            "invalid URL scheme (supported: http, https, ftp, ftps)"
        );
    }

    #[test]
    fn invalid_hostname_error_message() {
        let error = AppError::InvalidHostname;
        assert_eq!(format!("{error}"), "hostname is either missing or invalid");
    }
}
