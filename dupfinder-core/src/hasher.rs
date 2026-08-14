//! BLAKE3 hashing utilities for partial and full file content hashing.

use rayon::prelude::*;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::errors::DupfinderError;

/// Size of the partial hash prefix (8 KB).
pub const PARTIAL_HASH_SIZE: usize = 8 * 1024;

/// Compute the BLAKE3 hash of a file's first `PARTIAL_HASH_SIZE` bytes.
///
/// For files smaller than `PARTIAL_HASH_SIZE`, hashes the entire file
/// (which makes the partial hash identical to the full hash).
pub fn partial_hash(path: &Path) -> Result<String, DupfinderError> {
    let mut file = File::open(path).map_err(|e| DupfinderError::IoError {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut buffer = vec![0u8; PARTIAL_HASH_SIZE];
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|e| DupfinderError::IoError {
            path: path.to_path_buf(),
            source: e,
        })?;

    buffer.truncate(bytes_read);
    let hash = blake3::hash(&buffer);
    Ok(hash.to_hex().to_string())
}

/// Compute the BLAKE3 hash of a file's entire content.
///
/// Uses a buffered reader for memory efficiency on large files.
pub fn full_hash(path: &Path) -> Result<String, DupfinderError> {
    let mut file = File::open(path).map_err(|e| DupfinderError::IoError {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB read buffer

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| DupfinderError::IoError {
                path: path.to_path_buf(),
                source: e,
            })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(hash.to_hex().to_string())
}

/// Compute partial hashes for a batch of file paths in parallel.
///
/// Returns a vec of (index, hash_result) preserving the mapping to input order.
pub fn parallel_partial_hash(paths: &[&Path]) -> Vec<(usize, Result<String, DupfinderError>)> {
    paths
        .par_iter()
        .enumerate()
        .map(|(i, path)| (i, partial_hash(path)))
        .collect()
}

/// Compute full hashes for a batch of file paths in parallel.
///
/// Returns a vec of (index, hash_result) preserving the mapping to input order.
pub fn parallel_full_hash(paths: &[&Path]) -> Vec<(usize, Result<String, DupfinderError>)> {
    paths
        .par_iter()
        .enumerate()
        .map(|(i, path)| (i, full_hash(path)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_full_hash_deterministic() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "hello world").unwrap();

        let hash1 = full_hash(file.path()).unwrap();
        let hash2 = full_hash(file.path()).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_partial_hash_equals_full_for_small_files() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "small content").unwrap();

        let p_hash = partial_hash(file.path()).unwrap();
        let f_hash = full_hash(file.path()).unwrap();
        // For files smaller than PARTIAL_HASH_SIZE, partial and full hash should be identical
        assert_eq!(p_hash, f_hash);
    }

    #[test]
    fn test_different_content_different_hash() {
        let mut file1 = NamedTempFile::new().unwrap();
        write!(file1, "content A").unwrap();

        let mut file2 = NamedTempFile::new().unwrap();
        write!(file2, "content B").unwrap();

        let hash1 = full_hash(file1.path()).unwrap();
        let hash2 = full_hash(file2.path()).unwrap();
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_same_content_same_hash() {
        let mut file1 = NamedTempFile::new().unwrap();
        write!(file1, "identical content").unwrap();

        let mut file2 = NamedTempFile::new().unwrap();
        write!(file2, "identical content").unwrap();

        let hash1 = full_hash(file1.path()).unwrap();
        let hash2 = full_hash(file2.path()).unwrap();
        assert_eq!(hash1, hash2);
    }
}
