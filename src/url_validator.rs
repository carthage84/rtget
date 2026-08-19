use percent_encoding::percent_decode_str;
use url::Url;

use crate::error::AppError;

/// Validates a URL and ensures the scheme is one of http, https, ftp, or ftps.
pub fn validate_url(url: &str) -> Result<Url, AppError> {
    let parsed = Url::parse(url)?;

    match parsed.scheme() {
        "http" | "https" | "ftp" | "ftps" => {}
        _ => return Err(AppError::InvalidScheme),
    }

    if parsed.host().is_none() {
        return Err(AppError::InvalidHostname);
    }

    Ok(parsed)
}

/// Best-effort filename derived from a URL path.
pub fn filename_from_url(url: &Url) -> String {
    let last = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or("");
    let decoded = percent_decode_str(last).decode_utf8_lossy();
    sanitize_filename(&decoded)
}

/// Strip directory components and reject empty / reserved names.
pub fn sanitize_filename(name: &str) -> String {
    let trimmed = name.trim();
    let base = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed).trim();
    if base.is_empty() || base == "." || base == ".." {
        "index.html".to_string()
    } else {
        base.replace(['\0', ':'], "_")
    }
}

/// Parse a Content-Disposition header for a filename.
pub fn filename_from_content_disposition(header: &str) -> Option<String> {
    // Prefer RFC 5987 filename*=charset''value
    if let Some(star) = find_param(header, "filename*") {
        let value = unquote(&star);
        let decoded = if let Some((_charset, rest)) = value.split_once("''") {
            percent_decode_str(rest).decode_utf8_lossy().into_owned()
        } else {
            value
        };
        return Some(sanitize_filename(&decoded));
    }
    if let Some(plain) = find_param(header, "filename") {
        return Some(sanitize_filename(&unquote(&plain)));
    }
    None
}

fn find_param(header: &str, name: &str) -> Option<String> {
    for part in header.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case(name) {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].replace("\\\"", "\"")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_url() {
        assert!(validate_url("invalid-url").is_err());
    }

    #[test]
    fn accepts_http_and_ftp() {
        assert!(validate_url("https://example.com/file").is_ok());
        assert!(validate_url("ftp://ftp.example.com/pub/file").is_ok());
        assert!(validate_url("ftps://ftp.example.com/file").is_ok());
    }

    #[test]
    fn rejects_unknown_scheme() {
        match validate_url("sftp://example.com/file") {
            Err(AppError::InvalidScheme) => {}
            other => panic!("expected InvalidScheme, got {other:?}"),
        }
    }

    #[test]
    fn directory_url_uses_index_html() {
        let url = Url::parse("https://example.com/dir/").unwrap();
        assert_eq!(filename_from_url(&url), "index.html");
    }

    #[test]
    fn decodes_percent_encoded_name() {
        let url = Url::parse("https://example.com/my%20file.zip").unwrap();
        assert_eq!(filename_from_url(&url), "my file.zip");
    }

    #[test]
    fn parses_content_disposition() {
        assert_eq!(
            filename_from_content_disposition(r#"attachment; filename="report.pdf""#).as_deref(),
            Some("report.pdf")
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''na%C3%AFve.txt")
                .as_deref(),
            Some("naïve.txt")
        );
    }

    #[test]
    fn sanitizes_path_in_filename() {
        assert_eq!(sanitize_filename(r"..\secret.txt"), "secret.txt");
        assert_eq!(sanitize_filename(""), "index.html");
    }
}
