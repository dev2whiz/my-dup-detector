//! Directory traversal and file discovery.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

use crate::filter::{is_hidden, FileFilter};
use crate::progress::ProgressHandler;
use crate::types::{FileEntry, ScanPhase};

/// Result of scanning directories.
pub struct ScanResult {
    /// All discovered file entries (with metadata).
    pub files: Vec<FileEntry>,
    /// All discovered directory paths.
    pub directories: Vec<PathBuf>,
    /// All discovered symlinks (path, target).
    pub symlinks: Vec<(PathBuf, PathBuf)>,
    /// Total files scanned.
    pub total_files: u64,
    /// Total directories scanned.
    pub total_dirs: u64,
}

/// Scan one or more directory trees, applying filters, and collecting file metadata.
pub fn scan_directories(
    paths: &[PathBuf],
    filter: &FileFilter,
    max_depth: Option<usize>,
    include_hidden: bool,
    progress: &dyn ProgressHandler,
) -> ScanResult {
    progress.on_phase_start(ScanPhase::DirectoryTraversal, None);

    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut symlinks = Vec::new();
    let file_count = Arc::new(AtomicU64::new(0));
    let dir_count = Arc::new(AtomicU64::new(0));

    for scan_path in paths {
        let mut walker = WalkDir::new(scan_path).follow_links(false);

        if let Some(depth) = max_depth {
            walker = walker.max_depth(depth + 1); // +1 because root is depth 0
        }

        for entry in walker
            .into_iter()
            .filter_entry(|e| {
                // Skip hidden entries unless include_hidden is set
                if !include_hidden && is_hidden(e.path()) {
                    // But don't skip the root scan path itself
                    if e.depth() > 0 {
                        return false;
                    }
                }

                // Skip excluded directories
                if e.file_type().is_dir() {
                    if let Some(name) = e.file_name().to_str() {
                        if e.depth() > 0 && filter.is_excluded_dir(name) {
                            return false;
                        }
                    }
                }

                true
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue, // Skip entries we can't read
            };

            let path = entry.path().to_path_buf();

            // Handle symlinks
            if entry.path_is_symlink() {
                let target = std::fs::read_link(&path).unwrap_or_default();
                symlinks.push((path, target));
                continue;
            }

            if entry.file_type().is_dir() {
                let count = dir_count.fetch_add(1, Ordering::Relaxed) + 1;
                directories.push(path);
                if count.is_multiple_of(100) {
                    progress.on_progress(
                        ScanPhase::DirectoryTraversal,
                        count,
                        &format!("{} dirs scanned", count),
                    );
                }
                continue;
            }

            if entry.file_type().is_file() {
                // Apply glob exclusion filter
                if filter.is_excluded_by_pattern(&path) {
                    continue;
                }

                // Get metadata
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let size = metadata.len();
                let mtime = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                let count = file_count.fetch_add(1, Ordering::Relaxed) + 1;
                if count.is_multiple_of(500) {
                    progress.on_progress(
                        ScanPhase::DirectoryTraversal,
                        count,
                        &format!("{} files discovered", count),
                    );
                }

                files.push(FileEntry { path, size, mtime });
            }
        }
    }

    let total_files = file_count.load(Ordering::Relaxed);
    let total_dirs = dir_count.load(Ordering::Relaxed);

    progress.on_progress(
        ScanPhase::DirectoryTraversal,
        total_files,
        &format!(
            "Discovered {} files in {} directories",
            total_files, total_dirs
        ),
    );
    progress.on_phase_end(ScanPhase::DirectoryTraversal);

    ScanResult {
        files,
        directories,
        symlinks,
        total_files,
        total_dirs,
    }
}
