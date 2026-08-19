use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// Manages one or more download progress bars.
pub struct ProgressManager {
    multi: MultiProgress,
    quiet: bool,
}

impl ProgressManager {
    pub fn new(quiet: bool) -> Self {
        Self {
            multi: MultiProgress::new(),
            quiet,
        }
    }

    pub fn create_bar(&self, total_size: Option<u64>, label: &str) -> ProgressBar {
        let bar = if self.quiet {
            ProgressBar::hidden()
        } else {
            match total_size {
                Some(n) => self.multi.add(ProgressBar::new(n)),
                None => self.multi.add(ProgressBar::new_spinner()),
            }
        };

        bar.enable_steady_tick(Duration::from_millis(120));
        let template = if total_size.is_some() {
            format!(
                "{label} {{spinner:.green}} [{{elapsed_precise}}] {{bar:40.cyan/blue}} {{bytes}}/{{total_bytes}} ({{bytes_per_sec}}, {{eta}}) {{msg}}"
            )
        } else {
            format!(
                "{label} {{spinner:.green}} [{{elapsed_precise}}] {{bytes}} ({{bytes_per_sec}}) {{msg}}"
            )
        };
        if let Ok(style) = ProgressStyle::with_template(&template) {
            bar.set_style(style.progress_chars("=>-"));
        }
        bar
    }

    pub fn finish_all(&self, filename: &str) {
        if !self.quiet {
            let _ = self.multi.println(format!("Download complete: {filename}"));
        }
    }
}
