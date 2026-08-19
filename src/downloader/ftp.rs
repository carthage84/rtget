use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use indicatif::ProgressBar;
use log::{debug, info, warn};
use suppaftp::tokio::{
    AsyncFtpStream, AsyncNativeTlsConnector, AsyncNativeTlsFtpStream, ImplAsyncFtpStream,
    TokioTlsStream,
};
use suppaftp::types::FileType;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::config::DownloadConfig;
use crate::error::AppError;
use crate::filesystem::{self, existing_len};
use crate::progress::ProgressManager;
use crate::proxy::{self, ProxyKind};
use crate::retry::with_retry;
use crate::url_validator::filename_from_url;

use super::range::calculate_byte_ranges;

const MIN_RANGE_SIZE: u64 = 64 * 1024;

pub async fn download(config: &DownloadConfig, progress: &ProgressManager) -> Result<(), AppError> {
    let filename = filename_from_url(&config.url);
    let output = config.output_path(&filename);
    filesystem::ensure_parent_dir(&output).await?;

    info!("FTP {} -> {}", config.url, output.display());

    let size = with_retry(config.tries, || async {
        let mut ftp = connect(config).await?;
        let size = ftp.size(&filename).await.ok().map(|s| s as u64);
        let _ = ftp.quit().await;
        Ok(size)
    })
    .await?;

    if let Some(size) = size {
        info!("Remote size: {size} bytes");
        if config.resume && output.exists() && existing_len(&output) >= size {
            info!("{} already complete", output.display());
            progress.finish_all(&output.display().to_string());
            return Ok(());
        }
    }

    let use_ranges = size.is_some_and(|s| s >= MIN_RANGE_SIZE * 2) && config.connections > 1;

    let result = if use_ranges {
        let total = size.unwrap();
        match download_concurrent(config, &filename, &output, total, progress).await {
            Err(err) => {
                warn!("segmented FTP download failed ({err}); retrying with one connection");
                download_single(config, &filename, &output, size, progress).await
            }
            Ok(()) => Ok(()),
        }
    } else {
        download_single(config, &filename, &output, size, progress).await
    };

    if result.is_ok() {
        progress.finish_all(&output.display().to_string());
    }
    result
}

enum FtpSession {
    Plain(AsyncFtpStream),
    Tls(AsyncNativeTlsFtpStream),
}

impl FtpSession {
    async fn size(&mut self, path: &str) -> Result<usize, AppError> {
        match self {
            FtpSession::Plain(s) => Ok(s.size(path).await?),
            FtpSession::Tls(s) => Ok(s.size(path).await?),
        }
    }

    async fn quit(&mut self) -> Result<(), AppError> {
        match self {
            FtpSession::Plain(s) => Ok(s.quit().await?),
            FtpSession::Tls(s) => Ok(s.quit().await?),
        }
    }
}

async fn connect(config: &DownloadConfig) -> Result<FtpSession, AppError> {
    let host = config.ftp_host()?;
    let port = config.ftp_port();
    let stream = proxy::connect_tcp(host, port, &config.proxy).await?;
    let proxy = config.proxy.clone();

    if config.url.scheme() == "ftps" {
        let ftp = AsyncNativeTlsFtpStream::connect_with_stream(stream)
            .await
            .map_err(AppError::from)?;
        let mut ftp = attach_passive_builder(ftp, proxy);
        let tls = tls_connector(config.insecure)?;
        ftp = ftp.into_secure(tls, host).await.map_err(AppError::from)?;
        login_and_prepare(&mut ftp, config).await?;
        Ok(FtpSession::Tls(ftp))
    } else {
        let ftp = AsyncFtpStream::connect_with_stream(stream)
            .await
            .map_err(AppError::from)?;
        let mut ftp = attach_passive_builder(ftp, proxy);
        login_and_prepare(&mut ftp, config).await?;
        Ok(FtpSession::Plain(ftp))
    }
}

fn tls_connector(insecure: bool) -> Result<AsyncNativeTlsConnector, AppError> {
    let mut builder = suppaftp::async_native_tls::TlsConnector::new();
    if insecure {
        builder = builder.danger_accept_invalid_certs(true);
        builder = builder.danger_accept_invalid_hostnames(true);
    }
    Ok(AsyncNativeTlsConnector::from(builder))
}

fn attach_passive_builder<T>(
    mut ftp: ImplAsyncFtpStream<T>,
    proxy: ProxyKind,
) -> ImplAsyncFtpStream<T>
where
    T: TokioTlsStream + Send,
{
    if matches!(proxy, ProxyKind::None) {
        ftp.set_passive_nat_workaround(true);
        return ftp;
    }
    let proxy = Arc::new(proxy);
    ftp.passive_stream_builder(move |addr: SocketAddr| {
        let proxy = Arc::clone(&proxy);
        Box::pin(async move {
            proxy::connect_addr(addr, &proxy).await.map_err(|e| {
                suppaftp::FtpError::ConnectionError(std::io::Error::other(e.to_string()))
            })
        }) as Pin<Box<dyn Future<Output = suppaftp::FtpResult<TcpStream>> + Send + Sync>>
    })
}

async fn login_and_prepare<T>(
    ftp: &mut ImplAsyncFtpStream<T>,
    config: &DownloadConfig,
) -> Result<(), AppError>
where
    T: TokioTlsStream + Send,
{
    let user = config.username.as_deref().unwrap_or("anonymous");
    let pass = config.password.as_deref().unwrap_or("rtget@example.com");
    ftp.login(user, pass).await?;
    ftp.transfer_type(FileType::Binary).await?;
    let path = config.ftp_path();
    if let Some(dir) = ftp_directory(&path) {
        ftp.cwd(dir).await?;
    }
    Ok(())
}

/// Parent directory of an FTP URL path, if the file is not in the login directory.
fn ftp_directory(path: &str) -> Option<&str> {
    let trimmed = path.trim_start_matches('/');
    match trimmed.rsplit_once('/') {
        Some((dir, name)) if !dir.is_empty() && !name.is_empty() => Some(dir),
        _ => None,
    }
}

async fn download_single(
    config: &DownloadConfig,
    remote_path: &str,
    output: &Path,
    size: Option<u64>,
    progress: &ProgressManager,
) -> Result<(), AppError> {
    let already = if config.resume {
        existing_len(output)
    } else {
        0
    };
    if let Some(size) = size
        && already >= size
    {
        return Ok(());
    }
    let remaining = size.map(|s| s.saturating_sub(already));
    let bar = progress.create_bar(remaining, "ftp");
    if already > 0 {
        info!("Resuming FTP download from byte {already}");
    }

    let remote_path = remote_path.to_string();
    with_retry(config.tries, || {
        let remote_path = remote_path.clone();
        let bar = bar.clone();
        async move { fetch_to_path(config, &remote_path, output, already, remaining, &bar).await }
    })
    .await?;
    bar.finish_with_message("done");
    Ok(())
}

async fn fetch_to_path(
    config: &DownloadConfig,
    remote_path: &str,
    output: &Path,
    start: u64,
    expected: Option<u64>,
    bar: &ProgressBar,
) -> Result<(), AppError> {
    let mut session = connect(config).await?;
    match &mut session {
        FtpSession::Plain(ftp) => {
            copy_retr(ftp, remote_path, output, start, expected, bar).await?;
        }
        FtpSession::Tls(ftp) => {
            copy_retr(ftp, remote_path, output, start, expected, bar).await?;
        }
    }
    let _ = session.quit().await;
    Ok(())
}

async fn copy_retr<T>(
    ftp: &mut ImplAsyncFtpStream<T>,
    remote_path: &str,
    output: &Path,
    start: u64,
    expected: Option<u64>,
    bar: &ProgressBar,
) -> Result<(), AppError>
where
    T: TokioTlsStream + Send,
{
    if start > 0 {
        ftp.resume_transfer(start as usize).await?;
    }
    let mut stream = ftp.retr_as_stream(remote_path).await?;
    let mut file = if start > 0 {
        filesystem::open_append(output).await?
    } else {
        filesystem::open_write_truncate(output).await?
    };
    copy_limited(&mut stream, &mut file, expected, bar).await?;
    drop(file);
    ftp.finalize_retr_stream(stream).await?;
    Ok(())
}

async fn copy_limited<R, W>(
    reader: &mut R,
    writer: &mut W,
    expected: Option<u64>,
    bar: &ProgressBar,
) -> Result<u64, AppError>
where
    R: AsyncRead + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut buf = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        if expected.is_some_and(|max| written >= max) {
            break;
        }
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let take = match expected {
            Some(max) => (max - written).min(n as u64) as usize,
            None => n,
        };
        writer.write_all(&buf[..take]).await?;
        written += take as u64;
        bar.inc(take as u64);
    }
    writer.flush().await?;
    Ok(written)
}

async fn download_concurrent(
    config: &DownloadConfig,
    remote_path: &str,
    output: &Path,
    total: u64,
    progress: &ProgressManager,
) -> Result<(), AppError> {
    let ranges = calculate_byte_ranges(config.connections, total);
    info!("FTP segmented download with {} connection(s)", ranges.len());

    let mut handles = Vec::new();
    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        let part = filesystem::part_path(output, index);
        let already = if config.resume {
            existing_len(&part)
        } else {
            0
        };
        let expected = end - start + 1;
        if already >= expected {
            continue;
        }
        if !config.resume && part.exists() {
            let _ = std::fs::remove_file(&part);
        }
        let cfg = config.clone();
        let remote_path = remote_path.to_string();
        let bar = progress.create_bar(
            Some(expected.saturating_sub(already)),
            &format!("part {}", index + 1),
        );
        handles.push(tokio::spawn(async move {
            with_retry(cfg.tries, || {
                let cfg = cfg.clone();
                let remote_path = remote_path.clone();
                let part = part.clone();
                let bar = bar.clone();
                async move {
                    let already = existing_len(&part);
                    fetch_range(&cfg, &remote_path, &part, start, end, already, &bar).await
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

async fn fetch_range(
    config: &DownloadConfig,
    remote_path: &str,
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
    debug!("FTP REST {from} RETR {remote_path} ({expected} bytes)");
    fetch_to_path(config, remote_path, part, from, Some(expected), bar).await?;
    bar.finish_with_message("done");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftp_directory_splits_nested_paths() {
        assert_eq!(ftp_directory("/gnu/wget/file.bin"), Some("gnu/wget"));
        assert_eq!(ftp_directory("/README"), None);
        assert_eq!(ftp_directory("/"), None);
        assert_eq!(ftp_directory(""), None);
    }
}
