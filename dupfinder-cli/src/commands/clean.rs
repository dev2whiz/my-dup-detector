//! `dupfinder clean` subcommand implementation.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use dupfinder_core::clean::{
    execute_clean_plan, generate_clean_plan, CleanAction, CleanOptions, CleanPlan, DeletionMethod,
    RemediationStrategy,
};
use dupfinder_core::safety::is_elevated_privilege;
use dupfinder_core::types::{CacheConfig, FeatureFlags, FilterConfig, ScanConfig};

use crate::output::CliProgressHandler;

/// Arguments for the `clean` subcommand.
#[derive(Args, Debug)]
pub struct CleanArgs {
    /// One or more directories to scan and clean.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Interactively review each duplicate set before action.
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Move files to OS Recycle Bin / Trash (enabled by default).
    #[arg(long, default_value_t = true)]
    pub trash: bool,

    /// Permanently unlink files from disk (WARNING: cannot be undone).
    #[arg(long)]
    pub permanent: bool,

    /// Replace duplicate files with POSIX/NTFS hardlinks pointing to original file.
    #[arg(long, conflicts_with_all = ["permanent", "reflink"])]
    pub hardlink: bool,

    /// Replace duplicate files with Copy-on-Write clones (macOS clonefile / Linux FICLONE).
    #[arg(long, conflicts_with_all = ["permanent", "hardlink"])]
    pub reflink: bool,

    /// Dry run mode: output planned remediation actions without modifying disk.
    #[arg(long)]
    pub dry_run: bool,

    /// Clean detected empty (zero-byte) files.
    #[arg(long)]
    pub clean_empty_files: bool,

    /// Clean detected empty directories.
    #[arg(long)]
    pub clean_empty_dirs: bool,

    /// Include hidden files/directories (dotfiles) in cleanup.
    #[arg(long)]
    pub include_hidden: bool,

    /// Automatic yes to prompts (non-interactive confirmation).
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Explicitly allow running under root or elevated administrator privileges.
    #[arg(long)]
    pub allow_root: bool,

    /// Write an audit remediation manifest (JSON) to specified path.
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,

    // ── Scan Filtering Options ───────────────────────────────────────────
    /// Minimum file size to consider (e.g., 1024, 1KB, 1MB).
    #[arg(long, default_value = "1")]
    pub min_size: String,

    /// Glob pattern(s) to exclude files (repeatable).
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Directory name(s) to skip (repeatable).
    #[arg(long)]
    pub exclude_dir: Vec<String>,

    /// Built-in ignore preset: default, build, deps, jars, minimal, none.
    #[arg(long, value_name = "PRESET")]
    pub exclude_preset: Option<String>,

    /// Disable built-in default ignore presets.
    #[arg(long)]
    pub no_default_ignores: bool,

    /// Explicit custom ignore file to load (repeatable).
    #[arg(long = "ignore-file", value_name = "PATH")]
    pub ignore_files: Vec<PathBuf>,

    /// Scan JAR and archive files.
    #[arg(long)]
    pub include_jars: bool,

    /// Maximum recursion depth.
    #[arg(long, short = 'd')]
    pub depth: Option<usize>,

    /// Suppress scan progress indicators.
    #[arg(long)]
    pub quiet: bool,
}

/// Execute the `clean` subcommand.
pub fn run(args: CleanArgs) -> Result<()> {
    // 1. Elevated privilege guardrail
    if is_elevated_privilege() && !args.allow_root {
        eprintln!("\x1b[1;33m⚠️  SECURITY WARNING: Running as root / Administrator!\x1b[0m");
        eprintln!("Operating with elevated privileges can unintentionally affect system files or permissions.");
        eprintln!("To proceed anyway, explicitly supply the `--allow-root` flag.\n");
        bail!("Execution blocked: running as root/administrator without --allow-root");
    }

    // Determine remediation strategy
    let strategy = if args.hardlink {
        RemediationStrategy::Hardlink
    } else if args.reflink {
        RemediationStrategy::Reflink
    } else if args.permanent {
        RemediationStrategy::Delete(DeletionMethod::Permanent)
    } else {
        RemediationStrategy::Delete(DeletionMethod::Trash)
    };

    // 2. Configure scan parameters
    let min_size_bytes = parse_size_str(&args.min_size)
        .with_context(|| format!("Invalid --min-size value '{}'", args.min_size))?;

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

    let filter_config = FilterConfig {
        min_size: min_size_bytes,
        exclude_patterns: args.exclude,
        exclude_dirs: args.exclude_dir,
        preset,
        include_jars: args.include_jars,
        use_global_ignore: true,
        use_project_ignore: true,
        custom_ignore_files: args.ignore_files,
    };

    let scan_config = ScanConfig {
        paths: args.paths.clone(),
        max_depth: args.depth,
        include_hidden: args.include_hidden,
        features: FeatureFlags {
            duplicates: true,
            empty_files: args.clean_empty_files,
            empty_dirs: args.clean_empty_dirs,
            broken_links: false,
            similar_images: false,
            similarity_threshold: 0.90,
        },
        filters: filter_config,
        cache_config: CacheConfig::default(),
    };

    // 3. Run scan
    let progress_handler = CliProgressHandler::new(args.quiet);
    let report = dupfinder_core::scan(scan_config, &progress_handler)
        .context("Failed during preliminary filesystem scan")?;

    // 4. Generate clean plan
    let clean_options = CleanOptions {
        strategy,
        clean_empty_files: args.clean_empty_files,
        clean_empty_dirs: args.clean_empty_dirs,
        include_hidden: args.include_hidden,
        dry_run: args.dry_run,
    };

    let mut plan = generate_clean_plan(&report, &clean_options)
        .context("Failed to generate safe cleanup plan")?;

    if plan.actions.is_empty() {
        println!("\n✨ No duplicate or redundant files found to remediate.");
        return Ok(());
    }

    // 5. Interactive review mode
    if args.interactive && !args.dry_run {
        plan = run_interactive_review(plan, strategy)?;
    }

    // 6. Display Plan Summary
    print_plan_summary(&plan, strategy, args.dry_run);

    if plan.total_files_to_remediate == 0 && plan.total_dirs_to_remove == 0 {
        if plan.already_linked_count > 0 {
            println!("\nAll duplicates are already hardlinked to their originals.");
        } else {
            println!("\nNothing marked for remediation.");
        }
        return Ok(());
    }

    // 7. Confirmation prompt if not dry-run and not auto-confirmed
    if !args.dry_run && !args.yes && !args.interactive && !confirm_action(strategy)? {
        println!("Operation cancelled by user.");
        return Ok(());
    }

    // 8. Execute Clean Plan
    let exec_res = execute_clean_plan(&plan, strategy, args.dry_run, &progress_handler)
        .context("Failed while executing remediation operations")?;

    // 9. Output Execution Summary
    print_execution_summary(&exec_res, args.dry_run);

    // 10. Write Manifest if requested
    if let Some(manifest_path) = args.manifest {
        let manifest_json = serde_json::to_string_pretty(&exec_res.manifest)
            .context("Failed to serialize remediation manifest")?;
        fs::write(&manifest_path, manifest_json).with_context(|| {
            format!("Failed to write manifest to '{}'", manifest_path.display())
        })?;
        println!(
            "📝 Remediation audit manifest saved to '{}'",
            manifest_path.display()
        );
    }

    Ok(())
}

fn print_plan_summary(plan: &CleanPlan, strategy: RemediationStrategy, dry_run: bool) {
    let mode_str = if dry_run {
        "DRY RUN (Simulated - No Files Modified)"
    } else {
        match strategy {
            RemediationStrategy::Delete(DeletionMethod::Trash) => "OS Trash / Recycle Bin",
            RemediationStrategy::Delete(DeletionMethod::Permanent) => "Permanent Unlink",
            RemediationStrategy::Hardlink => "Hardlink Deduplication (POSIX/NTFS)",
            RemediationStrategy::Reflink => "Copy-on-Write (Reflink / APFS clone)",
        }
    };

    println!("\n========================================================");
    println!("           DUPFINDER REMEDIATION PLAN                   ");
    println!("========================================================");
    println!(" Mode:                  {}", mode_str);
    println!(" Target Files:          {}", plan.total_files_to_remediate);
    println!(" Empty Directories:     {}", plan.total_dirs_to_remove);
    println!(
        " Reclaimable Space:     {}",
        format_bytes(plan.total_bytes_reclaimable)
    );
    if plan.already_linked_count > 0 {
        println!(" 🔗 Already Linked:      {}", plan.already_linked_count);
    }
    if plan.blocked_count > 0 {
        println!(" ⚠️ Blocked (Protected): {}", plan.blocked_count);
    }
    if plan.skipped_count > 0 {
        println!(" ℹ️ Skipped (Hidden/Sent): {}", plan.skipped_count);
    }
    println!("--------------------------------------------------------");
}

fn print_execution_summary(res: &dupfinder_core::clean::CleanExecutionResult, dry_run: bool) {
    println!("\n========================================================");
    if dry_run {
        println!("           DRY-RUN SIMULATION COMPLETE                  ");
    } else {
        println!("           REMEDIATION EXECUTION COMPLETE               ");
    }
    println!("========================================================");
    println!(" Files Processed:       {}", res.succeeded_files);
    println!(" Directories Removed:   {}", res.succeeded_dirs);
    println!(
        " Space Reclaimed:       {}",
        format_bytes(res.bytes_reclaimed)
    );
    if res.already_linked > 0 {
        println!(" Already Linked:        {}", res.already_linked);
    }
    if !res.failed.is_empty() {
        println!(" ❌ Errors Encountered: {}", res.failed.len());
        for (path, err) in &res.failed {
            println!("   - {}: {}", path.display(), err);
        }
    }
    println!("========================================================\n");
}

fn confirm_action(strategy: RemediationStrategy) -> Result<bool> {
    let prompt = match strategy {
        RemediationStrategy::Delete(DeletionMethod::Trash) => {
            "Proceed to move duplicate files to Trash? [y/N]: "
        }
        RemediationStrategy::Delete(DeletionMethod::Permanent) => {
            "\x1b[1;31mWARNING: This will permanently delete files. Proceed? [y/N]: \x1b[0m"
        }
        RemediationStrategy::Hardlink => {
            "Proceed to replace duplicate files with hardlinks to originals? [y/N]: "
        }
        RemediationStrategy::Reflink => {
            "Proceed to replace duplicate files with CoW (reflink) clones? [y/N]: "
        }
    };

    print!("{}", prompt);
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    let stdin = io::stdin();
    stdin
        .lock()
        .read_line(&mut input)
        .context("Failed to read user input")?;

    Ok(input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes"))
}

fn run_interactive_review(mut plan: CleanPlan, strategy: RemediationStrategy) -> Result<CleanPlan> {
    let mut approved_actions = Vec::new();
    let mut apply_all = false;
    let mut total_files = 0;
    let mut total_bytes = 0;

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    let action_verb = match strategy {
        RemediationStrategy::Delete(_) => "delete",
        RemediationStrategy::Hardlink => "hardlink",
        RemediationStrategy::Reflink => "reflink",
    };

    for action in plan.actions {
        if apply_all {
            if let CleanAction::DeleteDuplicate { size, .. }
            | CleanAction::HardlinkDuplicate { size, .. }
            | CleanAction::ReflinkDuplicate { size, .. } = &action
            {
                total_files += 1;
                total_bytes += *size;
            }
            approved_actions.push(action);
            continue;
        }

        match &action {
            CleanAction::DeleteDuplicate {
                path,
                size,
                original,
            }
            | CleanAction::HardlinkDuplicate {
                path,
                size,
                original,
            }
            | CleanAction::ReflinkDuplicate {
                path,
                size,
                original,
            } => {
                println!("\n--------------------------------------------------------");
                println!("Original:  {}", original.display());
                println!("Duplicate: {} ({})", path.display(), format_bytes(*size));
                print!(
                    "Action: [k]eep original & {} dup, [s]kip, [a]pply all, [q]uit? ",
                    action_verb
                );
                io::stdout().flush().context("Failed to flush stdout")?;

                let mut choice = String::new();
                reader
                    .read_line(&mut choice)
                    .context("Failed to read input")?;
                let choice = choice.trim().to_lowercase();

                match choice.as_str() {
                    "k" | "y" | "" => {
                        total_files += 1;
                        total_bytes += *size;
                        approved_actions.push(action);
                    }
                    "a" => {
                        apply_all = true;
                        total_files += 1;
                        total_bytes += *size;
                        approved_actions.push(action);
                    }
                    "q" => {
                        println!("Interactive review aborted.");
                        break;
                    }
                    _ => {
                        println!("Skipped duplicate: {}", path.display());
                    }
                }
            }
            other => {
                approved_actions.push(other.clone());
            }
        }
    }

    plan.actions = approved_actions;
    plan.total_files_to_remediate = total_files;
    plan.total_bytes_reclaimable = total_bytes;
    Ok(plan)
}

fn parse_size_str(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }

    let (num_part, unit_part) = match s.find(|c: char| c.is_alphabetic()) {
        Some(idx) => (&s[..idx], &s[idx..]),
        None => (s, ""),
    };

    let number: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number '{}' in size", num_part))?;

    let multiplier: u64 = match unit_part.to_lowercase().as_str() {
        "" | "b" | "bytes" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        "t" | "tb" | "tib" => 1024 * 1024 * 1024 * 1024,
        _ => bail!("Unknown size unit '{}'. Use B, KB, MB, GB, TB", unit_part),
    };

    Ok((number * multiplier as f64) as u64)
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
