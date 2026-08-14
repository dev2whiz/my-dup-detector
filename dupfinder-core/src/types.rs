//! Shared types, configuration structs, and enums for dupfinder-core.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// Configuration for a scan operation.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// One or more directories to scan.
    pub paths: Vec<PathBuf>,
    /// Which detection features are enabled.
    pub features: FeatureFlags,
    /// File filtering options.
    pub filters: FilterConfig,
    /// Cache configuration.
    pub cache_config: CacheConfig,
    /// Maximum recursion depth (None = unlimited).
    pub max_depth: Option<usize>,
    /// Whether to include hidden files/directories.
    pub include_hidden: bool,
}

/// Flags controlling which detection features to run.
#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub duplicates: bool,
    pub empty_files: bool,
    pub empty_dirs: bool,
    pub broken_links: bool,
    pub similar_images: bool,
    pub similarity_threshold: f64,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            duplicates: true,
            empty_files: true,
            empty_dirs: true,
            broken_links: true,
            similar_images: false,
            similarity_threshold: 0.90,
        }
    }
}

use crate::ignore::IgnorePreset;

/// File filtering configuration.
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Minimum file size in bytes (files smaller are skipped for duplicate detection).
    pub min_size: u64,
    /// Glob patterns to exclude files.
    pub exclude_patterns: Vec<String>,
    /// Directory names to skip entirely.
    pub exclude_dirs: Vec<String>,
    /// Active ignore preset (default: IgnorePreset::Default).
    pub preset: Option<IgnorePreset>,
    /// Whether to scan JAR/archive files even when using presets (default: false = JARs ignored).
    pub include_jars: bool,
    /// Whether to load the user's global ignore file (~/.config/dupfinder/dupignore) if present.
    pub use_global_ignore: bool,
    /// Whether to load `<scan_root>/.dupignore` if present in scanned directories.
    pub use_project_ignore: bool,
    /// Explicit custom ignore files to load.
    pub custom_ignore_files: Vec<PathBuf>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_size: 1,
            exclude_patterns: Vec::new(),
            exclude_dirs: Vec::new(),
            preset: Some(IgnorePreset::Default),
            include_jars: false,
            use_global_ignore: true,
            use_project_ignore: true,
            custom_ignore_files: Vec::new(),
        }
    }
}

/// Cache configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Whether caching is enabled.
    pub enabled: bool,
    /// Custom cache directory (None = platform default).
    pub cache_dir: Option<PathBuf>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_dir: None,
        }
    }
}

/// Phases of the scanning process (for progress reporting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    /// Traversing directories to discover files.
    DirectoryTraversal,
    /// Grouping files by size.
    SizeGrouping,
    /// Computing partial (8KB prefix) hashes.
    PartialHashing,
    /// Computing full content hashes.
    FullHashing,
    /// Detecting empty files.
    EmptyFileDetection,
    /// Detecting empty directories.
    EmptyDirDetection,
    /// Detecting broken symbolic links.
    BrokenLinkDetection,
    /// Computing perceptual image hashes and detecting visual similarities.
    PerceptualHashing,
    /// Generating report.
    ReportGeneration,
}

impl std::fmt::Display for ScanPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanPhase::DirectoryTraversal => write!(f, "Scanning directories"),
            ScanPhase::SizeGrouping => write!(f, "Grouping by size"),
            ScanPhase::PartialHashing => write!(f, "Partial hashing"),
            ScanPhase::FullHashing => write!(f, "Full hashing"),
            ScanPhase::EmptyFileDetection => write!(f, "Finding empty files"),
            ScanPhase::EmptyDirDetection => write!(f, "Finding empty directories"),
            ScanPhase::BrokenLinkDetection => write!(f, "Finding broken symlinks"),
            ScanPhase::PerceptualHashing => write!(f, "Analyzing image similarities"),
            ScanPhase::ReportGeneration => write!(f, "Generating report"),
        }
    }
}

/// Metadata about a file discovered during scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// File size in bytes.
    pub size: u64,
    /// Last modification time.
    #[serde(with = "system_time_serde")]
    pub mtime: SystemTime,
}

/// A group of duplicate files sharing the same content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// BLAKE3 hash of the file content.
    pub hash: String,
    /// File size in bytes (same for all files in the group).
    pub size: u64,
    /// The file considered the "original" (oldest mtime).
    pub original: FileEntry,
    /// All other copies (duplicates).
    pub duplicates: Vec<FileEntry>,
}

/// A broken symbolic link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenSymlink {
    /// Path to the symlink itself.
    pub link_path: PathBuf,
    /// The target path the symlink points to (which doesn't exist).
    pub target_path: PathBuf,
}

/// Information about the scan itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanInfo {
    /// Directories that were scanned.
    pub paths: Vec<PathBuf>,
    /// ISO 8601 timestamp of when the scan started.
    pub timestamp: String,
    /// Duration of the scan in seconds.
    pub duration_secs: f64,
    /// Total number of files scanned.
    pub files_scanned: u64,
    /// Total number of directories scanned.
    pub dirs_scanned: u64,
}

/// Statistics about cache usage during the scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of files whose hash was found in the cache.
    pub hits: u64,
    /// Number of files whose hash had to be computed.
    pub misses: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Duplicate detection results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DuplicateReport {
    /// Total number of duplicate groups.
    pub total_groups: usize,
    /// Total number of redundant files (excluding originals).
    pub total_redundant_files: usize,
    /// Total bytes that could be reclaimed.
    pub reclaimable_bytes: u64,
    /// All duplicate groups.
    pub groups: Vec<DuplicateGroup>,
}

/// A file entry that is perceptually similar to a canonical original image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarImageEntry {
    /// File metadata.
    pub file: FileEntry,
    /// Visual similarity percentage compared to canonical image (e.g. 95.5%).
    pub similarity_percentage: f64,
    /// Perceptual Hamming distance from canonical image hash (0 = identical).
    pub distance: u32,
}

/// A cluster of perceptually similar images sharing visual characteristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarImageGroup {
    /// Canonical highest-resolution or original image in group.
    pub canonical: FileEntry,
    /// Other image files matching canonical image within threshold.
    pub similar_files: Vec<SimilarImageEntry>,
    /// Perceptual hash string of canonical image.
    pub perceptual_hash: String,
}

/// Perceptual image similarity detection results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimilarImageReport {
    /// Total number of similar image clusters.
    pub total_groups: usize,
    /// Total number of similar image files (excluding canonical originals).
    pub total_similar_files: usize,
    /// Reclaimable capacity across similar image groups.
    pub reclaimable_bytes: u64,
    /// All similar image groups.
    pub groups: Vec<SimilarImageGroup>,
}

/// The complete result of a scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    /// Version of dupfinder that generated this report.
    pub version: String,
    /// Information about the scan.
    pub scan_info: ScanInfo,
    /// Duplicate file results.
    pub duplicates: DuplicateReport,
    /// Similar image results (when --similar-images is enabled).
    pub similar_images: Option<SimilarImageReport>,
    /// Empty files found.
    pub empty_files: Vec<PathBuf>,
    /// Empty directories found.
    pub empty_dirs: Vec<PathBuf>,
    /// Broken symbolic links found.
    pub broken_symlinks: Vec<BrokenSymlink>,
    /// Cache usage statistics.
    pub cache_stats: CacheStats,
}

/// Custom serialization for SystemTime as ISO 8601 strings.
mod system_time_serde {
    use chrono::{DateTime, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::SystemTime;

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let datetime: DateTime<Utc> = (*time).into();
        let s = datetime.to_rfc3339();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let datetime = s
            .parse::<DateTime<Utc>>()
            .map_err(serde::de::Error::custom)?;
        Ok(datetime.into())
    }
}
