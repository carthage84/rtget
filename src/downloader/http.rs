use std::path::Path;

use futures_util::StreamExt;
use indicatif::ProgressBar;
use log::{debug, info, warn};
use reqwest::header::{
    ACCEPT_ENCODING, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, HeaderMap,
    RANGE,
};
use reqwest::{Client, StatusCode, Url};
use tokio::io::AsyncWriteExt;

use crate::config::DownloadConfig;
use crate::error::AppError;
use crate::filesystem::{self, existing_len};
use crate::html_redirect::{
    extract_html_redirect, is_html_content_type, looks_like_html_bytes, url_looks_like_html,
};
use crate::progress::ProgressManager;
use crate::retry::with_retry;
use crate::url_validator::{filename_from_content_disposition, filename_from_url};

use super::range::calculate_byte_ranges;

const MIN_RANGE_SIZE: u64 = 64 * 1024;
const MAX_HTML_HOPS: u8 = 5;
const HTML_SNIFF_LIMIT: usize = 512 * 1024;

enum OnceOutcome {
    Complete,
    HtmlRedirect(Url),
}

pub async fn download(config: &DownloadConfig, progress: &ProgressManager) -> Result<(), AppError> {
    let client = build_client(config)?;
    let mut current = config.clone();
    for hop in 0..=MAX_HTML_HOPS {
        match download_once(&client, &current, progress).await? {
            OnceOutcome::Complete => return Ok(()),
            OnceOutcome::HtmlRedirect(next) => {
                if hop == MAX_HTML_HOPS {
                    return Err(AppError::Download(
                        "too many HTML download redirects".into(),
                    ));
                }
                if next == current.url {
                    return Err(AppError::Download(format!(
                        "server returned an HTML page instead of a file at {}",
                        current.url
                    )));
                }
                info!("Following HTML download redirect to {next}");
                current.url = next;
            }
        }
    }
    Err(AppError::Download(
        "too many HTML download redirects".into(),
    ))
}

async fn download_once(
    client: &Client,
    config: &DownloadConfig,
    progress: &ProgressManager,
) -> Result<OnceOutcome, AppError> {
    let meta = with_retry(config.tries, || probe(client, config)).await?;

    let filename = meta
        .filename
        .clone()
        .unwrap_or_else(|| filename_from_url(&meta.url));
    let output = config.output_path(&filename);
    filesystem::ensure_parent_dir(&output).await?;

    info!("Saving to {}", output.display());
    if let Some(size) = meta.size {
        info!("Remote size: {size} bytes");
    }
    if let Some(ct) = &meta.content_type {
        debug!("Content-Type: {ct}");
    }

    if config.resume
        && output.exists()
        && meta.size.is_some_and(|size| existing_len(&output) >= size)
        && !meta
            .content_type
            .as_deref()
            .is_some_and(is_html_content_type)
    {
        info!("{} already complete", output.display());
        progress.finish_all(&output.display().to_string());
        return Ok(OnceOutcome::Complete);
    }

    let use_ranges = meta.accept_ranges
        && config.connections > 1
        && meta.size.is_some_and(|s| s >= MIN_RANGE_SIZE * 2)
        && !meta
            .content_type
            .as_deref()
            .is_some_and(is_html_content_type);

    let result = if use_ranges {
        match download_concurrent(client, config, &meta, &output, progress).await {
            Err(AppError::Download(msg)) if msg.contains("range not honoured") => {
                warn!("{msg}; falling back to a single connection");
                download_single(client, config, &meta, &output, progress).await
            }
            Ok(()) => Ok(OnceOutcome::Complete),
            Err(err) => Err(err),
        }
    } else {
        if config.connections > 1 && !use_ranges {
            info!("Using a single connection (server does not support ranges or size is unknown)");
        }
        download_single(client, config, &meta, &output, progress).await
    };

    match result {
        Ok(OnceOutcome::Complete) => {
            progress.finish_all(&output.display().to_string());
            Ok(OnceOutcome::Complete)
        }
        other => other,
    }
}

struct RemoteMeta {
    url: Url,
    size: Option<u64>,
    accept_ranges: bool,
    filename: Option<String>,
    content_type: Option<String>,
}

fn build_client(config: &DownloadConfig) -> Result<Client, AppError> {
    let mut builder = Client::builder()
        .user_agent(&config.user_agent)
        .redirect(reqwest::redirect::Policy::limited(20))
        .danger_accept_invalid_certs(config.insecure);

    if let Some(timeout) = config.timeout {
        builder = builder.timeout(timeout).connect_timeout(timeout);
    } else {
        builder = builder.connect_timeout(std::time::Duration::from_secs(30));
    }

    if config.no_proxy {
        builder = builder.no_proxy();
    } else if let Some(proxy_url) = config.proxy.as_reqwest_url() {
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|e| AppError::Proxy(format!("invalid proxy `{proxy_url}`: {e}")))?;
        builder = builder.proxy(proxy);
    }

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT_ENCODING, HeaderValueConst::identity());
    headers.extend(config.extra_headers.clone());
    builder = builder.default_headers(headers);

    builder.build().map_err(AppError::from)
}

struct HeaderValueConst;
impl HeaderValueConst {
    fn identity() -> reqwest::header::HeaderValue {
        reqwest::header::HeaderValue::from_static("identity")
    }
}

async fn probe(client: &Client, config: &DownloadConfig) -> Result<RemoteMeta, AppError> {
    match try_head(client, config).await {
        Ok(meta) => return Ok(meta),
        Err(err) => {
            debug!("HEAD probe failed: {err}; trying a range GET");
        }
    }
    try_range_probe(client, config).await
}

async fn try_head(client: &Client, config: &DownloadConfig) -> Result<RemoteMeta, AppError> {
    let mut req = client.head(config.url.clone());
    req = apply_auth(req, config);
    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(AppError::Connect(format!(
            "HEAD {} returned {}",
            config.url,
            resp.status()
        )));
    }
    Ok(meta_from_response(&resp, false))
}

async fn try_range_probe(client: &Client, config: &DownloadConfig) -> Result<RemoteMeta, AppError> {
    let mut req = client.get(config.url.clone()).header(RANGE, "bytes=0-0");
    req = apply_auth(req, config);
    let resp = req.send().await?;
    if resp.status() == StatusCode::PARTIAL_CONTENT {
        return Ok(meta_from_response(&resp, true));
    }
    if resp.status().is_success() {
        let mut meta = meta_from_response(&resp, false);
        meta.accept_ranges = false;
        return Ok(meta);
    }
    Err(AppError::Connect(format!(
        "GET {} returned {}",
        config.url,
        resp.status()
    )))
}

fn meta_from_response(resp: &reqwest::Response, range_confirmed: bool) -> RemoteMeta {
    let accept_ranges = range_confirmed
        || resp
            .headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.to_ascii_lowercase().contains("bytes"));

    let size = content_length(resp).or_else(|| content_range_total(resp));
    let filename = resp
        .headers()
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_content_disposition);

    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    RemoteMeta {
        url: resp.url().clone(),
        size,
        accept_ranges,
        filename,
        content_type,
    }
}

fn content_length(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
}

fn content_range_total(resp: &reqwest::Response) -> Option<u64> {
    let header = resp.headers().get(CONTENT_RANGE)?.to_str().ok()?;
    // bytes 0-0/12345  or  bytes 0-99/*
    let total = header.rsplit('/').next()?;
    if total == "*" {
        None
    } else {
        total.parse().ok()
    }
}

fn apply_auth(req: reqwest::RequestBuilder, config: &DownloadConfig) -> reqwest::RequestBuilder {
    match (&config.username, &config.password) {
        (Some(user), pass) => req.basic_auth(user, pass.as_deref()),
        _ => req,
    }
}

async fn download_single(
    client: &Client,
    config: &DownloadConfig,
    meta: &RemoteMeta,
    output: &Path,
    progress: &ProgressManager,
) -> Result<OnceOutcome, AppError> {
    let already = if config.resume {
        existing_len(output)
    } else {
        0
    };
    if let Some(size) = meta.size
        && already >= size
        && !meta
            .content_type
            .as_deref()
            .is_some_and(is_html_content_type)
    {
        return Ok(OnceOutcome::Complete);
    }

    let remaining = meta.size.map(|s| s.saturating_sub(already));
    let bar = progress.create_bar(remaining, "file");
    if already > 0 {
        info!("Resuming from byte {already}");
    }

    let outcome = with_retry(config.tries, || {
        let bar = bar.clone();
        async move {
            let already = if config.resume {
                existing_len(output)
            } else {
                0
            };
            let remaining = meta.size.map(|s| s.saturating_sub(already));
            stream_to_file(client, config, &meta.url, output, already, remaining, &bar).await
        }
    })
    .await?;

    match outcome {
        OnceOutcome::Complete => {
            bar.finish_with_message("done");
            Ok(OnceOutcome::Complete)
        }
        other => Ok(other),
    }
}

async fn stream_to_file(
    client: &Client,
    config: &DownloadConfig,
    url: &Url,
    output: &Path,
    start: u64,
    expected: Option<u64>,
    bar: &ProgressBar,
) -> Result<OnceOutcome, AppError> {
    let mut req = client.get(url.clone());
    if start > 0 {
        req = req.header(RANGE, format!("bytes={start}-"));
    }
    req = apply_auth(req, config);
    let resp = req.send().await?;
    if !resp.status().is_success() && resp.status() != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::Connect(format!(
            "GET {url} returned {}",
            resp.status()
        )));
    }

    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let declared_html =
        content_type.as_deref().is_some_and(is_html_content_type) && !url_looks_like_html(url);

    if declared_html {
        let html = resp.text().await?;
        return html_outcome(&html, url);
    }

    let mut file = if start > 0 {
        filesystem::open_append(output).await?
    } else {
        filesystem::open_write_truncate(output).await?
    };

    let mut written = 0u64;
    let mut stream = resp.bytes_stream();
    let sniff = content_type.is_none() && start == 0 && !url_looks_like_html(url);
    let mut first = true;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if first && sniff && looks_like_html_bytes(&chunk) {
            return collect_html_redirect(chunk.to_vec(), stream, url).await;
        }
        first = false;
        file.write_all(&chunk).await?;
        written += chunk.len() as u64;
        bar.inc(chunk.len() as u64);
        if let Some(max) = expected
            && written >= max
        {
            break;
        }
    }
    file.flush().await?;
    if written == 0 {
        return Err(AppError::Download(format!("empty response from {url}")));
    }
    Ok(OnceOutcome::Complete)
}

fn html_outcome(html: &str, url: &Url) -> Result<OnceOutcome, AppError> {
    if let Some(next) = extract_html_redirect(html, url) {
        Ok(OnceOutcome::HtmlRedirect(next))
    } else {
        Err(AppError::Download(format!(
            "server returned an HTML page instead of a file at {url}"
        )))
    }
}

async fn collect_html_redirect<S>(
    mut buf: Vec<u8>,
    mut stream: S,
    url: &Url,
) -> Result<OnceOutcome, AppError>
where
    S: StreamExt<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buf.extend_from_slice(&chunk);
        if buf.len() >= HTML_SNIFF_LIMIT {
            break;
        }
    }
    let html = String::from_utf8_lossy(&buf);
    html_outcome(&html, url)
}

async fn download_concurrent(
    client: &Client,
    config: &DownloadConfig,
    meta: &RemoteMeta,
    output: &Path,
    progress: &ProgressManager,
) -> Result<(), AppError> {
    let total = meta.size.expect("range downloads require a known size");
    let ranges = calculate_byte_ranges(config.connections, total);
    info!("Downloading with {} connection(s)", ranges.len());

    let mut handles = Vec::new();
    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        let part = filesystem::part_path(output, index);
        let already = if config.resume {
            existing_len(&part)
        } else {
            0
        };
        let expected = end.saturating_sub(start) + 1;
        if already >= expected {
            debug!("part {index} already complete");
            continue;
        }
        if !config.resume && part.exists() {
            let _ = std::fs::remove_file(&part);
        }

        let client = client.clone();
        let url = meta.url.clone();
        let cfg = config.clone();
        let bar = progress.create_bar(
            Some(expected.saturating_sub(already)),
            &format!("part {}", index + 1),
        );

        handles.push(tokio::spawn(async move {
            with_retry(cfg.tries, || {
                let client = client.clone();
                let url = url.clone();
                let part = part.clone();
                let bar = bar.clone();
                let cfg = cfg.clone();
                async move {
                    let already = existing_len(&part);
                    download_range(&client, &cfg, &url, &part, start, end, already, &bar).await
                }
            })
            .await
        }));
    }

    for handle in handles {
        handle.await??;
    }

    filesystem::merge_parts(output, ranges.len()).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_range(
    client: &Client,
    config: &DownloadConfig,
    url: &Url,
    part: &Path,
    start: u64,
    end: u64,
    already: u64,
    bar: &ProgressBar,
) -> Result<(), AppError> {
    let from = start + already;
    if from > end {
        return Ok(());
    }
    let expected = end - from + 1;

    let mut req = client
        .get(url.clone())
        .header(RANGE, format!("bytes={from}-{end}"));
    req = apply_auth(req, config);
    let resp = req.send().await?;

    if resp.status() == StatusCode::OK {
        return Err(AppError::Download(
            "range not honoured (server returned HTTP 200)".into(),
        ));
    }
    if resp.status() != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::Connect(format!(
            "range request failed: {}",
            resp.status()
        )));
    }

    let mut file = filesystem::open_append(part).await?;
    let mut written = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let take = (expected - written).min(chunk.len() as u64) as usize;
        file.write_all(&chunk[..take]).await?;
        written += take as u64;
        bar.inc(take as u64);
        if written >= expected {
            break;
        }
    }
    file.flush().await?;
    if written != expected {
        return Err(AppError::Download(format!(
            "short read for {}-{end}: got {written}, expected {expected}",
            from
        )));
    }
    bar.finish_with_message("done");
    Ok(())
}

/// Tiny HTTP/1.1 server used by unit tests.
#[cfg(test)]
use std::io::Write;

#[cfg(test)]
pub async fn serve_static(
    listener: tokio::net::TcpListener,
    body: Vec<u8>,
    support_ranges: bool,
    support_head: bool,
) {
    loop {
        let Ok((mut sock, _)) = listener.accept().await else {
            break;
        };
        let body = body.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = vec![0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let mut lines = request.split("\r\n");
            let request_line = lines.next().unwrap_or("");
            let method = request_line.split_whitespace().next().unwrap_or("");
            let mut range: Option<(u64, u64)> = None;
            for line in lines {
                let l = line.to_ascii_lowercase();
                if let Some(rest) = l.strip_prefix("range: bytes=") {
                    let rest = rest.trim();
                    if let Some((a, b)) = rest.split_once('-')
                        && let Ok(start) = a.parse::<u64>()
                    {
                        let end = b
                            .parse::<u64>()
                            .unwrap_or((body.len() as u64).saturating_sub(1));
                        range = Some((start, end.min((body.len() as u64).saturating_sub(1))));
                    }
                }
            }

            let mut out = Vec::new();
            if method == "HEAD" {
                if !support_head {
                    write!(&mut out, "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").ok();
                } else {
                    write_headers(&mut out, body.len() as u64, support_ranges, None);
                }
            } else if method == "GET" {
                if let Some((start, end)) = range.filter(|_| support_ranges) {
                    let slice = &body[start as usize..=end as usize];
                    write!(&mut out, "HTTP/1.1 206 Partial Content\r\n").ok();
                    write!(&mut out, "Content-Length: {}\r\n", slice.len()).ok();
                    write!(
                        &mut out,
                        "Content-Range: bytes {start}-{end}/{}\r\n",
                        body.len()
                    )
                    .ok();
                    write!(
                        &mut out,
                        "Accept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    )
                    .ok();
                    out.extend_from_slice(slice);
                } else {
                    write_headers(&mut out, body.len() as u64, support_ranges, None);
                    out.extend_from_slice(&body);
                }
            } else {
                write!(
                    &mut out,
                    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n"
                )
                .ok();
            }
            let _ = sock.write_all(&out).await;
        });
    }
}

#[cfg(test)]
fn write_headers(out: &mut Vec<u8>, len: u64, ranges: bool, disposition: Option<&str>) {
    write!(out, "HTTP/1.1 200 OK\r\n").ok();
    write!(out, "Content-Length: {len}\r\n").ok();
    if ranges {
        write!(out, "Accept-Ranges: bytes\r\n").ok();
    }
    if let Some(d) = disposition {
        write!(out, "Content-Disposition: {d}\r\n").ok();
    }
    write!(out, "Connection: close\r\n\r\n").ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::ProgressManager;
    use crate::proxy::ProxyKind;
    use std::path::PathBuf;
    use tokio::net::TcpListener;

    fn test_config(url: Url, output: PathBuf, connections: usize, resume: bool) -> DownloadConfig {
        DownloadConfig {
            url,
            output: Some(output),
            connections,
            resume,
            timeout: Some(std::time::Duration::from_secs(5)),
            tries: 2,
            user_agent: "rtget-test".into(),
            extra_headers: HeaderMap::new(),
            proxy: ProxyKind::None,
            no_proxy: true,
            insecure: false,
            username: None,
            password: None,
            quiet: true,
        }
    }

    async fn spawn_server(
        body: Vec<u8>,
        ranges: bool,
        head: bool,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(serve_static(listener, body, ranges, head));
        (port, handle)
    }

    #[tokio::test]
    async fn single_connection_download() {
        let body = b"hello wget clone".to_vec();
        let (port, server) = spawn_server(body.clone(), false, true).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("file.txt");
        let url = Url::parse(&format!("http://127.0.0.1:{port}/file.txt")).unwrap();
        let config = test_config(url, output.clone(), 1, false);
        let progress = ProgressManager::new(true);
        download(&config, &progress).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        server.abort();
    }

    #[tokio::test]
    async fn concurrent_range_download() {
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 256) as u8).collect();
        let (port, server) = spawn_server(body.clone(), true, true).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("blob.bin");
        let url = Url::parse(&format!("http://127.0.0.1:{port}/blob.bin")).unwrap();
        let config = test_config(url, output.clone(), 4, false);
        let progress = ProgressManager::new(true);
        download(&config, &progress).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        server.abort();
    }

    #[tokio::test]
    async fn falls_back_when_ranges_ignored() {
        let body: Vec<u8> = (0..150_000u32).map(|i| (i % 251) as u8).collect();
        let (port, server) = spawn_server(body.clone(), false, true).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("blob.bin");
        let url = Url::parse(&format!("http://127.0.0.1:{port}/blob.bin")).unwrap();
        let config = test_config(url, output.clone(), 4, false);
        let progress = ProgressManager::new(true);
        download(&config, &progress).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        server.abort();
    }

    #[tokio::test]
    async fn resumes_partial_single_download() {
        let body = (0..8000u32).map(|i| (i % 200) as u8).collect::<Vec<_>>();
        let (port, server) = spawn_server(body.clone(), true, true).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("partial.bin");
        std::fs::write(&output, &body[..3000]).unwrap();
        let url = Url::parse(&format!("http://127.0.0.1:{port}/partial.bin")).unwrap();
        let config = test_config(url, output.clone(), 1, true);
        let progress = ProgressManager::new(true);
        download(&config, &progress).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        server.abort();
    }

    #[tokio::test]
    async fn follows_html_js_replace_redirect() {
        let body = b"FAKEMP4PAYLOAD".to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let html = format!(
            "<!DOCTYPE html><html><body><script>url=window.location.href.replace('/wp-content/storage/','/storage/token/'); window.location.replace(url);</script></body></html>"
        );
        let file_body = body.clone();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let html = html.clone();
                let file_body = file_body.clone();
                tokio::spawn(async move {
                    use std::io::Write as _;
                    use tokio::io::AsyncReadExt;
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let path = request
                        .lines()
                        .next()
                        .unwrap_or("")
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/");
                    let mut out = Vec::new();
                    if path.contains("/storage/token/") {
                        write!(&mut out, "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", file_body.len()).ok();
                        out.extend_from_slice(&file_body);
                    } else {
                        write!(&mut out, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", html.len()).ok();
                        out.extend_from_slice(html.as_bytes());
                    }
                    let _ = sock.write_all(&out).await;
                });
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("clip.mp4");
        let url = Url::parse(&format!(
            "http://127.0.0.1:{port}/wp-content/storage/2017/04/clip.mp4"
        ))
        .unwrap();
        let config = test_config(url, output.clone(), 1, false);
        let progress = ProgressManager::new(true);
        download(&config, &progress).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        server.abort();
    }

    #[tokio::test]
    async fn works_without_head() {
        let body = b"no-head-body".to_vec();
        let (port, server) = spawn_server(body.clone(), true, false).await;
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("file.bin");
        let url = Url::parse(&format!("http://127.0.0.1:{port}/file.bin")).unwrap();
        let config = test_config(url, output.clone(), 1, false);
        let progress = ProgressManager::new(true);
        download(&config, &progress).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        server.abort();
    }
}
