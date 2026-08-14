//! File filtering logic for scan operations.

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

use crate::types::FilterConfig;

/// Compiled filter that efficiently tests files against configured rules.
pub struct FileFilter {
    /// Minimum file size in bytes.
    pub min_size: u64,
    /// Compiled glob patterns for exclusion.
    exclude_globs: Option<GlobSet>,
    /// Directory names to skip.
    pub exclude_dirs: Vec<String>,
}

impl FileFilter {
    /// Build a `FileFilter` from the configuration.
    pub fn from_config(config: &FilterConfig) -> Result<Self, globset::Error> {
        let exclude_globs = if config.exclude_patterns.is_empty() {
            None
        } else {
            let mut builder = GlobSetBuilder::new();
            for pattern in &config.exclude_patterns {
                builder.add(Glob::new(pattern)?);
            }
            Some(builder.build()?)
        };

        Ok(Self {
            min_size: config.min_size,
            exclude_globs,
            exclude_dirs: config.exclude_dirs.clone(),
        })
    }

    /// Check if a file should be excluded based on glob patterns.
    pub fn is_excluded_by_pattern(&self, path: &Path) -> bool {
        if let Some(ref globs) = self.exclude_globs {
            globs.is_match(path)
        } else {
            false
        }
    }

    /// Check if a directory name is in the exclusion list.
    pub fn is_excluded_dir(&self, dir_name: &str) -> bool {
        self.exclude_dirs.iter().any(|d| d == dir_name)
    }

    /// Check if a file meets the minimum size requirement.
    pub fn meets_min_size(&self, size: u64) -> bool {
        size >= self.min_size
    }
}

/// Check if a path component represents a hidden file/directory (starts with '.').
pub fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hidden() {
        assert!(is_hidden(Path::new(".hidden")));
        assert!(is_hidden(Path::new("/some/path/.dotfile")));
        assert!(!is_hidden(Path::new("visible")));
        assert!(!is_hidden(Path::new("/some/path/normal.txt")));
    }

    #[test]
    fn test_excluded_dir() {
        let config = FilterConfig::default();
        let filter = FileFilter::from_config(&config).unwrap();
        assert!(filter.is_excluded_dir(".git"));
        assert!(filter.is_excluded_dir("node_modules"));
        assert!(!filter.is_excluded_dir("src"));
    }

    #[test]
    fn test_min_size() {
        let config = FilterConfig {
            min_size: 1024,
            ..Default::default()
        };
        let filter = FileFilter::from_config(&config).unwrap();
        assert!(!filter.meets_min_size(100));
        assert!(filter.meets_min_size(1024));
        assert!(filter.meets_min_size(2048));
    }

    #[test]
    fn test_glob_exclusion() {
        let config = FilterConfig {
            exclude_patterns: vec!["*.log".to_string(), "*.tmp".to_string()],
            ..Default::default()
        };
        let filter = FileFilter::from_config(&config).unwrap();
        assert!(filter.is_excluded_by_pattern(Path::new("debug.log")));
        assert!(filter.is_excluded_by_pattern(Path::new("/var/app.tmp")));
        assert!(!filter.is_excluded_by_pattern(Path::new("main.rs")));
    }
}
