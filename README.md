# dupfinder

[![GitHub Release](https://img.shields.io/github/v/release/dev2whiz/my-dup-detector?style=flat-square&color=blue)](https://github.com/dev2whiz/my-dup-detector/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg?style=flat-square)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rustc-1.70%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![Platform Support](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg?style=flat-square)](https://github.com/dev2whiz/my-dup-detector)

A fast, cross-platform CLI tool for detecting duplicate files, similar images, empty files/folders, and broken symlinks with safe remediation guardrails and interactive terminal inspection.

Inspired by [czkawka](https://github.com/qarmin/czkawka) and [rmlint](https://github.com/sahib/rmlint).

## Features

- **Duplicate file detection** — 3-stage pipeline (size → partial hash → full hash) using BLAKE3
- **Perceptual image similarity detection** — visual clustering (`dHash`/`blockhash`) across JPEG, PNG, WebP, BMP, GIF, and TIFF (`--similar-images`)
- **Safe remediation & cleanup** — OS Recycle Bin / Trash integration by default, with dry-run previews and audit manifest exports (`dupfinder clean`)
- **Hardlink & CoW Reflink deduplication** — replace duplicates with space-saving hardlinks or Copy-on-Write clones (`--hardlink`, `--reflink`)
- **Interactive Terminal UI (TUI)** — dual-pane visual inspector with metadata comparisons, text/hex previews, and keyboard controls (`dupfinder tui`)
- **Empty file & folder detection** — finds zero-byte files and recursively empty directories
- **Broken symlink detection** — finds symbolic links pointing to missing targets
- **Smart filtering & presets** — built-in ignore profiles for JVM (Java/Kotlin JARs & classes), Node/JS, Python, Rust, C/C++, .NET, Go, etc.
- **Ignore files** — supports project-level `.dupignore` and global `~/.config/dupfinder/dupignore` configuration
- **Zero-harm safety guardrails** — OS path blacklists, sentinel protection, TOCTOU re-stat verification, and elevated privilege warnings
- **Fast** — parallel scanning and hashing with rayon
- **Hash caching** — subsequent scans skip unchanged files
- **Cross-platform** — macOS, Linux, Windows

## Installation

```bash
cargo install --path dupfinder-cli
```

## Usage

### 1. Scanning & Analysis

```bash
# Scan directories (automatically skips build artifacts, jars, and dependency folders)
dupfinder scan ~/Workspace ~/Downloads

# Scan for visually similar images (90% similarity threshold)
dupfinder scan ~/Pictures --similar-images --similarity 0.90

# Scan but include JVM JAR files in the search
dupfinder scan --include-jars ~/Workspace

# Use a specific ignore preset (default, build, deps, jars, minimal, none)
dupfinder scan --exclude-preset minimal ~/Documents

# Output as JSON report
dupfinder scan -f json -o report.json ~/Documents

# Scan only for duplicates, skip hidden files
dupfinder scan --no-empty-files --no-empty-dirs --no-broken-links ~/Documents
```

### 2. Interactive Terminal UI Inspector

```bash
# Launch interactive dual-pane TUI inspector
dupfinder tui ~/Documents ~/Downloads

# Keybindings:
#   [Tab]       : Switch between duplicate groups and file items
#   [↑] / [↓]   : Navigate list items
#   [D]         : Toggle mark for Safe Deletion (OS Trash)
#   [L]         : Toggle mark for Hardlink Deduplication
#   [O]         : Open selected file in external system default viewer
#   [Enter]     : Execute marked actions with confirmation dialog
#   [?]         : Show keyboard shortcut help
#   [q] / [Esc] : Quit inspector
```

### 3. Safe Cleanup & Remediation

```bash
# Preview planned deletions without modifying disk (dry-run)
dupfinder clean ~/Downloads --dry-run

# Interactively review each duplicate set before moving to OS Trash
dupfinder clean -i ~/Downloads

# Replace duplicate files with POSIX/NTFS hardlinks pointing to canonical original
dupfinder clean ~/Documents --hardlink

# Replace duplicate files with Copy-on-Write clones (macOS clonefile / Linux FICLONE)
dupfinder clean ~/Documents --reflink

# Clean empty files and empty directories along with duplicates
dupfinder clean ~/Downloads --clean-empty-files --clean-empty-dirs

# Export an audit remediation manifest to JSON
dupfinder clean ~/Documents --manifest audit_manifest.json
```

### 4. Ignore Configuration & Cache Management

```bash
# Display built-in default ignore lists
dupfinder ignore show-defaults

# Initialize a .dupignore file in the current directory or globally
dupfinder ignore init
dupfinder ignore init --global

# View cache info
dupfinder cache info

# Clear the hash cache
dupfinder cache clear
```

## License

MIT
