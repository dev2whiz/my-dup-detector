//! dupfinder-core: Core library for duplicate file detection and filesystem analysis.
//!
//! This library provides the scanning, hashing, deduplication, and reporting
//! functionality used by the `dupfinder` CLI tool. It can also be used as a
//! standalone library by other applications.

pub mod cache;
pub mod clean;
pub mod dedup;
pub mod empty;
pub mod errors;
pub mod filter;
pub mod hasher;
pub mod ignore;
pub mod progress;
pub mod report;
pub mod safety;
pub mod scanner;
pub mod symlinks;
pub mod types;

use std::time::Instant;

use cache::HashCache;
use chrono::Utc;
use errors::Result;
use filter::FileFilter;
use progress::ProgressHandler;
use types::{CacheStats, DuplicateReport, ScanConfig, ScanInfo, ScanPhase, ScanReport};

/// The current version of dupfinder.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run a complete scan operation with the given configuration.
///
/// This is the main entry point for the core library. It:
/// 1. Traverses directories to discover files
/// 2. Runs enabled detection features (duplicates, empty files/dirs, broken symlinks)
/// 3. Returns a complete scan report
pub fn scan(config: ScanConfig, progress: &dyn ProgressHandler) -> Result<ScanReport> {
    let start_time = Instant::now();
    let timestamp = Utc::now().to_rfc3339();

    // Build the file filter
    let file_filter = FileFilter::from_config_with_roots(&config.filters, &config.paths)?;

    // ── Phase 1: Directory traversal ─────────────────────────────────────────
    let scan_result = scanner::scan_directories(
        &config.paths,
        &file_filter,
        config.max_depth,
        config.include_hidden,
        progress,
    );

    // ── Phase 2: Duplicate detection ─────────────────────────────────────────
    let mut duplicates = DuplicateReport::default();
    let mut cache_stats = CacheStats::default();

    if config.features.duplicates {
        let mut cache = if config.cache_config.enabled {
            HashCache::load(&config.cache_config).ok()
        } else {
            None
        };

        let (dup_report, stats) = dedup::find_duplicates(
            &scan_result.files,
            config.filters.min_size,
            &mut cache,
            progress,
        )?;

        duplicates = dup_report;
        cache_stats = stats;

        // Save cache to disk
        if let Some(ref cache_ref) = cache {
            let _ = cache_ref.save();
        }
    }

    // ── Phase 3: Empty file detection ────────────────────────────────────────
    let empty_files = if config.features.empty_files {
        empty::find_empty_files(&scan_result.files, progress)
    } else {
        Vec::new()
    };

    // ── Phase 4: Empty directory detection ───────────────────────────────────
    let empty_dirs = if config.features.empty_dirs {
        empty::find_empty_dirs(&scan_result.directories, &scan_result.files, progress)
    } else {
        Vec::new()
    };

    // ── Phase 5: Broken symlink detection ────────────────────────────────────
    let broken_symlinks = if config.features.broken_links {
        symlinks::find_broken_symlinks(&scan_result.symlinks, progress)
    } else {
        Vec::new()
    };

    // ── Build final report ───────────────────────────────────────────────────
    let duration = start_time.elapsed();

    progress.on_phase_start(ScanPhase::ReportGeneration, None);
    progress.on_phase_end(ScanPhase::ReportGeneration);

    let report = ScanReport {
        version: VERSION.to_string(),
        scan_info: ScanInfo {
            paths: config.paths,
            timestamp,
            duration_secs: duration.as_secs_f64(),
            files_scanned: scan_result.total_files,
            dirs_scanned: scan_result.total_dirs,
        },
        duplicates,
        empty_files,
        empty_dirs,
        broken_symlinks,
        cache_stats,
    };

    Ok(report)
}
