//! `dupfinder cache` subcommand implementation.

use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::PathBuf;

use dupfinder_core::cache;
use dupfinder_core::types::CacheConfig;

/// Cache management actions.
#[derive(Subcommand, Debug)]
pub enum CacheAction {
    /// Show cache statistics.
    Info {
        /// Custom cache directory.
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Delete all cached data.
    Clear {
        /// Custom cache directory.
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
}

/// Run a cache subcommand.
pub fn run(action: CacheAction) -> Result<()> {
    match action {
        CacheAction::Info { cache_dir } => {
            let config = CacheConfig {
                enabled: true,
                cache_dir,
            };
            let info = cache::get_cache_info(&config);

            println!("dupfinder cache info");
            println!("────────────────────────────────────");
            println!("  Cache path:    {}", info.path.display());
            println!("  Exists:        {}", if info.exists { "yes" } else { "no" });
            println!("  Entries:       {}", info.entry_count);
            println!(
                "  File size:     {}",
                format_cache_size(info.file_size_bytes)
            );
            println!("────────────────────────────────────");
        }
        CacheAction::Clear { cache_dir } => {
            let config = CacheConfig {
                enabled: true,
                cache_dir,
            };
            cache::clear_cache(&config).context("Failed to clear cache")?;
            println!("Cache cleared successfully.");
        }
    }

    Ok(())
}

fn format_cache_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
