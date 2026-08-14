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
├── DEVELOP.md                  # Development & testing guide
├── README.md                   # User documentation
├── LICENSE                     # MIT License
├── docs/
│   └── plans/
│       └── 01-implementation-plan.md  # Architectural specification
├── dupfinder-core/             # Core detection & processing library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Public scan API entry point
│       ├── types.rs            # Configs, reports, and data types
│       ├── scanner.rs          # Directory traversal & discovery
│       ├── hasher.rs           # BLAKE3 partial and full hashing
│       ├── dedup.rs            # 3-stage duplicate detection pipeline
│       ├── empty.rs            # Empty file & folder detection
│       ├── symlinks.rs         # Broken symlink detection
│       ├── cache.rs            # JSON hash cache management
│       ├── filter.rs           # File/dir filtering & glob rules
│       ├── progress.rs         # Progress reporting trait
│       ├── report.rs           # JSON & text report formatting
│       └── errors.rs           # Error definitions
└── dupfinder-cli/              # CLI executable
    ├── Cargo.toml
    └── src/
        ├── main.rs             # CLI entry point & clap dispatch
        ├── output.rs           # Progress display (indicatif)
        └── commands/
            ├── mod.rs
            ├── scan.rs         # `dupfinder scan` implementation
            └── cache.rs        # `dupfinder cache` implementation
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

### Run All Unit Tests
```bash
cargo test
```

### Run Tests for Core Library Only
```bash
cargo test -p dupfinder-core
```

### Run Specific Test Suites / Modules
```bash
# Run dedup pipeline tests
cargo test -p dupfinder-core dedup

# Run hashing tests
cargo test -p dupfinder-core hasher

# Run cache tests
cargo test -p dupfinder-core cache

# Run filtering tests
cargo test -p dupfinder-core filter

# Run empty file/folder tests
cargo test -p dupfinder-core empty

# Run symlink tests
cargo test -p dupfinder-core symlinks
```

### Run Tests with Verbose Output
```bash
cargo test -- --nocapture
```

---

## 5. Running the CLI in Development

You can execute the CLI binary directly through Cargo using `cargo run`:

### Basic Scans
```bash
# Scan a directory
cargo run --bin dupfinder -- scan /path/to/dir

# Scan multiple directories
cargo run --bin dupfinder -- scan ~/Documents ~/Downloads

# Limit recursion depth
cargo run --bin dupfinder -- scan . --depth 2
```

### Detection Feature Flags
```bash
# Disable specific detectors
cargo run --bin dupfinder -- scan ~/Documents --no-empty-files --no-broken-links

# Only check for duplicates
cargo run --bin dupfinder -- scan ~/Documents --no-empty-files --no-empty-dirs --no-broken-links
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

Before submitting changes, ensure the code complies with standard Rust styling and lints:

```bash
# Check formatting
cargo fmt --all -- --check

# Format code
cargo fmt --all

# Run clippy linter
cargo clippy --all-targets --all-features -- -D warnings
```

---

## 7. Adding New Features

1. **Detection Logic**: Implement new detection algorithms in `dupfinder-core/src/` with accompanying unit tests.
2. **Configuration & Data Types**: Update `dupfinder-core/src/types.rs` if introducing new scan options or report fields.
3. **CLI Arguments**: Expose new flags/options in `dupfinder-cli/src/commands/scan.rs` or create new subcommands in `dupfinder-cli/src/commands/`.
4. **Reporting**: Update text and JSON rendering in `dupfinder-core/src/report.rs`.

---

## 8. Cross-Platform Guidelines

- Use `Path` / `PathBuf` instead of string manipulations for all filesystem paths.
- Avoid hardcoded path separators (`/` or `\`).
- Use the `dirs` crate for OS-specific cache and config directories (`~/Library/Caches` on macOS, `~/.cache` on Linux, `%LOCALAPPDATA%` on Windows).
- Symlink operations should be platform-gated or gracefully handled on platforms where privileges are restricted (e.g. Windows non-admin).

---

## 9. Release & Version Management

For version bumping policies, release checklists, crates.io publishing procedures, and multi-platform packaging:
- See the dedicated [Release Guide (`RELEASING.md`)](RELEASING.md).
- Version history is tracked in [Changelog (`CHANGELOG.md`)](CHANGELOG.md).

