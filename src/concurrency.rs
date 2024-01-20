use tokio::task;
use std::sync::Arc;

/// Download task struct
#[derive(Clone)]
struct DownloadTask {
    url: String,
    start: usize,
    end: usize,
}

impl DownloadTask {
    /// Creates a new download task.
    pub fn new(url: String, start: usize, end: usize) -> Self {
        DownloadTask { url, start, end }
    }

    /// Execute the download task
    async fn execute(&self) {

    }
}

/// Manage concurrent downloads
pub struct ConcurrentDownloader {
    tasks: Vec<DownloadTask>,
}

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

            /// Spawn an asynchronous task for each download task
            let handle = task::spawn(async move {
                task.execute().await;
            });

            handles.push(handle);
        }

        // Await all spawned tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }
    }
}