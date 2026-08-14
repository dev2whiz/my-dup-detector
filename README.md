# dupfinder

A fast, cross-platform CLI tool for detecting duplicate files, empty files/folders, and broken symlinks.

Inspired by [czkawka](https://github.com/qarmin/czkawka) and [rmlint](https://github.com/sahib/rmlint).

## Features

- **Duplicate file detection** — 3-stage pipeline (size → partial hash → full hash) using BLAKE3
- **Empty file detection** — finds zero-byte files
- **Empty folder detection** — finds recursively empty directories
- **Broken symlink detection** — finds symbolic links pointing to missing targets
- **Smart filtering & presets** — built-in ignore profiles for JVM (Java/Kotlin JARs & classes), Node/JS, Python, Rust, C/C++, .NET, Go, etc.
- **Ignore files** — supports project-level `.dupignore` and global `~/.config/dupfinder/dupignore` configuration
- **Report-only** — generates reports for review; no destructive operations
- **Fast** — parallel scanning and hashing with rayon
- **Hash caching** — subsequent scans skip unchanged files
- **Cross-platform** — macOS, Linux, Windows

## Installation

```bash
cargo install --path dupfinder-cli
```

## Usage

```bash
# Scan directories (automatically skips build artifacts, jars, and dependency folders)
dupfinder scan ~/Workspace ~/Downloads

# Scan but include JVM JAR files in the search
dupfinder scan --include-jars ~/Workspace

# Use a specific ignore preset (default, build, deps, jars, minimal, none)
dupfinder scan --exclude-preset minimal ~/Documents

# Display built-in default ignore lists
dupfinder ignore show-defaults

# Initialize a .dupignore file in the current directory or globally
dupfinder ignore init
dupfinder ignore init --global

# Output as JSON report
dupfinder scan -f json -o report.json ~/Documents

# Scan only for duplicates, skip hidden files
dupfinder scan --no-empty-files --no-empty-dirs --no-broken-links ~/Documents

# View cache info
dupfinder cache info

# Clear the hash cache
dupfinder cache clear
```

## License

MIT
