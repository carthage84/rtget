use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Manages multiple progress bars for concurrent tasks.
pub struct ProgressManager {
    // Manages a collection of progress bars.
    multi_progress: MultiProgress,
    // Stores individual progress bars
    bars: Vec<ProgressBar>,
}

impl ProgressManager {
    /// Creates a new `ProgressManager`.
    ///
    /// Returns an instance of `ProgressManager` with no progress bars initially.
    pub fn new() -> ProgressManager {
        ProgressManager {
            multi_progress: MultiProgress::new(),
            bars: Vec::new()
        }
    }

    /// Creates and adds a new progress bar.
    ///
    /// `total_size` is the total size of the task for the new progress bar.
    /// Returns the index of the newly created progress bar.
    pub fn create_progress_bar(&mut self, total_size: u64) -> usize {
        let bar = self.multi_progress.add(ProgressBar::new(total_size));
        bar.set_style(ProgressStyle::default_bar()
            .template("{spinner.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {msg}")
            .unwrap()
            .progress_chars("#>-"));
        self.bars.push(bar);
        self.bars.len() - 1 // Return the index of the new bar
    }

    /// Updates the progress of a specific progress bar.
    ///
    /// `bar_index` specifies which progress bar to update.
    /// `progress` is the new progress value for the specified bar.
    pub fn update(&mut self, bar_index: usize, progress: u64) {
        if let Some(bar) = self.bars.get(bar_index) {
            bar.set_position(progress);
        }
    }

    /// Completes a progress bar and displays a final message.
    ///
    /// `bar_index` specifies which progress bar to finish.
    /// `msg` is the message to display upon completion.
    pub fn finish_with_message(&mut self, bar_index: usize, msg: &str) {
        if let Some(bar) = self.bars.get(bar_index) {
            bar.finish_with_message(msg.to_string());
        }
    }
}