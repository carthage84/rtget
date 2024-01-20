use url::{Url, ParseError};
use crate::error::AppError;

/// Validates a given URL string.
///
/// Returns `Ok` if the URL is valid and conforms to the expected schemes,
/// otherwise returns an `Err` with a description of the issue.
pub fn validate_url(url: &str) -> Result<Url, AppError> {
    let parsed_url = Url::parse(url).map_err(|e| AppError::UrlParseError(e.to_string()))?;

    // Check if the schema is one of the allowed ones
    match parsed_url.scheme() {
        "http" | "https" | "ftp" | "ftps" => (),
        _ => return Err(AppError::InvalidScheme),
    }

    // Hostname validation
    if parsed_url.host().is_none() {
        return Err(AppError::InvalidHostname);
    }

    Ok(parsed_url)
}