//! 3-stage duplicate file detection pipeline.
//!
//! Stage 1: Group files by size (unique sizes are unique files)
//! Stage 2: Partial hash (first 8KB) to further narrow candidates
//! Stage 3: Full hash to confirm true duplicates

use rayon::prelude::*;
use std::collections::HashMap;

use crate::cache::HashCache;
use crate::errors::Result;
use crate::hasher;
use crate::progress::ProgressHandler;
use crate::types::{CacheStats, DuplicateGroup, DuplicateReport, FileEntry, ScanPhase};

/// Run the 3-stage duplicate detection pipeline.
pub fn find_duplicates(
    files: &[FileEntry],
    min_size: u64,
    cache: &mut Option<HashCache>,
    progress: &dyn ProgressHandler,
) -> Result<(DuplicateReport, CacheStats)> {
    let mut cache_stats = CacheStats::default();

    // ── Stage 1: Group by file size ──────────────────────────────────────────
    progress.on_phase_start(ScanPhase::SizeGrouping, Some(files.len() as u64));

    let mut size_groups: HashMap<u64, Vec<&FileEntry>> = HashMap::new();
    for file in files {
        // Skip files below minimum size
        if file.size < min_size {
            continue;
        }
        size_groups.entry(file.size).or_default().push(file);
    }

    // Remove groups with only one file (unique by size)
    size_groups.retain(|_, group| group.len() > 1);

    let candidates_after_size: usize = size_groups.values().map(|g| g.len()).sum();
    progress.on_progress(
        ScanPhase::SizeGrouping,
        candidates_after_size as u64,
        &format!(
            "{} files in {} size groups",
            candidates_after_size,
            size_groups.len()
        ),
    );
    progress.on_phase_end(ScanPhase::SizeGrouping);

    if size_groups.is_empty() {
        return Ok((DuplicateReport::default(), cache_stats));
    }

    // ── Stage 2: Partial hash (first 8KB) ────────────────────────────────────
    let all_candidates: Vec<&FileEntry> = size_groups.values().flatten().copied().collect();
    progress.on_phase_start(ScanPhase::PartialHashing, Some(all_candidates.len() as u64));

    // Compute partial hashes in parallel
    let partial_results: Vec<(&FileEntry, Option<String>)> = all_candidates
        .par_iter()
        .map(|file| {
            let hash = hasher::partial_hash(&file.path).ok();
            (*file, hash)
        })
        .collect();

    // Group by (size, partial_hash)
    let mut partial_groups: HashMap<(u64, String), Vec<&FileEntry>> = HashMap::new();
    for (file, hash_opt) in &partial_results {
        if let Some(ref hash) = hash_opt {
            partial_groups
                .entry((file.size, hash.clone()))
                .or_default()
                .push(file);
        }
    }

    // Remove groups with only one file
    partial_groups.retain(|_, group| group.len() > 1);

    let candidates_after_partial: usize = partial_groups.values().map(|g| g.len()).sum();
    progress.on_progress(
        ScanPhase::PartialHashing,
        candidates_after_partial as u64,
        &format!(
            "{} files in {} partial-hash groups",
            candidates_after_partial,
            partial_groups.len()
        ),
    );
    progress.on_phase_end(ScanPhase::PartialHashing);

    if partial_groups.is_empty() {
        return Ok((DuplicateReport::default(), cache_stats));
    }

    // ── Stage 3: Full hash ───────────────────────────────────────────────────
    let full_candidates: Vec<&FileEntry> = partial_groups.values().flatten().copied().collect();
    progress.on_phase_start(ScanPhase::FullHashing, Some(full_candidates.len() as u64));

    // Try cache lookups first, then hash the rest in parallel
    let mut full_hashes: Vec<(&FileEntry, String)> = Vec::new();
    let mut need_hashing: Vec<&FileEntry> = Vec::new();

    for file in &full_candidates {
        if let Some(ref cache_ref) = cache {
            if let Some(cached_hash) = cache_ref.lookup(&file.path, file.size, file.mtime) {
                full_hashes.push((file, cached_hash.to_string()));
                cache_stats.hits += 1;
                continue;
            }
        }
        cache_stats.misses += 1;
        need_hashing.push(file);
    }

    // Hash remaining files in parallel
    let computed: Vec<(&FileEntry, Option<String>)> = need_hashing
        .par_iter()
        .map(|file| {
            let hash = hasher::full_hash(&file.path).ok();
            (*file, hash)
        })
        .collect();

    for (file, hash_opt) in computed {
        if let Some(hash) = hash_opt {
            // Update cache
            if let Some(ref mut cache_ref) = cache {
                cache_ref.insert(&file.path, file.size, file.mtime, hash.clone());
            }
            full_hashes.push((file, hash));
        }
    }

    // Group by full hash
    let mut hash_groups: HashMap<String, Vec<&FileEntry>> = HashMap::new();
    for (file, hash) in &full_hashes {
        hash_groups.entry(hash.clone()).or_default().push(file);
    }

    // Remove groups with only one file
    hash_groups.retain(|_, group| group.len() > 1);

    progress.on_progress(
        ScanPhase::FullHashing,
        hash_groups.len() as u64,
        &format!("{} duplicate groups confirmed", hash_groups.len()),
    );
    progress.on_phase_end(ScanPhase::FullHashing);

    // ── Build report ─────────────────────────────────────────────────────────
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    let mut total_redundant = 0usize;
    let mut reclaimable = 0u64;

    for (hash, mut group_files) in hash_groups {
        // Sort by mtime (oldest first) to determine the "original"
        group_files.sort_by_key(|f| f.mtime);

        let original = (*group_files[0]).clone();
        let duplicates: Vec<FileEntry> = group_files[1..].iter().map(|f| (*f).clone()).collect();

        let dup_count = duplicates.len();
        total_redundant += dup_count;
        reclaimable += original.size * dup_count as u64;

        groups.push(DuplicateGroup {
            hash,
            size: original.size,
            original,
            duplicates,
        });
    }

    // Sort groups by reclaimable space (largest first)
    groups.sort_by(|a, b| {
        let a_reclaimable = a.size * a.duplicates.len() as u64;
        let b_reclaimable = b.size * b.duplicates.len() as u64;
        b_reclaimable.cmp(&a_reclaimable)
    });

    let report = DuplicateReport {
        total_groups: groups.len(),
        total_redundant_files: total_redundant,
        reclaimable_bytes: reclaimable,
        groups,
    };

    Ok((report, cache_stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;
    use std::io::Write;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, name: &str, content: &str, mtime_offset: u64) -> FileEntry {
        let path = dir.path().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "{}", content).unwrap();
        drop(file);

        // Set mtime
        let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1000 + mtime_offset);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(mtime)).ok(); // May fail on some systems, that's OK for tests

        let metadata = std::fs::metadata(&path).unwrap();
        FileEntry {
            path,
            size: metadata.len(),
            mtime: metadata.modified().unwrap(),
        }
    }

    #[test]
    fn test_no_duplicates() {
        let dir = TempDir::new().unwrap();
        let files = vec![
            create_test_file(&dir, "a.txt", "unique content A", 0),
            create_test_file(&dir, "b.txt", "unique content B", 1),
            create_test_file(&dir, "c.txt", "unique content C", 2),
        ];

        let (report, _stats) = find_duplicates(&files, 1, &mut None, &SilentProgress).unwrap();
        assert_eq!(report.total_groups, 0);
        assert_eq!(report.total_redundant_files, 0);
    }

    #[test]
    fn test_finds_duplicates() {
        let dir = TempDir::new().unwrap();
        let files = vec![
            create_test_file(&dir, "a.txt", "duplicate content", 0),
            create_test_file(&dir, "b.txt", "duplicate content", 1),
            create_test_file(&dir, "c.txt", "unique content", 2),
        ];

        let (report, _stats) = find_duplicates(&files, 1, &mut None, &SilentProgress).unwrap();
        assert_eq!(report.total_groups, 1);
        assert_eq!(report.total_redundant_files, 1);
    }

    #[test]
    fn test_oldest_is_original() {
        let dir = TempDir::new().unwrap();
        let file_old = create_test_file(&dir, "old.txt", "same content", 0);
        let file_new = create_test_file(&dir, "new.txt", "same content", 100);

        let files = vec![file_new, file_old.clone()];

        let (report, _stats) = find_duplicates(&files, 1, &mut None, &SilentProgress).unwrap();

        assert_eq!(report.total_groups, 1);
        // The original should have the earlier mtime
        assert!(report.groups[0].original.mtime <= report.groups[0].duplicates[0].mtime);
    }
}
