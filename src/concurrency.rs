use tokio::task;
use std::sync::Arc;
use crate::downloader::{Downloader, FileDownloader};

/// Download the task struct
#[derive(Clone)]
pub struct DownloadTask {
    url: String,
    start: usize,
    end: usize,
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
    pub fn new(url: String, start: usize, end: usize) -> Self {
        DownloadTask { url, start, end }
    }

    // Execute the download task
    async fn execute(url: String, start: usize, end: usize) -> Result<(), Box<dyn std::error::Error>> {
        let downloader = FileDownloader::new();
        match downloader.download_chunk(&url, start, end).await {
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
    pub async fn execute_all(&self) {
        let mut handles = vec![];

        for task in &self.tasks {
            let task = Arc::new(task.clone()); // Wrap the task in Arc
            let url = task.url.clone();
            let start = task.start;
            let end = task.end;

            // Spawn an asynchronous task for each download task
            let handle = task::spawn(async move {
                DownloadTask::execute(url, start, end).await.unwrap();
            });

            handles.push(handle);
        }

        // Await all spawned tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }
    }
}

/// Unit tests
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    // Mock version of DownloadTask for testing
    struct MockDownloadTask {
        url: String,
        start: usize,
        end: usize,
    }

    impl MockDownloadTask {
        fn new(url: String, start: usize, end: usize) -> Self {
            MockDownloadTask { url, start, end }
        }

        async fn execute(&self) {
            // Simulate a download task (e.g., a simple async delay)
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    #[test]
    fn test_execute_all_tasks() {
        let runtime = Runtime::new().unwrap(); // Create a Tokio runtime for the async test

        runtime.block_on(async {
            let tasks = vec![
                DownloadTask::new("https://example.com".to_string(), 0, 65536),
                DownloadTask::new("https://example.com".to_string(), 0, 65536),
                DownloadTask::new("https://example.com".to_string(), 0, 65536),
                DownloadTask::new("https://example.com".to_string(), 0, 65536),
                DownloadTask::new("https://example.com".to_string(), 0, 65536),
                DownloadTask::new("https://example.com".to_string(), 0, 65536),
            ];

            let downloader = ConcurrentDownloader::new(tasks);
            downloader.execute_all().await; // This runs the tasks

            // Assertions to check if tasks were executed
            // This might depend on whether your tasks modify some state or produce some output
        });
    }

    #[test]
    fn test_no_tasks() {
        let runtime = Runtime::new().unwrap();

        runtime.block_on(async {
            let downloader = ConcurrentDownloader::new(vec![]);
            downloader.execute_all().await; // No tasks to execute

            // Assertions to confirm no errors or panics occur when no tasks are present
        });
    }
}