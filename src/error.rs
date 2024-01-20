use std::fmt::Formatter;

#[derive(Debug)]
pub enum AppError {
    UrlParseError(String),
    InvalidScheme,
    InvalidHostname,
    UrlValidationError(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::UrlParseError(ref err) => write!(f, "URL parsing error: {}", err),
            AppError::InvalidScheme => write!(f, "Invalid URL scheme"),
            AppError::InvalidHostname => write!(f, "Hostname is either missing or invalid"),
            AppError::UrlValidationError(msg) => write!(f, "URL is not valid: {}", msg),
            // TODO: handle other errors as the need arise
        }
    }
}

impl std::error::Error for AppError {}