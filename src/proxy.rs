use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_socks::tcp::Socks5Stream;
use url::Url;

use crate::error::AppError;

/// How traffic should be sent to a remote host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyKind {
    None,
    Http(Url),
    Socks5 { url: Url, remote_dns: bool },
}

impl ProxyKind {
    pub fn from_url(raw: Option<&str>, no_proxy: bool) -> Result<Self, AppError> {
        if no_proxy {
            return Ok(ProxyKind::None);
        }
        let Some(raw) = raw.filter(|s| !s.is_empty()) else {
            return Ok(ProxyKind::None);
        };
        let url =
            Url::parse(raw).map_err(|e| AppError::Proxy(format!("invalid proxy URL: {e}")))?;
        match url.scheme() {
            "http" | "https" => Ok(ProxyKind::Http(url)),
            "socks5" => Ok(ProxyKind::Socks5 {
                url,
                remote_dns: false,
            }),
            "socks5h" | "socks" => Ok(ProxyKind::Socks5 {
                url,
                remote_dns: true,
            }),
            other => Err(AppError::Proxy(format!(
                "unsupported proxy scheme `{other}` (use http, https, socks5, or socks5h)"
            ))),
        }
    }

    pub fn as_reqwest_url(&self) -> Option<String> {
        match self {
            ProxyKind::None => None,
            ProxyKind::Http(url) | ProxyKind::Socks5 { url, .. } => Some(url.to_string()),
        }
    }
}

/// Open a TCP connection to `host:port`, optionally tunnelling through a proxy.
pub async fn connect_tcp(host: &str, port: u16, proxy: &ProxyKind) -> Result<TcpStream, AppError> {
    match proxy {
        ProxyKind::None => TcpStream::connect((host, port))
            .await
            .map_err(|e| AppError::Connect(format!("{host}:{port}: {e}"))),
        ProxyKind::Socks5 { url, remote_dns } => socks_connect(url, host, port, *remote_dns).await,
        ProxyKind::Http(url) => http_connect(url, host, port).await,
    }
}

async fn socks_connect(
    proxy: &Url,
    host: &str,
    port: u16,
    remote_dns: bool,
) -> Result<TcpStream, AppError> {
    let proxy_host = proxy
        .host_str()
        .ok_or_else(|| AppError::Proxy("SOCKS proxy is missing a host".into()))?;
    let proxy_port = proxy.port_or_known_default().unwrap_or(1080);
    let username = percent_decode_opt(proxy.username());
    let password = proxy.password().map(percent_decode_owned);

    let dest = if remote_dns {
        format!("{host}:{port}")
    } else {
        // Resolve locally, then ask the proxy to connect to the IP.
        let addr = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| AppError::Connect(format!("DNS lookup failed for {host}: {e}")))?
            .next()
            .ok_or_else(|| AppError::Connect(format!("no addresses for {host}")))?;
        addr.to_string()
    };

    let stream = if let (Some(user), Some(pass)) = (username.as_deref(), password.as_deref()) {
        Socks5Stream::connect_with_password((proxy_host, proxy_port), dest.as_str(), user, pass)
            .await
    } else {
        Socks5Stream::connect((proxy_host, proxy_port), dest.as_str()).await
    }
    .map_err(|e| AppError::Proxy(format!("SOCKS connect failed: {e}")))?;

    Ok(stream.into_inner())
}

async fn http_connect(proxy: &Url, host: &str, port: u16) -> Result<TcpStream, AppError> {
    let proxy_host = proxy
        .host_str()
        .ok_or_else(|| AppError::Proxy("HTTP proxy is missing a host".into()))?;
    let proxy_port = proxy
        .port_or_known_default()
        .ok_or_else(|| AppError::Proxy("HTTP proxy is missing a port".into()))?;

    let mut stream = TcpStream::connect((proxy_host, proxy_port))
        .await
        .map_err(|e| AppError::Proxy(format!("could not reach HTTP proxy: {e}")))?;

    let mut request = format!(
        "CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\nProxy-Connection: Keep-Alive\r\n"
    );
    if !proxy.username().is_empty() {
        let user = percent_decode_owned(proxy.username());
        let pass = proxy.password().unwrap_or("");
        let token = base64_encode(format!("{user}:{pass}").as_bytes());
        request.push_str(&format!("Proxy-Authorization: Basic {token}\r\n"));
    }
    request.push_str("\r\n");

    stream.write_all(request.as_bytes()).await?;

    let mut buf = Vec::with_capacity(512);
    let mut tmp = [0u8; 256];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(AppError::Proxy(
                "HTTP proxy closed the CONNECT tunnel".into(),
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            return Err(AppError::Proxy(
                "HTTP proxy CONNECT response too large".into(),
            ));
        }
    }

    let header_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(buf.len());
    let header = String::from_utf8_lossy(&buf[..header_end]);
    let status_line = header.lines().next().unwrap_or("");
    // HTTP/1.1 200 Connection established
    let ok = status_line.split_whitespace().nth(1) == Some("200");
    if !ok {
        return Err(AppError::Proxy(format!(
            "HTTP CONNECT failed: {status_line}"
        )));
    }
    Ok(stream)
}

fn percent_decode_opt(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(percent_decode_owned(s))
    }
}

fn percent_decode_owned(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied();
        let b2 = bytes.get(i + 2).copied();
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[((b0 & 0x03) << 4 | b1.unwrap_or(0) >> 4) as usize] as char);
        match b1 {
            Some(b1) => out.push(TABLE[((b1 & 0x0f) << 2 | b2.unwrap_or(0) >> 6) as usize] as char),
            None => out.push('='),
        }
        match b2 {
            Some(b2) => out.push(TABLE[(b2 & 0x3f) as usize] as char),
            None => out.push('='),
        }
        i += 3;
    }
    out
}

/// Helper used by FTP PASV connections.
pub async fn connect_addr(addr: SocketAddr, proxy: &ProxyKind) -> Result<TcpStream, AppError> {
    match addr {
        SocketAddr::V4(v4) => connect_tcp(&v4.ip().to_string(), v4.port(), proxy).await,
        SocketAddr::V6(v6) => connect_tcp(&v6.ip().to_string(), v6.port(), proxy).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_and_socks_proxy() {
        let http = ProxyKind::from_url(Some("http://127.0.0.1:8080"), false).unwrap();
        assert!(matches!(http, ProxyKind::Http(_)));
        let socks = ProxyKind::from_url(Some("socks5://127.0.0.1:1080"), false).unwrap();
        assert!(matches!(
            socks,
            ProxyKind::Socks5 {
                remote_dns: false,
                ..
            }
        ));
        let socks_h = ProxyKind::from_url(Some("socks5h://127.0.0.1:1080"), false).unwrap();
        assert!(matches!(
            socks_h,
            ProxyKind::Socks5 {
                remote_dns: true,
                ..
            }
        ));
    }

    #[test]
    fn no_proxy_disables_everything() {
        let kind = ProxyKind::from_url(Some("http://127.0.0.1:8080"), true).unwrap();
        assert_eq!(kind, ProxyKind::None);
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(ProxyKind::from_url(Some("ftp://proxy"), false).is_err());
    }

    #[test]
    fn base64_encodes_proxy_auth() {
        assert_eq!(
            base64_encode(b"Aladdin:open sesame"),
            "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }
}
