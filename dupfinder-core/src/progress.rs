//! Progress reporting trait for scan operations.

use crate::types::ScanPhase;

/// Trait for receiving progress updates during scanning.
///
/// Implement this trait to display progress bars, log messages,
/// or any other form of progress feedback.
pub trait ProgressHandler: Send + Sync {
    /// Called when a new scan phase begins.
    ///
    /// `total` is the expected number of items to process in this phase,
    /// or `None` if the total is unknown.
    fn on_phase_start(&self, phase: ScanPhase, total: Option<u64>);

    /// Called periodically during a phase with the current progress.
    fn on_progress(&self, phase: ScanPhase, current: u64, message: &str);

    /// Called when a scan phase completes.
    fn on_phase_end(&self, phase: ScanPhase);
}

/// A no-op progress handler that discards all updates.
pub struct SilentProgress;

impl ProgressHandler for SilentProgress {
    fn on_phase_start(&self, _phase: ScanPhase, _total: Option<u64>) {}
    fn on_progress(&self, _phase: ScanPhase, _current: u64, _message: &str) {}
    fn on_phase_end(&self, _phase: ScanPhase) {}
}
