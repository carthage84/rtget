use std::path::Display;
use std::time::Duration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Manages multiple progress bars for concurrent tasks.
pub struct ProgressManager {
    // Manages a collection of progress bars.
    multi_progress: MultiProgress,
}

// Implement ProgressManager
// This is required to allow the progress bars to be updated and completed
impl ProgressManager {
    /// Creates a new `ProgressManager`.
    ///
    /// Returns an instance of `ProgressManager` with no progress bars initially.
    pub fn new() -> ProgressManager {
        ProgressManager {
            multi_progress: MultiProgress::new(),
        }
    }

    /// Creates and adds a new progress bar.
    ///
    /// `total_size` is the total size of the task for the new progress bar.
    /// Returns the index of the newly created progress bar.
    pub fn create_progress_bar(&mut self, total_size: u64, part: usize) -> ProgressBar {
        let bar = self.multi_progress.add(ProgressBar::new(total_size));
        bar.enable_steady_tick(Duration::from_millis(100));
        bar.set_style(ProgressStyle::default_bar()
            .template(&format!("[Part {}] {{spinner:.green}} [{{elapsed_precise}}] {{bar:60.green/blue}} {{percent}}% {{bytes}}/{{total_bytes}} [{{binary_bytes_per_sec}}] ({{eta}}) {{msg:.green}}", part + 1))
            .unwrap()
            .progress_chars("█▓▒░"));
        bar
    }

    pub fn finish_all(&self, filename: Display) {
        println!("Download complete: {} ", filename)
    }
}