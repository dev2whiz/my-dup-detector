//! `dupfinder scan` subcommand implementation.

use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use dupfinder_core::report::{self, ReportFormat};
use dupfinder_core::types::{CacheConfig, FeatureFlags, FilterConfig, ScanConfig};

use crate::output::CliProgressHandler;

/// Arguments for the `scan` subcommand.
#[derive(Args, Debug)]
pub struct ScanArgs {
    /// One or more directories to scan.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    // ── Detection feature toggles ────────────────────────────────────────
    /// Disable duplicate file detection.
    #[arg(long)]
    pub no_duplicates: bool,

    /// Disable empty file detection.
    #[arg(long)]
    pub no_empty_files: bool,

    /// Disable empty directory detection.
    #[arg(long)]
    pub no_empty_dirs: bool,

    /// Disable broken symlink detection.
    #[arg(long)]
    pub no_broken_links: bool,

    // ── Filtering ────────────────────────────────────────────────────────
    /// Minimum file size to consider (e.g., 1024, 1KB, 1MB).
    #[arg(long, default_value = "1")]
    pub min_size: String,

    /// Glob pattern(s) to exclude files (repeatable).
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Directory name(s) to skip (repeatable).
    #[arg(long)]
    pub exclude_dir: Vec<String>,

    /// Include hidden files/directories (dotfiles).
    #[arg(long)]
    pub include_hidden: bool,

    /// Built-in ignore preset: default, build, deps, jars, minimal, none.
    #[arg(long, value_name = "PRESET")]
    pub exclude_preset: Option<String>,

    /// Disable built-in default ignore presets.
    #[arg(long)]
    pub no_default_ignores: bool,

    /// Explicit custom ignore file to load (repeatable).
    #[arg(long = "ignore-file", value_name = "PATH")]
    pub ignore_files: Vec<PathBuf>,

    /// Scan JAR and archive files (overrides exclusion in presets).
    #[arg(long)]
    pub include_jars: bool,

    /// Disable loading global ignore file (~/.config/dupfinder/dupignore).
    #[arg(long)]
    pub no_global_ignore: bool,

    /// Disable loading local project ignore files (<scan_root>/.dupignore).
    #[arg(long)]
    pub no_project_ignore: bool,

    /// Maximum recursion depth (0 = current directory only).
    #[arg(long, short = 'd')]
    pub depth: Option<usize>,

    // ── Output ───────────────────────────────────────────────────────────
    /// Write report to a file (format inferred from extension: .json, .txt).
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Output format: text, json.
    #[arg(long, short = 'f', default_value = "text")]
    pub format: String,

    /// Suppress progress output; only print final report.
    #[arg(long)]
    pub quiet: bool,

    /// Show detailed scan information.
    #[arg(long)]
    pub verbose: bool,

    // ── Cache ────────────────────────────────────────────────────────────
    /// Disable hash caching for this scan.
    #[arg(long)]
    pub no_cache: bool,

    /// Custom cache directory.
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
}

/// Parse a human-readable size string like "1KB", "10MB", or "1024" into bytes.
fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();

    if let Ok(n) = s.parse::<u64>() {
        return Ok(n);
    }

    // Try parsing with suffix
    let suffixes = [
        ("TB", 1024u64 * 1024 * 1024 * 1024),
        ("GB", 1024u64 * 1024 * 1024),
        ("MB", 1024u64 * 1024),
        ("KB", 1024u64),
        ("B", 1u64),
    ];

    for (suffix, multiplier) in &suffixes {
        if s.ends_with(suffix) {
            let num_str = s.trim_end_matches(suffix).trim();
            let num: f64 = num_str
                .parse()
                .with_context(|| format!("Invalid size value: {}", s))?;
            return Ok((num * *multiplier as f64) as u64);
        }
    }

    anyhow::bail!("Invalid size format: {}. Use e.g., 1024, 1KB, 10MB", s);
}

/// Run the scan subcommand.
pub fn run(args: ScanArgs) -> Result<()> {
    // Validate paths
    for path in &args.paths {
        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
        if !path.is_dir() {
            anyhow::bail!("Path is not a directory: {}", path.display());
        }
    }

    let min_size = parse_size(&args.min_size)?;

    let preset = if args.no_default_ignores {
        None
    } else if let Some(ref preset_name) = args.exclude_preset {
        match dupfinder_core::ignore::IgnorePreset::from_str_name(preset_name) {
            Some(p) => Some(p),
            None => anyhow::bail!(
                "Invalid preset: '{}'. Valid presets: default, build, deps, jars, minimal, none",
                preset_name
            ),
        }
    } else {
        Some(dupfinder_core::ignore::IgnorePreset::Default)
    };

    let config = ScanConfig {
        paths: args.paths,
        features: FeatureFlags {
            duplicates: !args.no_duplicates,
            empty_files: !args.no_empty_files,
            empty_dirs: !args.no_empty_dirs,
            broken_links: !args.no_broken_links,
        },
        filters: FilterConfig {
            min_size,
            exclude_patterns: args.exclude,
            exclude_dirs: args.exclude_dir,
            preset,
            include_jars: args.include_jars,
            use_global_ignore: !args.no_global_ignore,
            use_project_ignore: !args.no_project_ignore,
            custom_ignore_files: args.ignore_files,
        },
        cache_config: CacheConfig {
            enabled: !args.no_cache,
            cache_dir: args.cache_dir,
        },
        max_depth: args.depth,
        include_hidden: args.include_hidden,
    };

    // Set up progress handler
    let progress = CliProgressHandler::new(args.quiet);

    // Run the scan
    let scan_report = dupfinder_core::scan(config, &progress).context("Scan failed")?;

    // Finish progress bars
    progress.finish();

    // Determine output format
    let format = match args.format.to_lowercase().as_str() {
        "json" => ReportFormat::Json,
        "text" | "txt" => ReportFormat::Text,
        other => anyhow::bail!("Unknown format: {}. Use 'text' or 'json'", other),
    };

    // Generate and output report
    let report_str =
        report::generate_report(&scan_report, format).context("Failed to generate report")?;

    // Write to file if specified
    if let Some(ref output_path) = args.output {
        report::write_report_to_file(&scan_report, output_path)
            .context("Failed to write report file")?;
        if !args.quiet {
            eprintln!("\nReport written to: {}", output_path.display());
        }
    }

    // Always print to stdout (unless writing to file and quiet)
    if args.output.is_none() || !args.quiet {
        println!("{}", report_str);
    }

    Ok(())
}
