use std::path::PathBuf;
use std::time::Duration;

use argh::FromArgs;

use crate::error::AppError;
use crate::url_validator::validate_url;

/// A concurrent wget-like network downloader
#[derive(FromArgs, Clone, Debug)]
pub struct CommandLineArgs {
    /// URL(s) to download
    #[argh(positional)]
    pub url_pos: Vec<String>,

    /// URL to download (repeatable via multiple invocations; combined with positionals)
    #[argh(option, short = 'u')]
    pub url: Option<String>,

    /// read URLs from a file (one per line; `-` for stdin)
    #[argh(option, short = 'i', long = "input-file")]
    pub input_file: Option<String>,

    /// output file path
    #[argh(option, short = 'o')]
    pub output: Option<String>,

    /// directory to save files into
    #[argh(option, short = 'P', long = "directory-prefix")]
    pub directory_prefix: Option<String>,

    /// number of concurrent connections (default 4, max 64)
    #[argh(option, short = 'c')]
    pub connections: Option<u8>,

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

    /// skip download if the output file already exists
    #[argh(switch, short = 'n', long = "no-clobber")]
    pub no_clobber: bool,

    /// network timeout in seconds (0 = no timeout, default 30)
    #[argh(option, short = 'T')]
    pub timeout: Option<u64>,

    /// number of retries per request (default 5)
    #[argh(option, short = 't')]
    pub tries: Option<u32>,

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

    /// prompt for a password
    #[argh(switch, long = "ask-password")]
    pub ask_password: bool,

    /// cap download speed (`100k`, `2M`)
    #[argh(option, long = "limit-rate")]
    pub limit_rate: Option<String>,

    /// load cookies from a Netscape cookie file
    #[argh(option, long = "load-cookies")]
    pub load_cookies: Option<String>,

    /// save cookies to a Netscape cookie file when finished
    #[argh(option, long = "save-cookies")]
    pub save_cookies: Option<String>,

    /// path to a TOML config file
    #[argh(option, long = "config")]
    pub config: Option<String>,

    /// ignore the default config file
    #[argh(switch, long = "no-config")]
    pub no_config: bool,

    /// probe the URL and print metadata without saving a file
    #[argh(switch)]
    pub spider: bool,

    /// maximum HTTP/HTML redirects to follow (default 20)
    #[argh(option, long = "max-redirect")]
    pub max_redirect: Option<usize>,

    /// verify the finished file (`sha256:hex`, `sha512:hex`, or `md5:hex`)
    #[argh(option)]
    pub checksum: Option<String>,

    /// stop after the first failed URL in a batch
    #[argh(switch, long = "fail-fast")]
    pub fail_fast: bool,
}

impl CommandLineArgs {
    pub fn connections(&self) -> Result<Option<usize>, AppError> {
        match self.connections {
            None => Ok(None),
            Some(0) => Err(AppError::InvalidArgument(
                "connections must be at least 1".into(),
            )),
            Some(n) if n > 64 => Err(AppError::InvalidArgument(
                "connections cannot exceed 64".into(),
            )),
            Some(n) => Ok(Some(n as usize)),
        }
    }

    pub fn timeout(&self) -> Option<Option<Duration>> {
        self.timeout.map(|secs| {
            if secs == 0 {
                None
            } else {
                Some(Duration::from_secs(secs))
            }
        })
    }

    pub fn tries(&self) -> Option<u32> {
        self.tries.map(|n| n.max(1))
    }

    pub fn output_hint(&self) -> Option<PathBuf> {
        self.output.as_ref().map(PathBuf::from)
    }

    pub fn collect_urls(&self) -> Result<Vec<url::Url>, AppError> {
        let mut raw = Vec::new();
        if let Some(u) = &self.url {
            raw.push(u.clone());
        }
        raw.extend(self.url_pos.iter().cloned());
        if let Some(path) = &self.input_file {
            raw.extend(read_url_list(path)?);
        }
        if raw.is_empty() {
            return Err(AppError::InvalidArgument(
                "a URL is required (positional, -u/--url, or -i/--input-file)".into(),
            ));
        }
        raw.into_iter().map(|s| validate_url(&s)).collect()
    }
}

pub fn read_url_list(path: &str) -> Result<Vec<String>, AppError> {
    let text = if path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(path).map_err(|e| {
            AppError::InvalidArgument(format!("could not read input file `{path}`: {e}"))
        })?
    };
    Ok(parse_url_list(&text))
}

pub fn parse_url_list(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
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
        assert_eq!(args.connections, None);
    }

    #[test]
    fn parses_positional_url() {
        let args =
            CommandLineArgs::from_args(&["rtget"], &["https://example.com/file.bin"]).unwrap();
        assert_eq!(args.url_pos, vec!["https://example.com/file.bin"]);
        let urls = args.collect_urls().unwrap();
        assert_eq!(urls[0].as_str(), "https://example.com/file.bin");
    }

    #[test]
    fn parses_multiple_positional_urls() {
        let args = CommandLineArgs::from_args(
            &["rtget"],
            &["https://example.com/a", "https://example.com/b"],
        )
        .unwrap();
        assert_eq!(args.collect_urls().unwrap().len(), 2);
    }

    #[test]
    fn errors_when_no_arguments() {
        let args = CommandLineArgs::from_args(&["rtget"], &[]).unwrap();
        assert!(args.collect_urls().is_err());
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

    #[test]
    fn parse_url_list_skips_comments() {
        let text = "# hello\nhttps://a.example/x\n\n  https://b.example/y  \n";
        assert_eq!(
            parse_url_list(text),
            vec!["https://a.example/x", "https://b.example/y"]
        );
    }
}
