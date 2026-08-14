//! JSON-based hash cache for avoiding re-hashing unchanged files.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::errors::{DupfinderError, Result};
use crate::types::CacheConfig;

/// A single cached entry: maps a file to its previously computed hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Full content hash (BLAKE3).
    pub hash: String,
    /// File size when hash was computed.
    pub size: u64,
    /// Modification time when hash was computed (as seconds since epoch).
    pub mtime_secs: u64,
}

/// The on-disk cache structure.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheData {
    /// Version of the cache format.
    pub version: u32,
    /// Map from canonical file path to cached hash entry.
    pub entries: HashMap<String, CacheEntry>,
}

/// Hash cache manager.
pub struct HashCache {
    /// Where the cache file lives.
    cache_path: PathBuf,
    /// In-memory cache data.
    data: CacheData,
    /// Whether the cache has been modified and needs writing.
    dirty: bool,
}

impl HashCache {
    /// Load or create a cache at the configured location.
    pub fn load(config: &CacheConfig) -> Result<Self> {
        let cache_dir = cache_dir(config);
        let cache_path = cache_dir.join("cache.json");

        let data = if cache_path.exists() {
            let content = fs::read_to_string(&cache_path)
                .map_err(|e| DupfinderError::CacheError(format!("Failed to read cache: {}", e)))?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            CacheData {
                version: 1,
                entries: HashMap::new(),
            }
        };

        Ok(Self {
            cache_path,
            data,
            dirty: false,
        })
    }

    /// Look up a file's hash in the cache.
    ///
    /// Returns `Some(hash)` if the file's size and mtime match the cached entry.
    pub fn lookup(&self, path: &Path, size: u64, mtime: SystemTime) -> Option<&str> {
        let key = path.to_string_lossy().to_string();
        let entry = self.data.entries.get(&key)?;

        let mtime_secs = mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if entry.size == size && entry.mtime_secs == mtime_secs {
            Some(&entry.hash)
        } else {
            None
        }
    }

    /// Insert or update a file's hash in the cache.
    pub fn insert(&mut self, path: &Path, size: u64, mtime: SystemTime, hash: String) {
        let key = path.to_string_lossy().to_string();
        let mtime_secs = mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.data.entries.insert(
            key,
            CacheEntry {
                hash,
                size,
                mtime_secs,
            },
        );
        self.dirty = true;
    }

    /// Save the cache to disk if it has been modified.
    pub fn save(&self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        // Ensure cache directory exists
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                DupfinderError::CacheError(format!("Failed to create cache directory: {}", e))
            })?;
        }

        let json = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.cache_path, json)
            .map_err(|e| DupfinderError::CacheError(format!("Failed to write cache: {}", e)))?;

        Ok(())
    }

    /// Get the number of entries in the cache.
    pub fn entry_count(&self) -> usize {
        self.data.entries.len()
    }

    /// Get the cache file path.
    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.data.entries.clear();
        self.dirty = true;
    }
}

/// Determine the cache directory based on configuration and platform.
pub fn cache_dir(config: &CacheConfig) -> PathBuf {
    if let Some(ref dir) = config.cache_dir {
        return dir.clone();
    }

    // Use platform-appropriate cache directory
    if let Some(cache) = dirs::cache_dir() {
        cache.join("dupfinder")
    } else {
        // Fallback to home directory
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dupfinder")
    }
}

/// Get information about the cache (for `dupfinder cache info`).
pub struct CacheInfo {
    pub path: PathBuf,
    pub entry_count: usize,
    pub file_size_bytes: u64,
    pub exists: bool,
}

/// Retrieve cache info without loading all entries.
pub fn get_cache_info(config: &CacheConfig) -> CacheInfo {
    let dir = cache_dir(config);
    let path = dir.join("cache.json");
    let exists = path.exists();

    let (entry_count, file_size_bytes) = if exists {
        let file_size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let entry_count = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<CacheData>(&content).ok())
            .map(|data| data.entries.len())
            .unwrap_or(0);
        (entry_count, file_size)
    } else {
        (0, 0)
    };

    CacheInfo {
        path,
        entry_count,
        file_size_bytes,
        exists,
    }
}

/// Delete the cache file.
pub fn clear_cache(config: &CacheConfig) -> Result<()> {
    let dir = cache_dir(config);
    let path = dir.join("cache.json");
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| DupfinderError::CacheError(format!("Failed to delete cache: {}", e)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cache_insert_and_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let config = CacheConfig {
            enabled: true,
            cache_dir: Some(dir.path().to_path_buf()),
        };

        let mut cache = HashCache::load(&config).unwrap();
        let path = Path::new("/tmp/test.txt");
        let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

        cache.insert(path, 42, mtime, "abc123".to_string());

        assert_eq!(cache.lookup(path, 42, mtime), Some("abc123"));
        // Different size should miss
        assert_eq!(cache.lookup(path, 99, mtime), None);
        // Different mtime should miss
        let other_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);
        assert_eq!(cache.lookup(path, 42, other_mtime), None);
    }

    #[test]
    fn test_cache_save_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let config = CacheConfig {
            enabled: true,
            cache_dir: Some(dir.path().to_path_buf()),
        };

        let path = Path::new("/tmp/test.txt");
        let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);

        // Insert and save
        {
            let mut cache = HashCache::load(&config).unwrap();
            cache.insert(path, 42, mtime, "abc123".to_string());
            cache.save().unwrap();
        }

        // Reload and verify
        {
            let cache = HashCache::load(&config).unwrap();
            assert_eq!(cache.lookup(path, 42, mtime), Some("abc123"));
            assert_eq!(cache.entry_count(), 1);
        }
    }
}
