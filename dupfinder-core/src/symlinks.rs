//! Broken symbolic link detection.

use std::path::PathBuf;

use crate::progress::ProgressHandler;
use crate::types::{BrokenSymlink, ScanPhase};

/// Find broken symbolic links from the list of discovered symlinks.
///
/// A symlink is "broken" if its target does not exist or is inaccessible.
pub fn find_broken_symlinks(
    symlinks: &[(PathBuf, PathBuf)],
    progress: &dyn ProgressHandler,
) -> Vec<BrokenSymlink> {
    progress.on_phase_start(ScanPhase::BrokenLinkDetection, Some(symlinks.len() as u64));

    let mut broken = Vec::new();

    for (link_path, target_path) in symlinks {
        // Resolve the symlink target relative to the link's parent directory
        let resolved_target = if target_path.is_absolute() {
            target_path.clone()
        } else {
            link_path
                .parent()
                .unwrap_or(link_path)
                .join(target_path)
        };

        // Check if the resolved target exists
        if !resolved_target.exists() {
            broken.push(BrokenSymlink {
                link_path: link_path.clone(),
                target_path: target_path.clone(),
            });
        }
    }

    progress.on_progress(
        ScanPhase::BrokenLinkDetection,
        broken.len() as u64,
        &format!("Found {} broken symlinks", broken.len()),
    );
    progress.on_phase_end(ScanPhase::BrokenLinkDetection);

    broken
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::SilentProgress;
    use tempfile::TempDir;

    #[test]
    fn test_broken_symlink_detection() {
        let dir = TempDir::new().unwrap();

        // Create a valid target file
        let valid_target = dir.path().join("exists.txt");
        std::fs::write(&valid_target, "content").unwrap();

        // Create symlinks
        let valid_link = dir.path().join("valid_link");
        let broken_link = dir.path().join("broken_link");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&valid_target, &valid_link).unwrap();
            std::os::unix::fs::symlink(
                dir.path().join("nonexistent"),
                &broken_link,
            )
            .unwrap();
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, just test with no symlinks
            let symlinks: Vec<(PathBuf, PathBuf)> = vec![];
            let broken = find_broken_symlinks(&symlinks, &SilentProgress);
            assert_eq!(broken.len(), 0);
            return;
        }

        let symlinks = vec![
            (valid_link.clone(), valid_target.clone()),
            (broken_link.clone(), dir.path().join("nonexistent")),
        ];

        let broken = find_broken_symlinks(&symlinks, &SilentProgress);
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].link_path, broken_link);
    }
}
