use std::path::PathBuf;
use std::sync::{Arc};
use futures_util::future::join_all;
use indicatif::ProgressBar;
use crate::downloader::{Downloader, FileDownloader};
use crate::error::AppError;
use crate::progress::ProgressManager;

/// Download the task struct
#[derive(Clone)]
pub struct DownloadTask {
    pub url: String,
    pub start: usize,
    pub end: usize,
    pub index: usize,
    pub file_path: PathBuf,
}

/// Download a file concurrently
///
/// # Arguments
///
/// * `url` - The URL of the file to download
/// * `start` - The start byte of the file to download
/// * `end` - The end byte of the file to download
/// 
impl DownloadTask {
    // Creates a new download task.
    pub fn new(url: String, start: usize, end: usize, index: usize, file_path: PathBuf) -> Self {
        DownloadTask { url, start, end, index, file_path }
    }

    // Execute the download task
    async fn execute(url: String, start: usize, end: usize, index: usize, file_path: PathBuf, progress: ProgressBar, byte_ranges: Vec<(u64, u64)>) -> Result<(), Box<dyn std::error::Error>> {
        let downloader = FileDownloader::new();
        match downloader.download_chunk(
            &url,
            start,
            end,
            index,
            &*file_path,
            progress,
            byte_ranges.clone(),
        ).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}

/// Download multiple download tasks concurrently
///
/// # Arguments
///
/// * `tasks` - The download tasks to execute concurrently
///
pub struct ConcurrentDownloader {
    tasks: Vec<DownloadTask>,
}

/// Execute all download tasks concurrently
///
/// # Arguments
///
/// * `tasks` - The download tasks to execute concurrently
///
impl ConcurrentDownloader {
    /// Creates a new `ConcurrentDownloader` with specified tasks.
    pub fn new(tasks: Vec<DownloadTask>) -> Self {
        ConcurrentDownloader { tasks }
    }

    /// Execute all download tasks concurrently.
    pub async fn execute_all(
        &self,
        progress_manager: &mut ProgressManager,
        byte_ranges: Vec<(u64, u64)>,
    ) -> Result<(), AppError> {
        // Wrap FileDownloader in Arc for sharing across tasks
        let downloader = Arc::new(FileDownloader::new());
        let mut handles = vec![];

        for (i, task) in self.tasks.iter().enumerate() {
            let url = task.url.clone();
            let file_path = task.file_path.clone();
            let start = task.start;
            let end = task.end;
            let index = task.index;
            let byte_ranges = byte_ranges.clone();
            let downloader = Arc::clone(&downloader);
            let progress = progress_manager.create_progress_bar((end - start + 1) as u64, index);

            //println!("Spawning task {}: bytes={}-{}", index, start, end);
            let handle = tokio::spawn(async move {
                downloader.download_chunk(
                    &url,
                    start,
                    end,
                    index,
                    &file_path,
                    progress,
                    byte_ranges,
                )
                    .await
            });
            handles.push(handle);
        }

        // Await all tasks concurrently
        let results = join_all(handles).await;
        for result in results {
            result??; // Propagate JoinError or AppError
        }

        Ok(())
    }
}
