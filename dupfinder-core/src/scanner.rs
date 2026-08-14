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

        for entry in walker.into_iter().filter_entry(|e| {
            // Skip hidden entries unless include_hidden is set
            if !include_hidden && is_hidden(e.path()) {
                // But don't skip the root scan path itself
                if e.depth() > 0 {
                    return false;
                }
            }

            // Skip excluded directories
            if e.file_type().is_dir() && e.depth() > 0 {
                if let Some(name) = e.file_name().to_str() {
                    if filter.is_excluded_dir(name) {
                        return false;
                    }
                }
                if filter.is_excluded_by_pattern(e.path()) {
                    return false;
                }
            }

            true
        }) {
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
                if count % 100 == 0 {
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
                if count % 500 == 0 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;
    use crate::types::FilterConfig;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_scanner_prunes_build_dirs_and_jars() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create standard source files
        let src_dir = root.join("src");
        fs::create_dir(&src_dir).unwrap();
        let mut f1 = File::create(src_dir.join("main.rs")).unwrap();
        writeln!(f1, "fn main() {{}}").unwrap();

        // Create build / dependency folders
        let node_modules = root.join("node_modules").join("pkg");
        fs::create_dir_all(&node_modules).unwrap();
        let mut f2 = File::create(node_modules.join("index.js")).unwrap();
        writeln!(f2, "console.log(1);").unwrap();

        let target_dir = root.join("target").join("debug");
        fs::create_dir_all(&target_dir).unwrap();
        let mut f3 = File::create(target_dir.join("app")).unwrap();
        writeln!(f3, "binary").unwrap();

        // Create a JAR and a Python bytecode file
        let mut f4 = File::create(root.join("lib.jar")).unwrap();
        writeln!(f4, "jar archive content").unwrap();

        let mut f5 = File::create(root.join("test.pyc")).unwrap();
        writeln!(f5, "bytecode").unwrap();

        // 1. Scan with Default Preset -> should only discover main.rs
        let config = FilterConfig::default();
        let filter = FileFilter::from_config_with_roots(&config, &[root.to_path_buf()]).unwrap();
        let progress = SilentProgress;

        let result = scan_directories(&[root.to_path_buf()], &filter, None, false, &progress);

        let file_names: Vec<String> = result
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert_eq!(file_names, vec!["main.rs"]);

        // 2. Scan with include_jars = true -> should discover main.rs AND lib.jar
        let jar_config = FilterConfig {
            include_jars: true,
            ..Default::default()
        };
        let jar_filter =
            FileFilter::from_config_with_roots(&jar_config, &[root.to_path_buf()]).unwrap();
        let jar_result =
            scan_directories(&[root.to_path_buf()], &jar_filter, None, false, &progress);

        let jar_file_names: Vec<String> = jar_result
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert!(jar_file_names.contains(&"main.rs".to_string()));
        assert!(jar_file_names.contains(&"lib.jar".to_string()));
        assert!(!jar_file_names.contains(&"index.js".to_string()));
        assert!(!jar_file_names.contains(&"test.pyc".to_string()));
    }

    #[test]
    fn test_scanner_loads_project_dupignore() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create files
        let mut f1 = File::create(root.join("main.rs")).unwrap();
        writeln!(f1, "code").unwrap();

        let mut f2 = File::create(root.join("notes.txt")).unwrap();
        writeln!(f2, "secret notes").unwrap();

        let custom_dir = root.join("custom_cache");
        fs::create_dir(&custom_dir).unwrap();
        let mut f3 = File::create(custom_dir.join("item.dat")).unwrap();
        writeln!(f3, "cache").unwrap();

        // Create .dupignore in scan root
        let mut ignore_file = File::create(root.join(".dupignore")).unwrap();
        writeln!(ignore_file, "*.txt\ncustom_cache/\n").unwrap();

        let config = FilterConfig {
            use_project_ignore: true,
            ..Default::default()
        };
        let filter = FileFilter::from_config_with_roots(&config, &[root.to_path_buf()]).unwrap();
        let progress = SilentProgress;

        let result = scan_directories(&[root.to_path_buf()], &filter, None, false, &progress);

        let file_names: Vec<String> = result
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();

        assert_eq!(file_names, vec!["main.rs"]);
    }
}
