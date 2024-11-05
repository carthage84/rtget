mod http;
mod ftp;

use reqwest::{Client, Url};
use crate::error::AppError;

// Downloader trait to manage downloading files from different protocols
pub trait Downloader {
    fn new() -> Self;
    async fn download_chunk(&self, url: &str, start: usize, end: usize) -> Result<(), AppError>;
    async fn get_total_file_size(&self, url: &str) -> Result<usize, AppError>;
    fn calculate_byte_ranges(connections: usize,total_file_size: usize) -> Vec<(usize, usize)>;
}

// FileDownloader struct to manage downloading files from different protocols
pub struct FileDownloader {
    client: Client,
}

// Implement Downloader for FileDownloader
impl Downloader for FileDownloader {
    // Create a new FileDownloader struct
    // Returns a new FileDownloader struct
    fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    // Download a chunk of a file from a URL
    // `start` and `end` are the start and end byte positions of the chunk to download
    // Returns an error if the URL is not valid or the protocol is not supported
    async fn download_chunk(&self, url: &str, start: usize, end: usize) -> Result<(), AppError> {
        let parsed_url = Url::parse(url).map_err(|e| AppError::UrlParseError(e.to_string()))?;
        // Check if the URL is valid and the protocol is supported
        match parsed_url.scheme() {
            "http" | "https" => Ok(http::download(&self.client, url, start, end).await?),
            "ftp" | "sftp" => Ok(ftp::download(&self.client, url, start, end).await?),
            _ => Err(AppError::UnsupportedProtocol),
        }
    }

    // Get the total size of a file from a URL
    // Returns an error if the URL is not valid or the protocol is not supported
    async fn get_total_file_size(&self, url: &str) -> Result<usize, AppError> {
        let parsed_url = Url::parse(url).map_err(|e| AppError::UrlParseError(e.to_string()))?;
        // Check if the URL is valid and the protocol is supported
        match parsed_url.scheme() {
            "http" | "https" => Ok(http::get_total_file_size(&self.client, url).await?),
            "ftp" | "sftp" => Ok(ftp::get_total_file_size(&self.client, url).await?),
            _ => Err(AppError::UnsupportedProtocol),
        }
    }

    // Calculate byte ranges for a file
    // `connections` is the number of concurrent connections to use
    // `total_file_size` is the total size of the file to download
    // Returns a vector of byte ranges
    fn calculate_byte_ranges(connections: usize,total_file_size: usize) -> Vec<(usize, usize)>{
        let chunk_size = (total_file_size + connections - 1) / connections;
        // Calculate byte ranges for the file
        let byte_ranges: Vec<_> = (0..connections)
            .map(|i| {
                // Calculate start and end byte positions for the chunk
                let start = i * chunk_size;
                let end = std::cmp::min(start + chunk_size - 1, total_file_size - 1);
                (start, end)
            })
            .collect();
        byte_ranges
    }
}
