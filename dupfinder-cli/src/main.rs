//! `dupfinder` CLI — command-line interface for duplicate file detection.

mod commands;
mod output;

use clap::{Parser, Subcommand};

/// Fast, cross-platform duplicate file detection and cleanup recommendation tool.
#[derive(Parser, Debug)]
#[command(name = "dupfinder", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Scan directories for duplicates, empty files/folders, and broken symlinks.
    Scan(commands::scan::ScanArgs),
    /// Manage the hash cache.
    Cache {
        #[command(subcommand)]
        action: commands::cache::CacheAction,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Scan(args) => commands::scan::run(args),
        Commands::Cache { action } => commands::cache::run(action),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
