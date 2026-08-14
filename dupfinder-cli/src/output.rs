//! Terminal output formatting with progress bars using indicatif.

use std::sync::Mutex;

use indicatif::{ProgressBar, ProgressStyle};

use dupfinder_core::progress::ProgressHandler;
use dupfinder_core::types::ScanPhase;

/// CLI progress handler that displays progress bars using indicatif.
pub struct CliProgressHandler {
    /// Whether progress output is suppressed.
    quiet: bool,
    /// Current active progress bar.
    current_bar: Mutex<Option<ProgressBar>>,
}

impl CliProgressHandler {
    pub fn new(quiet: bool) -> Self {
        Self {
            quiet,
            current_bar: Mutex::new(None),
        }
    }

    /// Finish and clear any active progress bar.
    pub fn finish(&self) {
        if let Ok(mut bar) = self.current_bar.lock() {
            if let Some(pb) = bar.take() {
                pb.finish_and_clear();
            }
        }
    }
}

impl ProgressHandler for CliProgressHandler {
    fn on_phase_start(&self, phase: ScanPhase, total: Option<u64>) {
        if self.quiet {
            return;
        }

        // Finish previous bar
        self.finish();

        let pb = if let Some(total) = total {
            let bar = ProgressBar::new(total);
            bar.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{bar:30.cyan/dim}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("█▓░"),
            );
            bar
        } else {
            let bar = ProgressBar::new_spinner();
            bar.set_style(ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
            bar
        };

        pb.set_message(format!("{}", phase));

        if let Ok(mut current) = self.current_bar.lock() {
            *current = Some(pb);
        }
    }

    fn on_progress(&self, _phase: ScanPhase, current: u64, message: &str) {
        if self.quiet {
            return;
        }

        if let Ok(bar) = self.current_bar.lock() {
            if let Some(ref pb) = *bar {
                pb.set_position(current);
                pb.set_message(message.to_string());
            }
        }
    }

    fn on_phase_end(&self, _phase: ScanPhase) {
        if self.quiet {
            return;
        }

        if let Ok(mut bar) = self.current_bar.lock() {
            if let Some(pb) = bar.take() {
                pb.finish_and_clear();
            }
        }
    }
}
