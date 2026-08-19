use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use url::Url;

use crate::args::CommandLineArgs;
use crate::checksum::{self, Checksum};
use crate::cookies::CookieJar;
use crate::error::AppError;
use crate::filesystem;
use crate::proxy::ProxyKind;
use crate::rate::{self, RateLimiter};

const DEFAULT_CONNECTIONS: usize = 4;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_TRIES: u32 = 5;
const DEFAULT_MAX_REDIRECT: usize = 20;

/// Optional keys from a TOML config file. CLI flags override these.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub proxy: Option<String>,
    pub connections: Option<u8>,
    pub timeout: Option<u64>,
    pub tries: Option<u32>,
    pub user_agent: Option<String>,
    pub insecure: Option<bool>,
    pub no_proxy: Option<bool>,
    pub limit_rate: Option<String>,
    pub directory_prefix: Option<String>,
    pub max_redirect: Option<usize>,
    pub no_clobber: Option<bool>,
}

impl FileConfig {
    pub fn load(args: &CommandLineArgs) -> Result<Self, AppError> {
        if args.no_config {
            return Ok(Self::default());
        }
        let path = match &args.config {
            Some(p) => PathBuf::from(p),
            None => match default_config_path() {
                Some(p) if p.is_file() => p,
                _ => return Ok(Self::default()),
            },
        };
        load_path(&path)
    }
}

fn load_path(path: &Path) -> Result<FileConfig, AppError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        AppError::InvalidArgument(format!("could not read config {}: {e}", path.display()))
    })?;
    toml::from_str(&text)
        .map_err(|e| AppError::InvalidArgument(format!("invalid config {}: {e}", path.display())))
}

pub fn default_config_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("rtget").join("config.toml"))
    }
    #[cfg(not(windows))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            Some(PathBuf::from(xdg).join("rtget").join("config.toml"))
        } else {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/rtget/config.toml"))
        }
    }
}

/// Fully resolved download settings for one URL.
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
    pub no_clobber: bool,
    pub explicit_output: bool,
    pub directory_prefix: Option<PathBuf>,
    pub rate_limit: Option<Arc<RateLimiter>>,
    pub cookie_jar: Option<Arc<CookieJar>>,
    pub spider: bool,
    pub max_redirect: usize,
    pub checksum: Option<Checksum>,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            url: Url::parse("http://127.0.0.1/").expect("static URL"),
            output: None,
            connections: DEFAULT_CONNECTIONS,
            resume: false,
            timeout: Some(Duration::from_secs(DEFAULT_TIMEOUT_SECS)),
            tries: DEFAULT_TRIES,
            user_agent: format!("rtget/{}", env!("CARGO_PKG_VERSION")),
            extra_headers: HeaderMap::new(),
            proxy: ProxyKind::None,
            no_proxy: false,
            insecure: false,
            username: None,
            password: None,
            quiet: false,
            no_clobber: false,
            explicit_output: false,
            directory_prefix: None,
            rate_limit: None,
            cookie_jar: None,
            spider: false,
            max_redirect: DEFAULT_MAX_REDIRECT,
            checksum: None,
        }
    }
}

impl DownloadConfig {
    pub fn from_args_and_file(
        args: &CommandLineArgs,
        file: &FileConfig,
        url: Url,
    ) -> Result<Self, AppError> {
        let extra_headers = parse_headers(&args.header)?;
        let user = args.user.clone();
        let password = args.password.clone();
        let (username, password) = credentials(&url, user.as_deref(), password.as_deref());

        let connections = args
            .connections()?
            .or(file.connections.map(|n| n as usize))
            .unwrap_or(DEFAULT_CONNECTIONS);
        if !(1..=64).contains(&connections) {
            return Err(AppError::InvalidArgument(
                "connections must be between 1 and 64".into(),
            ));
        }

        let timeout = args.timeout().unwrap_or_else(|| match file.timeout {
            Some(0) => None,
            Some(s) => Some(Duration::from_secs(s)),
            None => Some(Duration::from_secs(DEFAULT_TIMEOUT_SECS)),
        });

        let tries = args.tries().or(file.tries).unwrap_or(DEFAULT_TRIES).max(1);
        let user_agent = args
            .user_agent
            .clone()
            .or_else(|| file.user_agent.clone())
            .unwrap_or_else(|| format!("rtget/{}", env!("CARGO_PKG_VERSION")));

        let no_proxy = args.no_proxy || file.no_proxy.unwrap_or(false);
        let proxy_raw = if args.no_proxy {
            None
        } else {
            args.proxy.as_deref().or(file.proxy.as_deref())
        };
        let proxy = ProxyKind::from_url(proxy_raw, no_proxy)?;

        let directory_prefix = args
            .directory_prefix
            .as_ref()
            .or(file.directory_prefix.as_ref())
            .map(PathBuf::from);

        let output = args.output_hint();
        let explicit_output = output.as_ref().is_some_and(|p| !filesystem::dir_hint(p));

        let rate_raw = args.limit_rate.as_deref().or(file.limit_rate.as_deref());
        let rate_limit = match rate_raw {
            Some(s) => Some(Arc::new(RateLimiter::new(rate::parse_rate(s)?))),
            None => None,
        };

        let checksum = args
            .checksum
            .as_deref()
            .map(checksum::parse_checksum)
            .transpose()?;

        let max_redirect = args
            .max_redirect
            .or(file.max_redirect)
            .unwrap_or(DEFAULT_MAX_REDIRECT);

        let no_clobber = args.no_clobber || file.no_clobber.unwrap_or(false);
        let insecure = args.no_check_certificate || file.insecure.unwrap_or(false);

        Ok(Self {
            url,
            output,
            connections,
            resume: args.resume,
            timeout,
            tries,
            user_agent,
            extra_headers,
            proxy,
            no_proxy,
            insecure,
            username,
            password,
            quiet: args.quiet || args.background,
            no_clobber,
            explicit_output,
            directory_prefix,
            rate_limit,
            cookie_jar: None,
            spider: args.spider,
            max_redirect,
            checksum,
        })
    }

    pub fn with_url(&self, url: Url) -> Self {
        let (username, password) =
            credentials(&url, self.username.as_deref(), self.password.as_deref());
        let mut cfg = self.clone();
        cfg.url = url;
        cfg.username = username;
        cfg.password = password;
        cfg
    }

    pub fn output_path(&self, suggested: &str) -> PathBuf {
        let named = match &self.output {
            Some(path) if filesystem::dir_hint(path) => path.join(suggested),
            Some(path) => path.clone(),
            None => PathBuf::from(suggested),
        };
        if named.is_absolute() {
            return named;
        }
        match &self.directory_prefix {
            Some(prefix) => prefix.join(named),
            None => named,
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

pub fn prompt_password() -> Result<String, AppError> {
    rpassword::prompt_password("Password: ")
        .map_err(|e| AppError::InvalidArgument(format!("could not read password: {e}")))
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
    use argh::FromArgs;

    #[test]
    fn parses_custom_headers() {
        let map = parse_headers(&["Accept: text/plain".into(), "X-Test: 1".into()]).unwrap();
        assert_eq!(map.get("accept").unwrap(), "text/plain");
        assert_eq!(map.get("x-test").unwrap(), "1");
    }

    #[test]
    fn file_config_fills_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
connections = 8
timeout = 12
tries = 2
user_agent = "rtget-cfg"
limit_rate = "100k"
no_clobber = true
"#,
        )
        .unwrap();
        let file = load_path(&path).unwrap();
        let args = CommandLineArgs::from_args(&["rtget"], &["http://example.com/a.bin"]).unwrap();
        let cfg = DownloadConfig::from_args_and_file(
            &args,
            &file,
            args.collect_urls().unwrap().remove(0),
        )
        .unwrap();
        assert_eq!(cfg.connections, 8);
        assert_eq!(cfg.timeout, Some(Duration::from_secs(12)));
        assert_eq!(cfg.tries, 2);
        assert_eq!(cfg.user_agent, "rtget-cfg");
        assert!(cfg.rate_limit.is_some());
        assert!(cfg.no_clobber);
    }

    #[test]
    fn cli_overrides_file_config() {
        let file = FileConfig {
            connections: Some(8),
            user_agent: Some("from-file".into()),
            ..FileConfig::default()
        };
        let args = CommandLineArgs::from_args(
            &["rtget"],
            &["http://example.com/a.bin", "-c", "2", "-U", "from-cli"],
        )
        .unwrap();
        let cfg = DownloadConfig::from_args_and_file(
            &args,
            &file,
            args.collect_urls().unwrap().remove(0),
        )
        .unwrap();
        assert_eq!(cfg.connections, 2);
        assert_eq!(cfg.user_agent, "from-cli");
    }

    #[test]
    fn directory_prefix_joins_suggested_name() {
        let cfg = DownloadConfig {
            directory_prefix: Some(PathBuf::from("dl")),
            ..DownloadConfig::default()
        };
        assert_eq!(
            cfg.output_path("file.bin"),
            PathBuf::from("dl").join("file.bin")
        );
    }
}
