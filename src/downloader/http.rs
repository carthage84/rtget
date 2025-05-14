use std::path::Path;
use std::result::Result;
use reqwest::Client;
use crate::error::AppError;
use futures_util::stream::StreamExt;
use indicatif::ProgressBar;
use crate::filesystem::FileSystem;
use log::{debug, info};

// Download a file from an HTTP URL
// Returns an error message if the download failed
pub async fn download(
    client: &Client,
    url: &str,
    start: usize,
    end: usize,
    index: usize,
    file_path: &Path,
    progress: ProgressBar,
    byte_ranges: Vec<(usize, usize)>,
) -> Result<(), AppError> {
    debug!("Starting download for chunk {}: bytes={}-{}", index, start, end);
    let expected_size = (end - start + 1) as u64;
    let part_start = start as u64;
    let response = client
        .get(url)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await
        .map_err(|e| AppError::CouldNotConnect(e.to_string()))?;

    if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let content_range = response
            .headers()
            .get("Content-Range")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::CouldNotConnect("Missing Content-Range header".to_string()))?;
        let expected_range = format!("bytes {}-{}/", start, end);
        if !content_range.starts_with(&expected_range) {
            return Err(AppError::CouldNotConnect(format!(
                "Invalid Content-Range: got {}, expected {}*",
                content_range, expected_range
            )));
        }
        debug!("Content-Range validated: {}", content_range);

        let mut stream = response.bytes_stream();
        let part_file_path = file_path.with_file_name(format!("{}_part_{}", file_path.display(), index));
        debug!("Writing to a partial file: {}", part_file_path.display());
        let mut filesystem = FileSystem::new(&part_file_path, byte_ranges.clone());
        let file = filesystem.create_file().await?;
        let mut offset = part_start;
        let mut total_downloaded = 0;
        let mut total_written = 0;
        let mut chunk_count = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| AppError::CouldNotConnect(e.to_string()))?;
            chunk_count += 1;
            let chunk_size = chunk.len() as u64;
            total_downloaded += chunk_size;
            debug!(
                "Part {}: chunk {}, total_downloaded = {}, total_written = {}, expected_size = {}",
                index, chunk_count, total_downloaded, total_written, expected_size
            );

            let remaining = expected_size - total_written; // Fix: Use total_written
            let write_size = chunk_size.min(remaining);
            let write_chunk = &chunk[..write_size as usize];

            debug!(
                "Chunk {} for part {}: {} bytes (writing: {}) at offset {}",
                chunk_count, index, chunk_size, write_size, offset
            );
            let written = file.write_chunk(write_chunk, offset, part_start, expected_size).await.map_err(|e| {
                info!("Write chunk error for part {}: {}", index, e);
                AppError::CouldNotConnect(e.to_string())
            })?;
            total_written += written as u64;
            if written > 0 {
                progress.inc(written as u64);
            }
            offset += written as u64;

            if total_written >= expected_size {
                debug!("Stopping download for part {}: reached expected written size {}", index, expected_size);
                break;
            }
        }

        progress.finish_with_message(format!("Part {} complete", index + 1));
        debug!(
            "Completed download for chunk {}: {} chunks, {} bytes downloaded, {} bytes written (expected: {})",
            index, chunk_count, total_downloaded, total_written, expected_size
        );
        if total_written != expected_size {
            return Err(AppError::CouldNotConnect(format!(
                "Written size {} does not match expected {} for chunk {}",
                total_written, expected_size, index
            )));
        }
        Ok(())
    } else {
        Err(AppError::CouldNotConnect(format!("Request failed: {}", response.status())))
    }
}

// Get the total file size from the HTTP response headers
// Returns the total file size in bytes as an usize or an error message if the size could not be parsed
pub async fn get_total_file_size(client: &Client, url: &str) -> Result<usize, AppError> {
    // Perform HTTP request
    match client.head(url).send().await {
        // If the request was successful,
        // parse the content length header and return the size in bytes
        Ok(response) => {
            // If the request was successful,
            // parse the content length header and return the size in bytes
            if response.status().is_success() {
                // Get the content length header value as a string
                response
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .ok_or(AppError::CouldNotConnect("Could not parse content length".to_string()))
            } else {
                // If the request was not successful, return an error message
                Err(AppError::CouldNotConnect(response.status().to_string()))?
            }
        }
        // If the request was not successful, return an error message
        Err(e) => Err(AppError::CouldNotConnect(e.to_string()))?,
    }
}