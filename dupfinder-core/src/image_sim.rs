//! Perceptual image hashing and visual similarity detection.
//!
//! Uses perceptual hashing (dHash / blockhash) to identify images that are visually
//! similar or duplicate despite compression artifacts, resizing, format conversion,
//! or metadata modifications.

use image_hasher::{HasherConfig, ImageHash};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::Path;

use crate::errors::DupfinderError;
use crate::progress::ProgressHandler;
use crate::types::{
    FileEntry, ScanPhase, SimilarImageEntry, SimilarImageGroup, SimilarImageReport,
};

/// Supported image extensions for perceptual hashing analysis.
const SUPPORTED_IMAGE_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "webp", "bmp", "gif", "tiff", "tif"];

/// Returns `true` if the file extension corresponds to a supported image format.
pub fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext_lower = ext.to_lowercase();
            SUPPORTED_IMAGE_EXTENSIONS.contains(&ext_lower.as_str())
        })
        .unwrap_or(false)
}

/// Compute perceptual hash for a single image file on disk.
pub fn compute_perceptual_hash(path: &Path) -> Result<ImageHash, DupfinderError> {
    let img = image::open(path).map_err(|e| DupfinderError::IoError {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;

    let hasher = HasherConfig::new().hash_size(8, 8).to_hasher();
    Ok(hasher.hash_image(&img))
}

/// Convert Hamming distance between two 64-bit hashes to a similarity percentage [0.0, 100.0].
pub fn distance_to_similarity_pct(distance: u32, bit_length: u32) -> f64 {
    if bit_length == 0 {
        return 100.0;
    }
    let clamped_dist = distance.min(bit_length);
    ((bit_length - clamped_dist) as f64 / bit_length as f64) * 100.0
}

/// Scan a set of file entries for visually similar images based on perceptual hashing.
pub fn find_similar_images(
    files: &[FileEntry],
    similarity_threshold: f64,
    progress: &dyn ProgressHandler,
) -> Result<SimilarImageReport, DupfinderError> {
    // 1. Filter files with supported image extensions
    let candidate_files: Vec<&FileEntry> = files
        .iter()
        .filter(|f| is_supported_image(&f.path))
        .collect();

    if candidate_files.len() < 2 {
        return Ok(SimilarImageReport::default());
    }

    progress.on_phase_start(
        ScanPhase::PerceptualHashing,
        Some(candidate_files.len() as u64),
    );

    // 2. Compute perceptual hashes in parallel
    // Maximum allowable Hamming distance based on similarity threshold (64-bit hash)
    let max_distance = (((1.0 - similarity_threshold.clamp(0.0, 1.0)) * 64.0).floor()) as u32;

    let hashed_images: Vec<(&FileEntry, ImageHash)> = candidate_files
        .par_iter()
        .filter_map(|&entry| match compute_perceptual_hash(&entry.path) {
            Ok(hash) => {
                progress.on_progress(ScanPhase::PerceptualHashing, 1, "");
                Some((entry, hash))
            }
            Err(_) => {
                // Gracefully skip unreadable or corrupted images
                progress.on_progress(ScanPhase::PerceptualHashing, 1, "");
                None
            }
        })
        .collect();

    progress.on_phase_end(ScanPhase::PerceptualHashing);

    // 3. Cluster similar images by Hamming distance
    let mut visited: HashSet<usize> = HashSet::new();
    let mut groups: Vec<SimilarImageGroup> = Vec::new();
    let mut total_similar_files = 0;
    let mut reclaimable_bytes = 0;

    for i in 0..hashed_images.len() {
        if visited.contains(&i) {
            continue;
        }

        let (anchor_entry, ref anchor_hash) = hashed_images[i];
        let mut cluster_members: Vec<SimilarImageEntry> = Vec::new();

        for (j, (target_entry, target_hash)) in hashed_images.iter().enumerate().skip(i + 1) {
            if visited.contains(&j) {
                continue;
            }

            let dist = anchor_hash.dist(target_hash);

            if dist <= max_distance {
                let similarity_pct = distance_to_similarity_pct(dist, 64);
                cluster_members.push(SimilarImageEntry {
                    file: (*target_entry).clone(),
                    similarity_percentage: similarity_pct,
                    distance: dist,
                });
                visited.insert(j);
            }
        }

        if !cluster_members.is_empty() {
            visited.insert(i);

            // Calculate cluster reclaimable bytes
            for member in &cluster_members {
                reclaimable_bytes += member.file.size;
                total_similar_files += 1;
            }

            // Canonical is the anchor image (or earliest mtime)
            groups.push(SimilarImageGroup {
                canonical: (*anchor_entry).clone(),
                similar_files: cluster_members,
                perceptual_hash: anchor_hash.to_base64(),
            });
        }
    }

    // Sort groups by reclaimable size descending
    groups.sort_by(|a, b| {
        let size_a: u64 = a.similar_files.iter().map(|f| f.file.size).sum();
        let size_b: u64 = b.similar_files.iter().map(|f| f.file.size).sum();
        size_b.cmp(&size_a)
    });

    let total_groups = groups.len();

    Ok(SimilarImageReport {
        total_groups,
        total_similar_files,
        reclaimable_bytes,
        groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};
    use std::time::SystemTime;
    use tempfile::tempdir;

    use crate::progress::SilentProgress;

    #[test]
    fn test_is_supported_image() {
        assert!(is_supported_image(Path::new("photo.jpg")));
        assert!(is_supported_image(Path::new("photo.JPEG")));
        assert!(is_supported_image(Path::new("image.PNG")));
        assert!(is_supported_image(Path::new("graphic.webp")));
        assert!(is_supported_image(Path::new("icon.bmp")));
        assert!(!is_supported_image(Path::new("document.pdf")));
        assert!(!is_supported_image(Path::new("archive.zip")));
        assert!(!is_supported_image(Path::new("script.rs")));
    }

    #[test]
    fn test_identical_and_similar_images_detection() {
        let dir = tempdir().unwrap();

        // 1. Create a base RGB image (100x100 gradient)
        let mut img1 = RgbImage::new(100, 100);
        for (x, y, pixel) in img1.enumerate_pixels_mut() {
            *pixel = Rgb([(x * 2) as u8, (y * 2) as u8, 128]);
        }
        let path1 = dir.path().join("img1.png");
        img1.save(&path1).unwrap();

        // 2. Create identical copy with different filename
        let path2 = dir.path().join("img2_copy.png");
        img1.save(&path2).unwrap();

        // 3. Create a slightly resized version (50x50)
        let dynamic = DynamicImage::ImageRgb8(img1);
        let img3_resized = dynamic.resize(50, 50, image::imageops::FilterType::Nearest);
        let path3 = dir.path().join("img3_resized.png");
        img3_resized.save(&path3).unwrap();

        // 4. Create a completely different image (white vs dark)
        let mut img4_diff = RgbImage::new(100, 100);
        for pixel in img4_diff.pixels_mut() {
            *pixel = Rgb([255, 255, 255]);
        }
        let path4 = dir.path().join("img4_diff.png");
        img4_diff.save(&path4).unwrap();

        let entries = vec![
            FileEntry {
                path: path1.clone(),
                size: 1000,
                mtime: SystemTime::UNIX_EPOCH,
            },
            FileEntry {
                path: path2.clone(),
                size: 1000,
                mtime: SystemTime::UNIX_EPOCH,
            },
            FileEntry {
                path: path3.clone(),
                size: 500,
                mtime: SystemTime::UNIX_EPOCH,
            },
            FileEntry {
                path: path4.clone(),
                size: 1000,
                mtime: SystemTime::UNIX_EPOCH,
            },
        ];

        let progress = SilentProgress;
        let report = find_similar_images(&entries, 0.90, &progress).unwrap();

        // Should cluster img1, img2, and img3 together
        assert_eq!(report.total_groups, 1);
        assert_eq!(report.total_similar_files, 2);

        let group = &report.groups[0];
        assert_eq!(group.canonical.path, path1);

        let matched_paths: Vec<_> = group.similar_files.iter().map(|f| &f.file.path).collect();
        assert!(matched_paths.contains(&&path2));
        assert!(matched_paths.contains(&&path3));
        assert!(!matched_paths.contains(&&path4));
    }

    #[test]
    fn test_corrupt_images_handled_gracefully() {
        let dir = tempdir().unwrap();
        let corrupt_path = dir.path().join("corrupt.png");
        std::fs::write(&corrupt_path, b"not a real image data").unwrap();

        let entries = vec![FileEntry {
            path: corrupt_path,
            size: 21,
            mtime: SystemTime::UNIX_EPOCH,
        }];

        let progress = SilentProgress;
        let report = find_similar_images(&entries, 0.90, &progress).unwrap();
        assert_eq!(report.total_groups, 0);
    }
}
