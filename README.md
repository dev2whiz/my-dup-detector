# dupfinder

A fast, cross-platform CLI tool for detecting duplicate files, empty files/folders, and broken symlinks.

Inspired by [czkawka](https://github.com/qarmin/czkawka) and [rmlint](https://github.com/sahib/rmlint).

## Features

- **Duplicate file detection** — 3-stage pipeline (size → partial hash → full hash) using BLAKE3
- **Empty file detection** — finds zero-byte files
- **Empty folder detection** — finds recursively empty directories
- **Broken symlink detection** — finds symbolic links pointing to missing targets
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
# Scan directories for all issues
dupfinder scan ~/Documents ~/Downloads

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
