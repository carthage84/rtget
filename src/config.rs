use std::path::PathBuf;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::args::CommandLineArgs;
use crate::error::AppError;
use crate::proxy::ProxyKind;
/// Fully resolved download settings.
#[derive(Clone, Debug)]
pub struct DownloadConfig {
    pub url: Url,
    pub output: Option<PathBuf>,
    pub connections: usize,
    pub resume: bool,
    pub timeout: Option<Duration>,
    pub tries: u32,
    pub user_agent: String,
    pub extra_headers: HeaderMap,
    pub proxy: ProxyKind,
    pub no_proxy: bool,
    pub insecure: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    pub quiet: bool,
}

impl DownloadConfig {
    pub fn from_args(args: &CommandLineArgs) -> Result<Self, AppError> {
        let url = args.resolved_url()?;
        let extra_headers = parse_headers(&args.header)?;
        let (username, password) =
            credentials(&url, args.user.as_deref(), args.password.as_deref());
        Ok(Self {
            url,
            output: args.output_hint(),
            connections: args.connections()?,
            resume: args.resume,
            timeout: args.timeout(),
            tries: args.tries(),
            user_agent: args.user_agent(),
            extra_headers,
            proxy: ProxyKind::from_url(args.proxy.as_deref(), args.no_proxy)?,
            no_proxy: args.no_proxy,
            insecure: args.no_check_certificate,
            username,
            password,
            quiet: args.quiet || args.background,
        })
    }

    pub fn output_path(&self, suggested: &str) -> PathBuf {
        match &self.output {
            Some(path) if path.is_dir() => path.join(suggested),
            Some(path) => path.clone(),
            None => PathBuf::from(suggested),
        }
    }

    pub fn ftp_port(&self) -> u16 {
        self.url
            .port()
            .unwrap_or(if self.url.scheme() == "ftps" { 990 } else { 21 })
    }

    pub fn ftp_host(&self) -> Result<&str, AppError> {
        self.url.host_str().ok_or(AppError::InvalidHostname)
    }

    pub fn ftp_path(&self) -> String {
        let path = self.url.path();
        if path.is_empty() {
            "/".to_string()
        } else {
            percent_encoding::percent_decode_str(path)
                .decode_utf8_lossy()
                .into_owned()
        }
    }
}

fn credentials(
    url: &Url,
    user: Option<&str>,
    password: Option<&str>,
) -> (Option<String>, Option<String>) {
    let url_user = if url.username().is_empty() {
        None
    } else {
        Some(
            percent_encoding::percent_decode_str(url.username())
                .decode_utf8_lossy()
                .into_owned(),
        )
    };
    let url_pass = url.password().map(|p| {
        percent_encoding::percent_decode_str(p)
            .decode_utf8_lossy()
            .into_owned()
    });
    (
        user.map(str::to_string).or(url_user),
        password.map(str::to_string).or(url_pass),
    )
}

fn parse_headers(headers: &[String]) -> Result<HeaderMap, AppError> {
    let mut map = HeaderMap::new();
    for raw in headers {
        let (name, value) = raw.split_once(':').ok_or_else(|| {
            AppError::InvalidArgument(format!("header `{raw}` must be in `Name: value` form"))
        })?;
        let name = HeaderName::from_bytes(name.trim().as_bytes()).map_err(|e| {
            AppError::InvalidArgument(format!("invalid header name `{}`: {e}", name.trim()))
        })?;
        let value = HeaderValue::from_str(value.trim()).map_err(|e| {
            AppError::InvalidArgument(format!("invalid header value for {name}: {e}"))
        })?;
        map.append(name, value);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_custom_headers() {
        let map = parse_headers(&["Accept: text/plain".into(), "X-Test: 1".into()]).unwrap();
        assert_eq!(map.get("accept").unwrap(), "text/plain");
        assert_eq!(map.get("x-test").unwrap(), "1");
    }
}
