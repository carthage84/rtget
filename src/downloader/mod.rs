mod http;
mod ftp;

use std::path::{Path, PathBuf};
use indicatif::ProgressBar;
use log::debug;
use reqwest::{Client, Url};
use crate::args::CommandLineArgs;
use crate::concurrency::{DownloadTask};
use crate::error::AppError;

// Downloader trait to manage to download files from different protocols
pub trait Downloader {
    fn new() -> Self;
    async fn download_chunk(
        &self,
        url: &str,
        start: usize,
        end: usize,
        index: usize,
        file_path: &Path,
        progress: ProgressBar,
        byte_ranges: Vec<(u64, u64)>,
    ) -> Result<(), AppError>;
    async fn get_total_file_size(&self, url: &str) -> Result<usize, AppError>;
    fn calculate_byte_ranges(connections: usize,total_file_size: usize) -> Vec<(usize, usize)>;
    async fn calculate_download_chunks(&self, args: CommandLineArgs) -> Result<Vec<DownloadTask>, AppError>;
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
    async fn download_chunk(
        &self,
        url: &str,
        start: usize,
        end: usize,
        index: usize,
        file_path: &Path,
        progress: ProgressBar,
        byte_ranges: Vec<(u64, u64)>,
    ) -> Result<(), AppError> {
        let parsed_url = Url::parse(url).map_err(|e| AppError::UrlParseError(e.to_string()))?;
        match parsed_url.scheme() {
            "http" | "https" => Ok(http::download(
                &self.client,
                url,
                start,
                end,
                index,
                file_path,
                progress,
                byte_ranges.into_iter().map(|(start, end)| (start as usize, end as usize)).collect(),
            )
                .await?),
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
        debug!("Byte ranges: {:?}", byte_ranges);
        byte_ranges
    }

    // Calculate download chunks for a file
    // `args` is the command line arguments
    // Returns a vector of download tasks
    async fn calculate_download_chunks(&self, args: CommandLineArgs) -> Result<Vec<DownloadTask>, AppError> {
        let total_size = self.get_total_file_size(&args.url).await?;
        debug!("Total size: {}", total_size);
        let byte_ranges = Self::calculate_byte_ranges(args.connections as usize, total_size);
        let output_path = match args.output {
            Some(output) => PathBuf::from(output),
            None => {
                let url = Url::parse(&args.url)
                    .map_err(|e| AppError::UrlParseError(e.to_string()))?;
                let path = url.path();
                let filename = Path::new(path)
                    .file_name()
                    .ok_or_else(|| AppError::CouldNotConnect("Could not derive filename from URL".to_string()))?;
                PathBuf::from(filename)
            }
        };
        
        let tasks: Vec<_> = byte_ranges
            .into_iter()
            .enumerate()
            .map(|(index, (start, end))| DownloadTask {
                url: args.url.clone(),
                start,
                end,
                index,
                file_path: output_path.clone(),
            })
            .collect();
        //println!("Created {} tasks", tasks.len());
        Ok(tasks)
    }
}
