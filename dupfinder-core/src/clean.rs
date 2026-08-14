//! Safe cleanup, deletion, and remediation engine.
//!
//! Provides the core business logic for:
//! - Planning duplicate and empty item deletions
//! - Validating safety and preserving original files
//! - Moving files to OS Recycle Bin / Trash or permanently unlinking
//! - Producing audit remediation manifests and dry-run execution plans

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::{DupfinderError, Result};
use crate::progress::ProgressHandler;
use crate::safety::{
    is_hidden_path, is_structural_sentinel, is_system_or_protected_path, validate_deletion_safety,
    verify_file_unmodified,
};
use crate::types::ScanReport;

/// The deletion method to apply during cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeletionMethod {
    /// Move items to the OS Recycle Bin / Trash (default).
    #[default]
    Trash,
    /// Permanently remove items from disk.
    Permanent,
}

/// Options configuring the cleanup operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanOptions {
    /// The deletion method (Trash by default).
    pub method: DeletionMethod,
    /// Whether to remove detected empty files.
    pub clean_empty_files: bool,
    /// Whether to remove detected empty directories.
    pub clean_empty_dirs: bool,
    /// Whether to include hidden files in deletion.
    pub include_hidden: bool,
    /// Dry run mode (simulate operations without disk modification).
    pub dry_run: bool,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            method: DeletionMethod::Trash,
            clean_empty_files: false,
            clean_empty_dirs: false,
            include_hidden: false,
            dry_run: false,
        }
    }
}

/// A specific remediation action planned for a target path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CleanAction {
    /// Delete a duplicate copy while preserving the original.
    DeleteDuplicate {
        path: PathBuf,
        size: u64,
        original: PathBuf,
    },
    /// Delete a zero-byte file.
    DeleteEmptyFile { path: PathBuf },
    /// Delete a recursively empty directory.
    DeleteEmptyDir { path: PathBuf },
    /// Action blocked by safety or system path protection.
    Blocked { path: PathBuf, reason: String },
    /// Action skipped (e.g. hidden file or structural sentinel).
    Skipped { path: PathBuf, reason: String },
}

impl CleanAction {
    /// Target path of this action.
    #[must_use]
    pub fn target_path(&self) -> &Path {
        match self {
            CleanAction::DeleteDuplicate { path, .. }
            | CleanAction::DeleteEmptyFile { path }
            | CleanAction::DeleteEmptyDir { path }
            | CleanAction::Blocked { path, .. }
            | CleanAction::Skipped { path, .. } => path,
        }
    }

    /// Size in bytes to be reclaimed if this action succeeds.
    #[must_use]
    pub fn reclaimable_bytes(&self) -> u64 {
        match self {
            CleanAction::DeleteDuplicate { size, .. } => *size,
            _ => 0,
        }
    }
}

/// A structured plan describing all cleanup actions before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanPlan {
    pub actions: Vec<CleanAction>,
    pub total_files_to_remove: usize,
    pub total_dirs_to_remove: usize,
    pub total_bytes_reclaimable: u64,
    pub blocked_count: usize,
    pub skipped_count: usize,
}

/// An entry in the audit remediation manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationManifestEntry {
    pub timestamp: String,
    pub path: PathBuf,
    pub action_type: String,
    pub size: u64,
    pub original: Option<PathBuf>,
    pub status: String,
    pub error: Option<String>,
}

/// Summary result of executing a cleanup plan.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CleanExecutionResult {
    pub succeeded_files: usize,
    pub succeeded_dirs: usize,
    pub bytes_reclaimed: u64,
    pub failed: Vec<(PathBuf, String)>,
    pub manifest: Vec<RemediationManifestEntry>,
}

/// Generate a verified `CleanPlan` from a `ScanReport`.
pub fn generate_clean_plan(report: &ScanReport, options: &CleanOptions) -> Result<CleanPlan> {
    let mut actions = Vec::new();
    let mut total_files_to_remove = 0;
    let mut total_dirs_to_remove = 0;
    let mut total_bytes_reclaimable = 0;
    let mut blocked_count = 0;
    let mut skipped_count = 0;

    // 1. Plan duplicate file deletions
    for group in &report.duplicates.groups {
        let original = &group.original.path;

        for duplicate in &group.duplicates {
            let path = &duplicate.path;

            // Check if protected / system path
            if is_system_or_protected_path(path).unwrap_or(true) {
                actions.push(CleanAction::Blocked {
                    path: path.clone(),
                    reason: "Protected system path or security vault".to_string(),
                });
                blocked_count += 1;
                continue;
            }

            // Check if hidden file without explicit inclusion
            if !options.include_hidden && is_hidden_path(path) {
                actions.push(CleanAction::Skipped {
                    path: path.clone(),
                    reason: "Hidden file (use --include-hidden to enable)".to_string(),
                });
                skipped_count += 1;
                continue;
            }

            // Check original safety validation
            if let Err(e) = validate_deletion_safety(original, std::slice::from_ref(path)) {
                actions.push(CleanAction::Blocked {
                    path: path.clone(),
                    reason: e.to_string(),
                });
                blocked_count += 1;
                continue;
            }

            actions.push(CleanAction::DeleteDuplicate {
                path: path.clone(),
                size: duplicate.size,
                original: original.clone(),
            });
            total_files_to_remove += 1;
            total_bytes_reclaimable += duplicate.size;
        }
    }

    // 2. Plan empty files cleanup
    if options.clean_empty_files {
        for path in &report.empty_files {
            if is_structural_sentinel(path) {
                actions.push(CleanAction::Skipped {
                    path: path.clone(),
                    reason: "Structural sentinel file (e.g. __init__.py, .gitkeep)".to_string(),
                });
                skipped_count += 1;
                continue;
            }

            if is_system_or_protected_path(path).unwrap_or(true) {
                actions.push(CleanAction::Blocked {
                    path: path.clone(),
                    reason: "Protected system path".to_string(),
                });
                blocked_count += 1;
                continue;
            }

            if !options.include_hidden && is_hidden_path(path) {
                actions.push(CleanAction::Skipped {
                    path: path.clone(),
                    reason: "Hidden file".to_string(),
                });
                skipped_count += 1;
                continue;
            }

            actions.push(CleanAction::DeleteEmptyFile { path: path.clone() });
            total_files_to_remove += 1;
        }
    }

    // 3. Plan empty directories cleanup
    if options.clean_empty_dirs {
        for path in &report.empty_dirs {
            if is_system_or_protected_path(path).unwrap_or(true) {
                actions.push(CleanAction::Blocked {
                    path: path.clone(),
                    reason: "Protected system directory".to_string(),
                });
                blocked_count += 1;
                continue;
            }

            if !options.include_hidden && is_hidden_path(path) {
                actions.push(CleanAction::Skipped {
                    path: path.clone(),
                    reason: "Hidden directory".to_string(),
                });
                skipped_count += 1;
                continue;
            }

            actions.push(CleanAction::DeleteEmptyDir { path: path.clone() });
            total_dirs_to_remove += 1;
        }
    }

    Ok(CleanPlan {
        actions,
        total_files_to_remove,
        total_dirs_to_remove,
        total_bytes_reclaimable,
        blocked_count,
        skipped_count,
    })
}

/// Execute a `CleanPlan` according to the specified deletion method and dry-run flag.
pub fn execute_clean_plan(
    plan: &CleanPlan,
    method: DeletionMethod,
    dry_run: bool,
    _progress: &dyn ProgressHandler,
) -> Result<CleanExecutionResult> {
    let mut result = CleanExecutionResult::default();

    for action in &plan.actions {
        let timestamp = Utc::now().to_rfc3339();

        match action {
            CleanAction::DeleteDuplicate {
                path,
                size,
                original,
            } => {
                if dry_run {
                    result.succeeded_files += 1;
                    result.bytes_reclaimed += *size;
                    result.manifest.push(RemediationManifestEntry {
                        timestamp,
                        path: path.clone(),
                        action_type: "delete_duplicate (dry-run)".to_string(),
                        size: *size,
                        original: Some(original.clone()),
                        status: "simulated".to_string(),
                        error: None,
                    });
                    continue;
                }

                // 1. Pre-execution TOCTOU check
                if let Err(e) = verify_file_unmodified(path, *size) {
                    let err_msg = e.to_string();
                    result.failed.push((path.clone(), err_msg.clone()));
                    result.manifest.push(RemediationManifestEntry {
                        timestamp,
                        path: path.clone(),
                        action_type: "delete_duplicate".to_string(),
                        size: *size,
                        original: Some(original.clone()),
                        status: "failed".to_string(),
                        error: Some(err_msg),
                    });
                    continue;
                }

                // 2. Perform deletion
                let delete_res = match method {
                    DeletionMethod::Trash => trash::delete(path).map_err(|e| {
                        DupfinderError::RemediationError(format!("Trash error: {}", e))
                    }),
                    DeletionMethod::Permanent => {
                        fs::remove_file(path).map_err(|e| DupfinderError::IoError {
                            path: path.clone(),
                            source: e,
                        })
                    }
                };

                match delete_res {
                    Ok(_) => {
                        result.succeeded_files += 1;
                        result.bytes_reclaimed += *size;
                        result.manifest.push(RemediationManifestEntry {
                            timestamp,
                            path: path.clone(),
                            action_type: match method {
                                DeletionMethod::Trash => "trash_duplicate".to_string(),
                                DeletionMethod::Permanent => {
                                    "permanent_delete_duplicate".to_string()
                                }
                            },
                            size: *size,
                            original: Some(original.clone()),
                            status: "success".to_string(),
                            error: None,
                        });
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        result.failed.push((path.clone(), err_msg.clone()));
                        result.manifest.push(RemediationManifestEntry {
                            timestamp,
                            path: path.clone(),
                            action_type: "delete_duplicate".to_string(),
                            size: *size,
                            original: Some(original.clone()),
                            status: "failed".to_string(),
                            error: Some(err_msg),
                        });
                    }
                }
            }

            CleanAction::DeleteEmptyFile { path } => {
                if dry_run {
                    result.succeeded_files += 1;
                    result.manifest.push(RemediationManifestEntry {
                        timestamp,
                        path: path.clone(),
                        action_type: "delete_empty_file (dry-run)".to_string(),
                        size: 0,
                        original: None,
                        status: "simulated".to_string(),
                        error: None,
                    });
                    continue;
                }

                let delete_res = match method {
                    DeletionMethod::Trash => trash::delete(path).map_err(|e| {
                        DupfinderError::RemediationError(format!("Trash error: {}", e))
                    }),
                    DeletionMethod::Permanent => {
                        fs::remove_file(path).map_err(|e| DupfinderError::IoError {
                            path: path.clone(),
                            source: e,
                        })
                    }
                };

                match delete_res {
                    Ok(_) => {
                        result.succeeded_files += 1;
                        result.manifest.push(RemediationManifestEntry {
                            timestamp,
                            path: path.clone(),
                            action_type: "delete_empty_file".to_string(),
                            size: 0,
                            original: None,
                            status: "success".to_string(),
                            error: None,
                        });
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        result.failed.push((path.clone(), err_msg.clone()));
                        result.manifest.push(RemediationManifestEntry {
                            timestamp,
                            path: path.clone(),
                            action_type: "delete_empty_file".to_string(),
                            size: 0,
                            original: None,
                            status: "failed".to_string(),
                            error: Some(err_msg),
                        });
                    }
                }
            }

            CleanAction::DeleteEmptyDir { path } => {
                if dry_run {
                    result.succeeded_dirs += 1;
                    result.manifest.push(RemediationManifestEntry {
                        timestamp,
                        path: path.clone(),
                        action_type: "delete_empty_dir (dry-run)".to_string(),
                        size: 0,
                        original: None,
                        status: "simulated".to_string(),
                        error: None,
                    });
                    continue;
                }

                match fs::remove_dir(path) {
                    Ok(_) => {
                        result.succeeded_dirs += 1;
                        result.manifest.push(RemediationManifestEntry {
                            timestamp,
                            path: path.clone(),
                            action_type: "delete_empty_dir".to_string(),
                            size: 0,
                            original: None,
                            status: "success".to_string(),
                            error: None,
                        });
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        result.failed.push((path.clone(), err_msg.clone()));
                        result.manifest.push(RemediationManifestEntry {
                            timestamp,
                            path: path.clone(),
                            action_type: "delete_empty_dir".to_string(),
                            size: 0,
                            original: None,
                            status: "failed".to_string(),
                            error: Some(err_msg),
                        });
                    }
                }
            }

            CleanAction::Blocked { path, reason } => {
                result.manifest.push(RemediationManifestEntry {
                    timestamp,
                    path: path.clone(),
                    action_type: "blocked".to_string(),
                    size: 0,
                    original: None,
                    status: "blocked".to_string(),
                    error: Some(reason.clone()),
                });
            }

            CleanAction::Skipped { path, reason } => {
                result.manifest.push(RemediationManifestEntry {
                    timestamp,
                    path: path.clone(),
                    action_type: "skipped".to_string(),
                    size: 0,
                    original: None,
                    status: "skipped".to_string(),
                    error: Some(reason.clone()),
                });
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;
    use crate::types::{DuplicateGroup, DuplicateReport, FileEntry, ScanInfo};
    use std::time::SystemTime;
    use tempfile::tempdir;

    fn make_test_report(orig: PathBuf, dup: PathBuf, size: u64) -> ScanReport {
        let original_file = FileEntry {
            path: orig,
            size,
            mtime: SystemTime::UNIX_EPOCH,
        };
        let duplicate_file = FileEntry {
            path: dup,
            size,
            mtime: SystemTime::UNIX_EPOCH,
        };

        ScanReport {
            version: "0.1.0".to_string(),
            scan_info: ScanInfo {
                paths: vec![PathBuf::from("/test")],
                timestamp: "2026-08-14T00:00:00Z".to_string(),
                duration_secs: 1.0,
                files_scanned: 2,
                dirs_scanned: 1,
            },
            duplicates: DuplicateReport {
                total_groups: 1,
                total_redundant_files: 1,
                reclaimable_bytes: size,
                groups: vec![DuplicateGroup {
                    hash: "testhash".to_string(),
                    size,
                    original: original_file,
                    duplicates: vec![duplicate_file],
                }],
            },
            empty_files: vec![],
            empty_dirs: vec![],
            broken_symlinks: vec![],
            cache_stats: Default::default(),
        }
    }

    #[test]
    fn test_generate_clean_plan_and_dry_run() {
        let dir = tempdir().unwrap();
        let orig = dir.path().join("orig.txt");
        let dup = dir.path().join("dup.txt");

        fs::write(&orig, b"content").unwrap();
        fs::write(&dup, b"content").unwrap();

        let report = make_test_report(orig.clone(), dup.clone(), 7);
        let options = CleanOptions {
            dry_run: true,
            ..Default::default()
        };

        let plan = generate_clean_plan(&report, &options).unwrap();
        assert_eq!(plan.total_files_to_remove, 1);
        assert_eq!(plan.total_bytes_reclaimable, 7);

        let progress = SilentProgress;
        let res = execute_clean_plan(&plan, DeletionMethod::Permanent, true, &progress).unwrap();
        assert_eq!(res.succeeded_files, 1);
        assert_eq!(res.bytes_reclaimed, 7);

        // Dry run must NOT delete files
        assert!(orig.exists());
        assert!(dup.exists());
    }

    #[test]
    fn test_execute_permanent_deletion() {
        let dir = tempdir().unwrap();
        let orig = dir.path().join("orig.txt");
        let dup = dir.path().join("dup.txt");

        fs::write(&orig, b"content").unwrap();
        fs::write(&dup, b"content").unwrap();

        let report = make_test_report(orig.clone(), dup.clone(), 7);
        let options = CleanOptions::default();

        let plan = generate_clean_plan(&report, &options).unwrap();
        let progress = SilentProgress;
        let res = execute_clean_plan(&plan, DeletionMethod::Permanent, false, &progress).unwrap();

        assert_eq!(res.succeeded_files, 1);
        assert!(orig.exists(), "Original file must be preserved");
        assert!(!dup.exists(), "Duplicate file must be deleted");
    }
}
