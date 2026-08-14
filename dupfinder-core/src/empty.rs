//! Empty file and empty directory detection.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::progress::ProgressHandler;
use crate::types::{FileEntry, ScanPhase};

/// Find all empty (0-byte) files from the scanned file list.
pub fn find_empty_files(files: &[FileEntry], progress: &dyn ProgressHandler) -> Vec<PathBuf> {
    progress.on_phase_start(ScanPhase::EmptyFileDetection, Some(files.len() as u64));

    let empty: Vec<PathBuf> = files
        .iter()
        .filter(|f| f.size == 0)
        .map(|f| f.path.clone())
        .collect();

    progress.on_progress(
        ScanPhase::EmptyFileDetection,
        empty.len() as u64,
        &format!("Found {} empty files", empty.len()),
    );
    progress.on_phase_end(ScanPhase::EmptyFileDetection);

    empty
}

/// Find all empty directories.
///
/// A directory is considered empty if it contains no files (recursively).
/// A directory containing only other empty directories is itself empty.
/// Results are returned in bottom-up order (deepest first).
pub fn find_empty_dirs(
    directories: &[PathBuf],
    files: &[FileEntry],
    progress: &dyn ProgressHandler,
) -> Vec<PathBuf> {
    progress.on_phase_start(ScanPhase::EmptyDirDetection, Some(directories.len() as u64));

    // Build a set of directories that contain at least one file
    let mut non_empty_dirs: HashSet<PathBuf> = HashSet::new();

    for file in files {
        // Mark all ancestor directories of this file as non-empty
        let mut current = file.path.parent();
        while let Some(dir) = current {
            if !non_empty_dirs.insert(dir.to_path_buf()) {
                // Already marked; all ancestors are already marked too
                break;
            }
            current = dir.parent();
        }
    }

    // Any scanned directory NOT in non_empty_dirs is empty
    let mut empty_dirs: Vec<PathBuf> = directories
        .iter()
        .filter(|dir| !non_empty_dirs.contains(dir.as_path()))
        .cloned()
        .collect();

    // Sort by path depth (deepest first) for bottom-up reporting
    empty_dirs.sort_by(|a, b| {
        let depth_a = a.components().count();
        let depth_b = b.components().count();
        depth_b.cmp(&depth_a)
    });

    progress.on_progress(
        ScanPhase::EmptyDirDetection,
        empty_dirs.len() as u64,
        &format!("Found {} empty directories", empty_dirs.len()),
    );
    progress.on_phase_end(ScanPhase::EmptyDirDetection);

    empty_dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;
    use std::time::SystemTime;

    #[test]
    fn test_find_empty_files() {
        let files = vec![
            FileEntry {
                path: PathBuf::from("/a/empty.txt"),
                size: 0,
                mtime: SystemTime::now(),
            },
            FileEntry {
                path: PathBuf::from("/a/nonempty.txt"),
                size: 100,
                mtime: SystemTime::now(),
            },
            FileEntry {
                path: PathBuf::from("/b/also_empty.txt"),
                size: 0,
                mtime: SystemTime::now(),
            },
        ];

        let empty = find_empty_files(&files, &SilentProgress);
        assert_eq!(empty.len(), 2);
        assert!(empty.contains(&PathBuf::from("/a/empty.txt")));
        assert!(empty.contains(&PathBuf::from("/b/also_empty.txt")));
    }

    #[test]
    fn test_find_empty_dirs() {
        let directories = vec![
            PathBuf::from("/root"),
            PathBuf::from("/root/has_files"),
            PathBuf::from("/root/empty_dir"),
            PathBuf::from("/root/empty_dir/nested_empty"),
        ];

        let files = vec![FileEntry {
            path: PathBuf::from("/root/has_files/doc.txt"),
            size: 100,
            mtime: SystemTime::now(),
        }];

        let empty = find_empty_dirs(&directories, &files, &SilentProgress);
        assert_eq!(empty.len(), 2);
        // Deepest first
        assert!(empty.contains(&PathBuf::from("/root/empty_dir/nested_empty")));
        assert!(empty.contains(&PathBuf::from("/root/empty_dir")));
    }
}
