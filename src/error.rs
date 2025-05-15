use std::fmt::Formatter;

// Error enum for the application
#[derive(Debug)]
pub enum AppError {
    UrlParseError(String),
    InvalidScheme,
    InvalidHostname,
    UrlValidationError(String),
    CouldNotConnect(String),
    UnsupportedProtocol,
    StringError(String),
    CouldNotReadChunk(String),
    TaskError(String),
}

// Implement Display for AppError
impl std::fmt::Display for AppError {
    // Implement Display for AppError
    // This is required to allow the error to be printed to the console
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Match the error type and print the appropriate message
        match self {
            AppError::UrlParseError(ref err) => write!(f, "{}", err),
            AppError::InvalidScheme => write!(f, "Invalid URL scheme"),
            AppError::InvalidHostname => write!(f, "Hostname is either missing or invalid"),
            AppError::UrlValidationError(msg) => write!(f, "URL is not valid: {}", msg),
            AppError::CouldNotConnect(msg) => write!(f, "Could not connect to the server: {}", msg),
            AppError::CouldNotReadChunk(msg) => write!(f, "Could not read chunk: {}", msg),
            AppError::UnsupportedProtocol => write!(f, "Unsupported protocol"),
            // TODO: handle other errors as the need arise
            AppError::StringError(msg) => write!(f, "An error occurred: {}", msg),
            AppError::TaskError(msg) => write!(f, "Task error: {}", msg),
        }
    }
}

// Implement From<String> for AppError
// This is required to allow the error to be converted from a String
impl From<String> for AppError {
    fn from(err: String) -> Self {
        AppError::StringError(err)
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        AppError::TaskError(err.to_string())
    }   
}

// Implement From<AppError> for AppError
// This is required to allow the error to be converted from another AppError
impl std::error::Error for AppError {}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_parse_error_message() {
        let error = AppError::UrlParseError("Invalid format".to_string());
        assert_eq!(format!("{}", error), "URL parsing error: Invalid format");
    }

    #[test]
    fn test_invalid_scheme_error_message() {
        let error = AppError::InvalidScheme;
        assert_eq!(format!("{}", error), "Invalid URL scheme");
    }

    #[test]
    fn test_invalid_hostname_error_message() {
        let error = AppError::InvalidHostname;
        assert_eq!(format!("{}", error), "Hostname is either missing or invalid");
    }

    #[test]
    fn test_url_validation_error_message() {
        let error = AppError::UrlValidationError("Invalid format".to_string());
        assert_eq!(format!("{}", error), "URL is not valid: Invalid format");
    }
}