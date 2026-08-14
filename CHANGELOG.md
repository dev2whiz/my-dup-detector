# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
