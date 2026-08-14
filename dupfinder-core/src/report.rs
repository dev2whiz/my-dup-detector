//! Report generation in JSON and human-readable text formats.

use chrono::{DateTime, Utc};
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::Path;

use crate::errors::Result;
use crate::types::ScanReport;

/// Output format for reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Text,
    Json,
}

/// Generate a report in the specified format and return it as a string.
pub fn generate_report(report: &ScanReport, format: ReportFormat) -> Result<String> {
    match format {
        ReportFormat::Text => generate_text_report(report),
        ReportFormat::Json => generate_json_report(report),
    }
}

/// Write a report to a file, inferring format from extension.
pub fn write_report_to_file(report: &ScanReport, path: &Path) -> Result<()> {
    let format = match path.extension().and_then(|e| e.to_str()) {
        Some("json") => ReportFormat::Json,
        _ => ReportFormat::Text,
    };

    let content = generate_report(report, format)?;
    let mut file =
        std::fs::File::create(path).map_err(|e| crate::errors::DupfinderError::IoError {
            path: path.to_path_buf(),
            source: e,
        })?;
    file.write_all(content.as_bytes())
        .map_err(|e| crate::errors::DupfinderError::IoError {
            path: path.to_path_buf(),
            source: e,
        })?;

    Ok(())
}

/// Generate a JSON report.
fn generate_json_report(report: &ScanReport) -> Result<String> {
    let json = serde_json::to_string_pretty(report)?;
    Ok(json)
}

/// Generate a human-readable text report.
fn generate_text_report(report: &ScanReport) -> Result<String> {
    let mut out = String::new();

    let separator = "═".repeat(65);
    let thin_sep = "─".repeat(65);

    // Header
    writeln!(out, "{}", separator).unwrap();
    writeln!(out, "  dupfinder v{} — Scan Report", report.version).unwrap();
    writeln!(
        out,
        "  Scanned: {}",
        report
            .scan_info
            .paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
    .unwrap();
    writeln!(out, "  Date: {}", report.scan_info.timestamp).unwrap();
    writeln!(out, "  Duration: {:.1}s", report.scan_info.duration_secs).unwrap();
    writeln!(out, "{}", separator).unwrap();
    writeln!(out).unwrap();

    // Duplicates
    if !report.duplicates.groups.is_empty() {
        writeln!(out, "── Duplicate Files {}", thin_sep).unwrap();
        writeln!(
            out,
            "  Found {} duplicate group{} ({} redundant file{}, {} reclaimable)",
            report.duplicates.total_groups,
            if report.duplicates.total_groups == 1 {
                ""
            } else {
                "s"
            },
            report.duplicates.total_redundant_files,
            if report.duplicates.total_redundant_files == 1 {
                ""
            } else {
                "s"
            },
            format_bytes(report.duplicates.reclaimable_bytes),
        )
        .unwrap();
        writeln!(out).unwrap();

        for (i, group) in report.duplicates.groups.iter().enumerate() {
            let total_files = 1 + group.duplicates.len();
            writeln!(
                out,
                "  Group {} — {} files, {} each (BLAKE3: {}…)",
                i + 1,
                total_files,
                format_bytes(group.size),
                &group.hash[..12.min(group.hash.len())],
            )
            .unwrap();

            let orig_mtime = format_mtime(&group.original.mtime);
            writeln!(
                out,
                "    [ORIGINAL]  {}  ({})",
                group.original.path.display(),
                orig_mtime,
            )
            .unwrap();

            for dup in &group.duplicates {
                let dup_mtime = format_mtime(&dup.mtime);
                writeln!(
                    out,
                    "    [DUPLICATE] {}  ({})",
                    dup.path.display(),
                    dup_mtime,
                )
                .unwrap();
            }
            writeln!(out).unwrap();
        }
    } else {
        writeln!(out, "── Duplicate Files {}", thin_sep).unwrap();
        writeln!(out, "  No duplicates found.").unwrap();
        writeln!(out).unwrap();
    }

    // Empty files
    if !report.empty_files.is_empty() {
        writeln!(out, "── Empty Files {}", thin_sep).unwrap();
        writeln!(
            out,
            "  Found {} empty file{}",
            report.empty_files.len(),
            if report.empty_files.len() == 1 {
                ""
            } else {
                "s"
            },
        )
        .unwrap();
        writeln!(out).unwrap();
        for path in &report.empty_files {
            writeln!(out, "    {}", path.display()).unwrap();
        }
        writeln!(out).unwrap();
    }

    // Empty directories
    if !report.empty_dirs.is_empty() {
        writeln!(out, "── Empty Directories {}", thin_sep).unwrap();
        writeln!(
            out,
            "  Found {} empty director{}",
            report.empty_dirs.len(),
            if report.empty_dirs.len() == 1 {
                "y"
            } else {
                "ies"
            },
        )
        .unwrap();
        writeln!(out).unwrap();
        for path in &report.empty_dirs {
            writeln!(out, "    {}/", path.display()).unwrap();
        }
        writeln!(out).unwrap();
    }

    // Broken symlinks
    if !report.broken_symlinks.is_empty() {
        writeln!(out, "── Broken Symbolic Links {}", thin_sep).unwrap();
        writeln!(
            out,
            "  Found {} broken symbolic link{}",
            report.broken_symlinks.len(),
            if report.broken_symlinks.len() == 1 {
                ""
            } else {
                "s"
            },
        )
        .unwrap();
        writeln!(out).unwrap();
        for link in &report.broken_symlinks {
            writeln!(
                out,
                "    {} → {} (target missing)",
                link.link_path.display(),
                link.target_path.display(),
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Summary
    writeln!(out, "── Summary {}", thin_sep).unwrap();
    writeln!(
        out,
        "  Scanned:          {:>8} files in {} directories",
        format_number(report.scan_info.files_scanned),
        format_number(report.scan_info.dirs_scanned),
    )
    .unwrap();

    if report.duplicates.total_groups > 0 {
        writeln!(
            out,
            "  Duplicates:       {} group{} ({} redundant, {} reclaimable)",
            report.duplicates.total_groups,
            if report.duplicates.total_groups == 1 {
                ""
            } else {
                "s"
            },
            report.duplicates.total_redundant_files,
            format_bytes(report.duplicates.reclaimable_bytes),
        )
        .unwrap();
    } else {
        writeln!(out, "  Duplicates:       None").unwrap();
    }

    writeln!(out, "  Empty files:      {}", report.empty_files.len()).unwrap();
    writeln!(out, "  Empty dirs:       {}", report.empty_dirs.len()).unwrap();
    writeln!(out, "  Broken symlinks:  {}", report.broken_symlinks.len()).unwrap();

    let total = report.cache_stats.hits + report.cache_stats.misses;
    if total > 0 {
        writeln!(
            out,
            "  Cache hits:       {} / {} ({:.1}%)",
            format_number(report.cache_stats.hits),
            format_number(total),
            report.cache_stats.hit_rate() * 100.0,
        )
        .unwrap();
    }

    writeln!(out, "{}", separator).unwrap();

    Ok(out)
}

/// Format a byte count as a human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format a number with thousands separators.
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Format a SystemTime as a date string.
fn format_mtime(mtime: &std::time::SystemTime) -> String {
    let datetime: DateTime<Utc> = (*mtime).into();
    datetime.format("%Y-%m-%d").to_string()
}
