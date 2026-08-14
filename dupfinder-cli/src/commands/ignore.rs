//! `dupfinder ignore` subcommand implementation for managing ignore rules and presets.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::fs;
use std::path::PathBuf;

use dupfinder_core::ignore::{
    generate_starter_ignore_content, get_global_ignore_path, PRESET_CATALOG,
};

/// Arguments for the `ignore` subcommand.
#[derive(Args, Debug)]
pub struct IgnoreArgs {
    #[command(subcommand)]
    pub action: IgnoreAction,
}

#[derive(Subcommand, Debug)]
pub enum IgnoreAction {
    /// Show built-in default ignore categories, directories, and file patterns.
    #[command(alias = "list")]
    ShowDefaults,

    /// Generate a starter `.dupignore` file.
    Init {
        /// Create the ignore file globally in user config directory instead of current directory.
        #[arg(long, short = 'g')]
        global: bool,

        /// Force overwrite if destination ignore file already exists.
        #[arg(long, short = 'f')]
        force: bool,
    },

    /// Show the path to the global user ignore file.
    Path,
}

pub fn run(args: IgnoreArgs) -> Result<()> {
    match args.action {
        IgnoreAction::ShowDefaults => show_defaults(),
        IgnoreAction::Init { global, force } => init_ignore_file(global, force),
        IgnoreAction::Path => show_path(),
    }
}

fn show_defaults() -> Result<()> {
    println!("Built-in Ignore Categories & Ecosystem Presets:\n");

    for category in PRESET_CATALOG {
        println!("  \x1b[1m{}\x1b[0m", category.name);
        println!("    Description : {}", category.description);

        if !category.dirs.is_empty() {
            println!("    Directories : {}", category.dirs.join(", "));
        }
        if !category.patterns.is_empty() {
            println!("    Patterns    : {}", category.patterns.join(", "));
        }
        println!();
    }

    println!("Presets available via --exclude-preset: default, build, deps, jars, minimal, none");
    println!("Use --include-jars to keep default presets but enable scanning *.jar, *.war, *.ear");
    println!("Use --no-default-ignores to disable all built-in rules.");

    Ok(())
}

fn init_ignore_file(global: bool, force: bool) -> Result<()> {
    let target_path = if global {
        get_global_ignore_path().context("Could not determine global config directory")?
    } else {
        PathBuf::from(".dupignore")
    };

    if target_path.exists() && !force {
        anyhow::bail!(
            "File already exists at: {}\nUse --force to overwrite.",
            target_path.display()
        );
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let starter_content = generate_starter_ignore_content();
    fs::write(&target_path, starter_content)
        .with_context(|| format!("Failed to write ignore file to {}", target_path.display()))?;

    println!("Initialized ignore file at: {}", target_path.display());
    Ok(())
}

fn show_path() -> Result<()> {
    match get_global_ignore_path() {
        Some(path) => {
            println!("Global ignore file location: {}", path.display());
            if path.exists() {
                println!("Status: File exists");
            } else {
                println!(
                    "Status: File does not exist (run 'dupfinder ignore init --global' to create)"
                );
            }
            Ok(())
        }
        None => {
            anyhow::bail!("Could not determine global configuration directory on this platform")
        }
    }
}
