# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-08-14

### Added
- **Safe Deletion & Interactive Cleanup (`dupfinder clean`)**:
  - Interactive item-by-item review mode (`-i`).
  - OS Recycle Bin / Trash integration by default via `trash` crate.
  - Optional permanent deletion mode (`--permanent`) with interactive confirmation.
  - Mandatory `--dry-run` simulation mode across all remediation commands.
  - Remediation audit manifest export (`--manifest <PATH>`).
- **Hardlink & Copy-on-Write Deduplication**:
  - POSIX & NTFS hardlink deduplication (`dupfinder clean --hardlink`).
  - Cross-platform Copy-on-Write (CoW) clone deduplication (`dupfinder clean --reflink` with macOS APFS `clonefile` / Linux Btrfs/XFS `FICLONE`).
  - Same-inode loop prevention and atomic temporary link replacement pattern.
- **Interactive Terminal UI Inspector (`dupfinder tui`)**:
  - Dual-pane layout powered by `ratatui` and `crossterm`.
  - Left pane for group browsing; right pane for duplicate metadata, original comparisons, and content/hex previews.
  - Interactive keybindings: `[Tab]` pane switching, `[D]` mark delete, `[L]` mark hardlink, `[O]` open in system default viewer, `[Enter]` execute with confirmation dialog.
  - Panic hook safeguarding terminal raw mode against corrupt exits.
- **Perceptual Image Similarity Detection (`--similar-images`)**:
  - Perceptual image hashing (`dHash` / `blockhash`) via `image_hasher` and `image` crates.
  - Configurable visual similarity threshold (`--similarity 0.90`).
  - Visual similarity percentage and reclaimable estimations in text and JSON reports.
- **Enterprise Safety & System Guardrails**:
  - Protected path blacklist engine protecting OS directories (`/System`, `/Library`, `C:\Windows`, etc.), toolchains, and security vaults (`.ssh`, `.gnupg`, `.git`).
  - Structural sentinel file protection (`__init__.py`, `.gitkeep`, etc.).
  - TOCTOU (Time-of-Check to Time-of-Use) re-stat validation immediately prior to file modifications.
  - Canonical original file preservation invariant ensuring groups never delete all copies.
  - Non-elevated privilege inspection and security warnings when executed as `root` / Administrator.

## [0.1.0] - 2026-08-14

### Added
- **Duplicate Detection Pipeline**: 3-stage chunked hashing (file size → 4KB BLAKE3 prefix → full BLAKE3) with rayon parallelism.
- **Empty Item Detection**: Zero-byte file and recursive empty directory scanner.
- **Broken Symlink Detection**: Flags broken symbolic links pointing to missing targets.
- **Smart Ignore Presets & Multi-Language Filtering**: Built-in ignore rules for JVM, Node/JS, Python, Rust, C/C++, .NET, Go, and VCS/OS metadata.
- **Hierarchical Ignore Configuration**: Automatic discovery of `<scan_root>/.dupignore` and `~/.config/dupfinder/dupignore`.
- **`dupfinder ignore` Subcommand**: Commands to view default ignore patterns (`show-defaults`) and initialize starter ignore files (`init`).
- **Persistent Hash Cache**: JSON cache storing BLAKE3 hashes keyed by device ID, inode, file size, and mtime.
- **Reporting Engine**: Human-readable text and JSON report generation with quiet mode option.
- **Project Governance & CI/CD**: Centralized Cargo workspace inheritance, release manual (`RELEASING.md`), and developer workflows.
