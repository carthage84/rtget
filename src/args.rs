use std::path::PathBuf;
use std::time::Duration;

use argh::FromArgs;

use crate::error::AppError;
use crate::url_validator::validate_url;

/// A concurrent wget-like network downloader
#[derive(FromArgs, Clone, Debug)]
pub struct CommandLineArgs {
    /// URL to download
    #[argh(positional)]
    pub url_pos: Option<String>,

    /// URL to download
    #[argh(option, short = 'u')]
    pub url: Option<String>,

    /// output file path
    #[argh(option, short = 'o')]
    pub output: Option<String>,

    /// number of concurrent connections (default 4, max 64)
    #[argh(option, short = 'c', default = "4")]
    pub connections: u8,

    /// run in the background
    #[argh(switch, short = 'b')]
    pub background: bool,

    /// verbose logging
    #[argh(switch, short = 'v')]
    pub verbose: bool,

    /// quiet mode (no progress bar)
    #[argh(switch, short = 'q')]
    pub quiet: bool,

    /// resume a partial download
    #[argh(switch, short = 'C')]
    pub resume: bool,

    /// network timeout in seconds (0 = no timeout, default 30)
    #[argh(option, short = 'T', default = "30")]
    pub timeout: u64,

    /// number of retries per request (default 5)
    #[argh(option, short = 't', default = "5")]
    pub tries: u32,

    /// user-agent string
    #[argh(option, short = 'U')]
    pub user_agent: Option<String>,

    /// extra HTTP header, repeatable (`Name: value`)
    #[argh(option, short = 'H')]
    pub header: Vec<String>,

    /// proxy URL (`http://`, `https://`, `socks5://`, or `socks5h://`)
    #[argh(option)]
    pub proxy: Option<String>,

    /// ignore proxy settings from the environment
    #[argh(switch)]
    pub no_proxy: bool,

    /// skip TLS certificate verification
    #[argh(switch)]
    pub no_check_certificate: bool,

    /// username for HTTP or FTP authentication
    #[argh(option, long = "user")]
    pub user: Option<String>,

    /// password for HTTP or FTP authentication
    #[argh(option, long = "password")]
    pub password: Option<String>,
}

impl CommandLineArgs {
    pub fn resolved_url(&self) -> Result<url::Url, AppError> {
        let raw = self
            .url
            .as_deref()
            .or(self.url_pos.as_deref())
            .ok_or_else(|| {
                AppError::InvalidArgument("a URL is required (positional or -u/--url)".into())
            })?;
        validate_url(raw)
    }

    pub fn connections(&self) -> Result<usize, AppError> {
        match self.connections {
            0 => Err(AppError::InvalidArgument(
                "connections must be at least 1".into(),
            )),
            n if n > 64 => Err(AppError::InvalidArgument(
                "connections cannot exceed 64".into(),
            )),
            n => Ok(n as usize),
        }
    }

    pub fn timeout(&self) -> Option<Duration> {
        if self.timeout == 0 {
            None
        } else {
            Some(Duration::from_secs(self.timeout))
        }
    }

    pub fn user_agent(&self) -> String {
        self.user_agent
            .clone()
            .unwrap_or_else(|| format!("rtget/{}", env!("CARGO_PKG_VERSION")))
    }

    pub fn output_hint(&self) -> Option<PathBuf> {
        self.output.as_ref().map(PathBuf::from)
    }

    pub fn tries(&self) -> u32 {
        self.tries.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flag_url_and_background() {
        let args = CommandLineArgs::from_args(
            &["rtget"],
            &["--url", "http://example.com", "--background"],
        )
        .unwrap();
        assert_eq!(args.url.as_deref(), Some("http://example.com"));
        assert!(args.background);
        assert_eq!(args.connections, 4);
    }

    #[test]
    fn parses_positional_url() {
        let args =
            CommandLineArgs::from_args(&["rtget"], &["https://example.com/file.bin"]).unwrap();
        assert_eq!(
            args.url_pos.as_deref(),
            Some("https://example.com/file.bin")
        );
        assert_eq!(
            args.resolved_url().unwrap().as_str(),
            "https://example.com/file.bin"
        );
    }

    #[test]
    fn errors_when_no_arguments() {
        let args = CommandLineArgs::from_args(&["rtget"], &[]);
        assert!(args.is_ok(), "optional fields should parse with defaults");
        let parsed = args.unwrap();
        assert!(parsed.resolved_url().is_err());
    }

    #[test]
    fn parses_proxy_and_headers() {
        let args = CommandLineArgs::from_args(
            &["rtget"],
            &[
                "http://example.com/a",
                "--proxy",
                "socks5://127.0.0.1:1080",
                "-H",
                "Accept: text/plain",
                "--resume",
            ],
        )
        .unwrap();
        assert_eq!(args.proxy.as_deref(), Some("socks5://127.0.0.1:1080"));
        assert_eq!(args.header, vec!["Accept: text/plain"]);
        assert!(args.resume);
    }

    #[test]
    fn rejects_zero_connections() {
        let args =
            CommandLineArgs::from_args(&["rtget"], &["http://example.com", "-c", "0"]).unwrap();
        assert!(args.connections().is_err());
    }
}
