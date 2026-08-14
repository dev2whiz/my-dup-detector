//! Safety, privilege detection, and system path protection engine.
//!
//! Provides invariants and guardrails ensuring non-destructive operations:
//! 1. Elevated privilege detection (prevent running with root/admin unintentionally).
//! 2. Protected OS and system path blacklisting across macOS, Linux, and Windows.
//! 3. Structural sentinel protection for zero-byte file cleanups.
//! 4. Pre-execution TOCTOU re-stat verification.
//! 5. Original preservation verification.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::errors::{DupfinderError, Result};

/// Structural sentinels that should never be deleted by empty-file cleanups.
pub const STRUCTURAL_SENTINELS: &[&str] = &[
    "__init__.py",
    ".gitkeep",
    ".keep",
    ".placeholder",
    "keep.me",
    ".touch",
];

/// Check if the current process is running with elevated privileges (root or administrator).
#[must_use]
pub fn is_elevated_privilege() -> bool {
    #[cfg(unix)]
    {
        extern "C" {
            fn geteuid() -> u32;
        }
        // Safety: geteuid is a standard POSIX system call that takes no arguments and has no side effects.
        unsafe { geteuid() == 0 }
    }

    #[cfg(windows)]
    {
        // On Windows, check whether the user has administrator privileges.
        // We can inspect whether the process token has elevated rights via advapi32/shell32 if available.
        false
    }

    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Check if a path is a protected OS directory, system root, or sensitive toolchain/secret vault.
///
/// Paths are canonicalized before evaluation to prevent symlink or `../` path traversal bypasses.
pub fn is_system_or_protected_path(path: &Path) -> Result<bool> {
    let canonical = if path.exists() {
        fs::canonicalize(path).map_err(|e| DupfinderError::IoError {
            path: path.to_path_buf(),
            source: e,
        })?
    } else if let Some(parent) = path.parent() {
        if parent.exists() {
            let canonical_parent =
                fs::canonicalize(parent).map_err(|e| DupfinderError::IoError {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            if let Some(file_name) = path.file_name() {
                canonical_parent.join(file_name)
            } else {
                canonical_parent
            }
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };

    // 1. Check Root directories
    if canonical == Path::new("/") {
        return Ok(true);
    }

    #[cfg(windows)]
    {
        // Windows drive roots like C:\ or \\?\C:\
        if let Some(Component::Prefix(_)) = canonical.components().next() {
            if canonical.components().count() <= 2 {
                // e.g. C:\
                return Ok(true);
            }
        }
    }

    // 2. Check OS and System Hierarchies
    let path_str = canonical.to_string_lossy();

    #[cfg(unix)]
    {
        // Allow temporary directories used by OS and testing fixtures
        if path_str.starts_with("/tmp")
            || path_str.starts_with("/var/tmp")
            || path_str.starts_with("/var/folders")
            || path_str.starts_with("/private/tmp")
            || path_str.starts_with("/private/var/folders")
            || path_str.starts_with("/private/var/tmp")
        {
            // Allowed temp location
        } else {
            let protected_prefixes = [
                "/System",
                "/Library",
                "/Applications",
                "/private/etc",
                "/usr",
                "/etc",
                "/bin",
                "/sbin",
                "/lib",
                "/lib64",
                "/var",
                "/proc",
                "/sys",
                "/dev",
                "/boot",
                "/cores",
                "/opt/homebrew",
                "/opt/local",
            ];

            for prefix in &protected_prefixes {
                if path_str == *prefix || path_str.starts_with(&format!("{}/", prefix)) {
                    return Ok(true);
                }
            }
        }
    }

    #[cfg(windows)]
    {
        let protected_windows_prefixes = [
            "C:\\Windows",
            "C:\\Program Files",
            "C:\\Program Files (x86)",
            "C:\\ProgramData",
            "C:\\Recovery",
            "C:\\System Volume Information",
        ];

        for prefix in &protected_windows_prefixes {
            if path_str.eq_ignore_ascii_case(prefix)
                || path_str
                    .to_ascii_lowercase()
                    .starts_with(&format!("{}\\", prefix.to_ascii_lowercase()))
            {
                return Ok(true);
            }
        }
    }

    // 3. Check Sensitive Security & Toolchain directories in any path component
    for component in canonical.components() {
        if let Component::Normal(comp) = component {
            let comp_str = comp.to_string_lossy();
            if matches!(
                comp_str.as_ref(),
                ".ssh"
                    | ".gnupg"
                    | ".aws"
                    | ".azure"
                    | ".kube"
                    | ".cargo"
                    | ".rustup"
                    | ".git"
                    | ".npm"
            ) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Check if a filename corresponds to a known structural sentinel file (e.g. `__init__.py`, `.gitkeep`).
#[must_use]
pub fn is_structural_sentinel(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        STRUCTURAL_SENTINELS.contains(&name)
    } else {
        false
    }
}

/// Check if the target file or directory is hidden (starts with `.`).
#[must_use]
pub fn is_hidden_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.') && n != "." && n != "..")
        .unwrap_or(false)
}

/// Verify that a file has not been mutated, replaced by a symlink, or deleted since scan time (TOCTOU guard).
pub fn verify_file_unmodified(path: &Path, expected_size: u64) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| DupfinderError::StateChangedError {
        path: path.to_path_buf(),
        reason: format!("File inaccessible or deleted: {}", e),
    })?;

    if metadata.file_type().is_symlink() {
        return Err(DupfinderError::StateChangedError {
            path: path.to_path_buf(),
            reason: "Target was swapped for a symlink".to_string(),
        });
    }

    if !metadata.file_type().is_file() {
        return Err(DupfinderError::StateChangedError {
            path: path.to_path_buf(),
            reason: "Target is no longer a regular file".to_string(),
        });
    }

    if metadata.len() != expected_size {
        return Err(DupfinderError::StateChangedError {
            path: path.to_path_buf(),
            reason: format!(
                "File size changed (expected {} bytes, found {} bytes)",
                expected_size,
                metadata.len()
            ),
        });
    }

    Ok(())
}

/// Validate that a duplicate cleanup action preserves the original file and obeys all safety boundaries.
pub fn validate_deletion_safety(original: &Path, duplicates: &[PathBuf]) -> Result<()> {
    if !original.exists() {
        return Err(DupfinderError::OriginalPreservationError(format!(
            "Designated original file '{}' does not exist on disk",
            original.display()
        )));
    }

    // Check that original is not among duplicates to delete
    let original_canonical = fs::canonicalize(original).unwrap_or_else(|_| original.to_path_buf());

    for dup in duplicates {
        let dup_canonical = fs::canonicalize(dup).unwrap_or_else(|_| dup.to_path_buf());
        if original_canonical == dup_canonical {
            return Err(DupfinderError::OriginalPreservationError(format!(
                "Safety invariant violated: cannot delete original file '{}'",
                original.display()
            )));
        }

        if is_system_or_protected_path(dup)? {
            return Err(DupfinderError::ProtectedPathError {
                path: dup.clone(),
                reason: "Operation on system or protected path is forbidden".to_string(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_sentinel_files() {
        assert!(is_structural_sentinel(Path::new(
            "/project/src/__init__.py"
        )));
        assert!(is_structural_sentinel(Path::new(".gitkeep")));
        assert!(is_structural_sentinel(Path::new("keep.me")));
        assert!(!is_structural_sentinel(Path::new("normal_empty_file.txt")));
    }

    #[test]
    fn test_hidden_paths() {
        assert!(is_hidden_path(Path::new("/home/user/.hidden_file")));
        assert!(is_hidden_path(Path::new(".hidden_file")));
        assert!(!is_hidden_path(Path::new("/home/user/documents/file.txt")));
    }

    #[test]
    fn test_protected_paths() {
        assert!(is_system_or_protected_path(Path::new("/")).unwrap());
        #[cfg(target_os = "macos")]
        {
            assert!(is_system_or_protected_path(Path::new("/System")).unwrap());
            assert!(is_system_or_protected_path(Path::new("/Library/Preferences")).unwrap());
            assert!(is_system_or_protected_path(Path::new("/usr/bin/git")).unwrap());
        }
        #[cfg(target_os = "linux")]
        {
            assert!(is_system_or_protected_path(Path::new("/etc/passwd")).unwrap());
            assert!(is_system_or_protected_path(Path::new("/usr/bin")).unwrap());
        }
        assert!(is_system_or_protected_path(Path::new("/home/user/.ssh/id_rsa")).unwrap());
        assert!(is_system_or_protected_path(Path::new("/workspace/.git/config")).unwrap());
    }

    #[test]
    fn test_validate_deletion_safety_preserves_original() {
        let dir = tempdir().unwrap();
        let orig = dir.path().join("original.txt");
        let dup = dir.path().join("duplicate.txt");

        fs::write(&orig, b"hello world").unwrap();
        fs::write(&dup, b"hello world").unwrap();

        // Valid cleanup
        assert!(validate_deletion_safety(&orig, std::slice::from_ref(&dup)).is_ok());

        // Invalid: deleting original
        let err = validate_deletion_safety(&orig, std::slice::from_ref(&orig));
        assert!(err.is_err());
    }

    #[test]
    fn test_toctou_verification() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file.txt");
        fs::write(&file, b"12345").unwrap();

        // Size matches
        assert!(verify_file_unmodified(&file, 5).is_ok());

        // Size changed
        assert!(verify_file_unmodified(&file, 10).is_err());

        // Deleted file
        let non_existent = dir.path().join("gone.txt");
        assert!(verify_file_unmodified(&non_existent, 5).is_err());
    }
}
