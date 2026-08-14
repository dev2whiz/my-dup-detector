# Developer Guide (`DEVELOP.md`)

This guide provides instructions for building, testing, and contributing to **`dupfinder`**.

---

## 1. Prerequisites

- **Rust toolchain** (Rust 2021 edition, 1.70+ recommended):
  ```bash
  # Check if installed
  cargo --version
  rustc --version
  ```
  If Rust is not installed, install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

---

## 2. Project Architecture & Workspace Layout

`dupfinder` is structured as a Cargo workspace with two main crates:

```
my-dup-detector/
├── Cargo.toml                  # Workspace manifest
├── AGENTS.md                   # AI Agent invariants & safety standards
├── DEVELOP.md                  # Development & testing guide
├── RELEASING.md                # Release procedures & SemVer policies
├── CHANGELOG.md                # Version changelog
├── README.md                   # User documentation
├── LICENSE                     # MIT License
├── dupfinder-core/             # Headless core detection & remediation library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Public scan API entry point
│       ├── types.rs            # Configs, reports, and data types
│       ├── scanner.rs          # Directory traversal & discovery
│       ├── hasher.rs           # BLAKE3 partial and full hashing
│       ├── dedup.rs            # 3-stage duplicate detection pipeline
│       ├── image_sim.rs        # Perceptual image hashing & clustering
│       ├── clean.rs            # Safe deletion, hardlinking, and reflink engine
│       ├── safety.rs           # Path blacklist, sentinel checks, TOCTOU validation
│       ├── empty.rs            # Empty file & folder detection
│       ├── symlinks.rs         # Broken symlink detection
│       ├── cache.rs            # JSON hash cache management
│       ├── filter.rs           # File/dir filtering & glob rules
│       ├── progress.rs         # Progress reporting trait
│       ├── report.rs           # JSON & text report formatting
│       └── errors.rs           # Error definitions
└── dupfinder-cli/              # CLI executable & user interface
    ├── Cargo.toml
    └── src/
        ├── main.rs             # CLI entry point & clap dispatch
        ├── output.rs           # Progress display (indicatif)
        └── commands/
            ├── mod.rs
            ├── scan.rs         # `dupfinder scan` implementation
            ├── clean.rs        # `dupfinder clean` remediation command
            ├── tui.rs          # `dupfinder tui` interactive terminal inspector
            ├── ignore.rs       # `dupfinder ignore` preset & config manager
            └── cache.rs        # `dupfinder cache` management
```

---

## 3. Building the Project

### Debug Build
```bash
cargo build
```

### Release Build (Optimized)
```bash
cargo build --release
```
The compiled binary will be located at `target/release/dupfinder`.

### Build Specific Crate
```bash
# Build only the core library
cargo build -p dupfinder-core

# Build only the CLI binary
cargo build -p dupfinder-cli
```

---

## 4. Running Tests

### Run All Unit & Integration Tests (Workspace)
```bash
cargo test --workspace
```

### Run Tests for Core Library Only
```bash
cargo test -p dupfinder-core
```

### Run Tests for CLI & TUI Only
```bash
cargo test -p dupfinder-cli
```

### Run Specific Test Modules
```bash
# Run dedup pipeline tests
cargo test -p dupfinder-core dedup

# Run perceptual image similarity tests
cargo test -p dupfinder-core image_sim

# Run safe cleanup & hardlinking tests
cargo test -p dupfinder-core clean

# Run safety & path protection tests
cargo test -p dupfinder-core safety

# Run TUI unit tests
cargo test -p dupfinder-cli commands::tui
```

### Run Tests with Verbose Output
```bash
cargo test --workspace -- --nocapture
```

---

## 5. Running the CLI in Development

You can execute the CLI binary directly through Cargo using `cargo run`:

### Basic & Perceptual Image Scans
```bash
# Scan a directory
cargo run --bin dupfinder -- scan /path/to/dir

# Scan multiple directories
cargo run --bin dupfinder -- scan ~/Documents ~/Downloads

# Scan for visually similar images (e.g. 90% similarity threshold)
cargo run --bin dupfinder -- scan ~/Pictures --similar-images --similarity 0.90

# Limit recursion depth
cargo run --bin dupfinder -- scan . --depth 2
```

### Interactive Terminal UI Inspector
```bash
# Launch interactive TUI
cargo run --bin dupfinder -- tui ~/Documents ~/Downloads
```

### Safe Cleanup & Remediation Commands
```bash
# Dry-run preview
cargo run --bin dupfinder -- clean ~/Downloads --dry-run

# Interactive review mode
cargo run --bin dupfinder -- clean -i ~/Downloads

# Hardlink deduplication (replace duplicate files with POSIX/NTFS hardlinks)
cargo run --bin dupfinder -- clean ~/Documents --hardlink

# Copy-on-Write reflink deduplication (macOS APFS clone / Linux FICLONE)
cargo run --bin dupfinder -- clean ~/Documents --reflink

# Export audit remediation manifest
cargo run --bin dupfinder -- clean ~/Documents --manifest audit.json
```

### Filtering & Ignore Options
```bash
# Set minimum file size threshold (e.g. 10KB, 5MB)
cargo run --bin dupfinder -- scan ~/Documents --min-size 10KB

# Use specific ignore preset: default, build, deps, jars, minimal, none
cargo run --bin dupfinder -- scan ~/Workspace --exclude-preset build

# Keep default presets but scan JAR/archive files
cargo run --bin dupfinder -- scan ~/Workspace --include-jars

# Disable all default presets
cargo run --bin dupfinder -- scan ~/Documents --no-default-ignores

# Load explicit custom ignore file
cargo run --bin dupfinder -- scan ~/Workspace --ignore-file .custom_ignore

# Exclude specific glob pattern or directory
cargo run --bin dupfinder -- scan ~/Documents --exclude "*.tmp" --exclude-dir build

# Include hidden files (dotfiles)
cargo run --bin dupfinder -- scan ~/Documents --include-hidden
```

### Ignore Configuration Management
```bash
# Display built-in default ignore categories, directories, and patterns
cargo run --bin dupfinder -- ignore show-defaults

# Generate a starter .dupignore file in the current directory
cargo run --bin dupfinder -- ignore init

# Generate a starter global ignore file (~/.config/dupfinder/dupignore)
cargo run --bin dupfinder -- ignore init --global

# Show the path to the global user ignore file
cargo run --bin dupfinder -- ignore path
```

### Output Formats & Reporting
```bash
# Generate human-readable text report (default)
cargo run --bin dupfinder -- scan ~/Documents -f text

# Generate JSON report to stdout
cargo run --bin dupfinder -- scan ~/Documents -f json

# Write report to file (format auto-detected from extension)
cargo run --bin dupfinder -- scan ~/Documents -o report.json
cargo run --bin dupfinder -- scan ~/Documents -o report.txt

# Quiet mode (suppress progress bars)
cargo run --bin dupfinder -- scan ~/Documents --quiet
```

### Cache Management
```bash
# View cache statistics
cargo run --bin dupfinder -- cache info

# Clear cache entries
cargo run --bin dupfinder -- cache clear

# Scan with caching disabled
cargo run --bin dupfinder -- scan ~/Documents --no-cache
```

---

## 6. Code Formatting & Linting

Before submitting changes, ensure the code complies with the mandatory 4-gate verification sequence:

```bash
# 1. Compile check
cargo check --workspace --all-targets

# 2. Run all unit and integration tests (MUST PASS 100%)
cargo test --workspace

# 3. Lint check (no warnings allowed)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 4. Code formatting check
cargo fmt --all -- --check
```

---

## 7. Cross-Platform Guidelines

- Use `Path` / `PathBuf` instead of string manipulations for all filesystem paths.
- Avoid hardcoded path separators (`/` or `\`).
- Use the `dirs` crate for OS-specific cache and config directories (`~/Library/Caches` on macOS, `~/.cache` on Linux, `%LOCALAPPDATA%` on Windows).
- Use `trash` crate for Recycle Bin / Trash integration.
- For platform-specific features (macOS APFS `clonefile`, Linux `FICLONE`), ensure fallback and conditional compilation.

---

## 8. Release & Version Management

For version bumping policies, release checklists, crates.io publishing procedures, and multi-platform packaging:
- See the dedicated [Release Guide (`RELEASING.md`)](RELEASING.md).
- Version history is tracked in [Changelog (`CHANGELOG.md`)](CHANGELOG.md).
