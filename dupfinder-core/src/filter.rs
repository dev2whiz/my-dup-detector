//! File filtering logic for scan operations.

use std::path::{Path, PathBuf};

use crate::ignore::{get_global_ignore_path, IgnoreRules};
use crate::types::FilterConfig;

/// Compiled filter that efficiently tests files against configured rules and presets.
pub struct FileFilter {
    /// Minimum file size in bytes.
    pub min_size: u64,
    /// Ignore rules compiled from presets, config files, and CLI options.
    pub rules: IgnoreRules,
}

impl FileFilter {
    /// Build a `FileFilter` from the configuration.
    pub fn from_config(config: &FilterConfig) -> Result<Self, globset::Error> {
        Self::from_config_with_roots(config, &[])
    }

    /// Build a `FileFilter` from the configuration and given scan root directories.
    pub fn from_config_with_roots(
        config: &FilterConfig,
        scan_roots: &[PathBuf],
    ) -> Result<Self, globset::Error> {
        let mut rules = match config.preset {
            Some(preset) => IgnoreRules::from_preset(preset, config.include_jars),
            None => IgnoreRules::empty(),
        };

        // 1. Global ignore file (~/.config/dupfinder/dupignore)
        if config.use_global_ignore {
            if let Some(global_path) = get_global_ignore_path() {
                if global_path.is_file() {
                    let _ = rules.load_from_file(&global_path);
                }
            }
        }

        // 2. Project-level ignore files (<scan_root>/.dupignore)
        if config.use_project_ignore {
            for root in scan_roots {
                let local_dupignore = root.join(".dupignore");
                if local_dupignore.is_file() {
                    let _ = rules.load_from_file(&local_dupignore);
                }
            }
        }

        // 3. Custom ignore files specified explicitly
        for custom_file in &config.custom_ignore_files {
            if custom_file.is_file() {
                let _ = rules.load_from_file(custom_file);
            }
        }

        // 4. CLI / user direct exclusions
        for dir in &config.exclude_dirs {
            rules.add_dir(dir);
        }

        for pattern in &config.exclude_patterns {
            rules.add_pattern(pattern);
        }

        rules.compile_globs();

        Ok(Self {
            min_size: config.min_size,
            rules,
        })
    }

    /// Check if a file should be excluded based on glob patterns.
    pub fn is_excluded_by_pattern(&self, path: &Path) -> bool {
        self.rules.is_path_excluded(path)
    }

    /// Check if a directory name is in the exclusion list.
    pub fn is_excluded_dir(&self, dir_name: &str) -> bool {
        self.rules.is_dir_excluded(dir_name)
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
    use crate::ignore::IgnorePreset;

    #[test]
    fn test_is_hidden() {
        assert!(is_hidden(Path::new(".hidden")));
        assert!(is_hidden(Path::new("/some/path/.dotfile")));
        assert!(!is_hidden(Path::new("visible")));
        assert!(!is_hidden(Path::new("/some/path/normal.txt")));
    }

    #[test]
    fn test_excluded_dir_defaults() {
        let config = FilterConfig::default();
        let filter = FileFilter::from_config(&config).unwrap();
        assert!(filter.is_excluded_dir(".git"));
        assert!(filter.is_excluded_dir("node_modules"));
        assert!(filter.is_excluded_dir("target"));
        assert!(filter.is_excluded_dir(".gradle"));
        assert!(filter.is_excluded_dir("__pycache__"));
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
        assert!(filter.is_excluded_by_pattern(Path::new("app.jar")));
        assert!(!filter.is_excluded_by_pattern(Path::new("main.rs")));
    }

    #[test]
    fn test_no_presets() {
        let config = FilterConfig {
            preset: None,
            use_global_ignore: false,
            use_project_ignore: false,
            exclude_patterns: vec!["*.custom".to_string()],
            exclude_dirs: vec!["custom_dir".to_string()],
            ..Default::default()
        };
        let filter = FileFilter::from_config(&config).unwrap();
        assert!(!filter.is_excluded_dir("node_modules"));
        assert!(!filter.is_excluded_by_pattern(Path::new("app.jar")));
        assert!(filter.is_excluded_dir("custom_dir"));
        assert!(filter.is_excluded_by_pattern(Path::new("test.custom")));
    }

    #[test]
    fn test_include_jars_toggle() {
        let config = FilterConfig {
            preset: Some(IgnorePreset::Default),
            include_jars: true,
            use_global_ignore: false,
            use_project_ignore: false,
            ..Default::default()
        };
        let filter = FileFilter::from_config(&config).unwrap();
        assert!(!filter.is_excluded_by_pattern(Path::new("app.jar")));
        assert!(filter.is_excluded_by_pattern(Path::new("main.o")));
    }
}
