# Release & Version Management Guide (`RELEASING.md`)

This document outlines the versioning policy, pre-release verification checklist, release procedures, and multi-platform packaging steps for **`dupfinder`**.

---

## 1. Versioning Policy

`dupfinder` strictly follows [Semantic Versioning (SemVer 2.0.0)](https://semver.org/):

$$\text{v}\textbf{MAJOR}.\textbf{MINOR}.\textbf{PATCH}$$

* **`MAJOR` (e.g., `1.0.0`)**: Incompatible API breaks in `dupfinder-core`, major CLI syntax changes, or backwards-incompatible report format shifts.
* **`MINOR` (e.g., `0.2.0`)**: Backward-compatible new features (e.g., `dupfinder clean`, perceptual image hashing, interactive TUI).
* **`PATCH` (e.g., `0.1.1`)**: Backward-compatible bug fixes, performance optimizations, or documentation updates.

### Workspace Versioning Model
`dupfinder` uses **Cargo Workspace Inheritance**. All crates (`dupfinder-core` and `dupfinder-cli`) share a single version specified in the root `Cargo.toml`:

```toml
# Root Cargo.toml
[workspace.package]
version = "0.1.0"
```

Subcrates inherit this version automatically:
```toml
# dupfinder-core/Cargo.toml and dupfinder-cli/Cargo.toml
[package]
version.workspace = true
```

---

## 2. Pre-Release Verification Checklist

Before creating a release or tagging a commit, run the full verification pipeline:

```bash
# 1. Ensure working tree is clean
git status

# 2. Check code formatting
cargo fmt --all -- --check

# 3. Run Clippy lints with warnings treated as errors
cargo clippy --all-targets --all-features -- -D warnings

# 4. Run all unit and integration tests across the workspace
cargo test --all

# 5. Build release binaries in optimized mode
cargo build --release

# 6. Perform a smoke test using the release binary
./target/release/dupfinder --version
./target/release/dupfinder scan --help
./target/release/dupfinder ignore show-defaults
```

---

## 3. Release Step-by-Step Procedure

### Step 1: Bump the Workspace Version
Update the `version` field under `[workspace.package]` in the root `Cargo.toml`:

```toml
[workspace.package]
version = "0.2.0" # <--- New Version
```

Also update the inter-crate dependency version in `dupfinder-cli/Cargo.toml`:
```toml
[dependencies]
dupfinder-core = { version = "0.2.0", path = "../dupfinder-core" }
```

Update `Cargo.lock` by running:
```bash
cargo check
```

### Step 2: Update `CHANGELOG.md`
Move items from the `[Unreleased]` section into a new version header with today's date:

```markdown
## [0.2.0] - 2026-08-15

### Added
- Feature description...

### Fixed
- Bug fix description...
```

### Step 3: Commit and Tag
Commit the version bump and create an annotated Git tag:

```bash
git add Cargo.toml Cargo.lock dupfinder-cli/Cargo.toml CHANGELOG.md
git commit -m "chore: release v0.2.0"
git tag -a v0.2.0 -m "Release v0.2.0"

# Push commit and tags to remote
git push origin main
git push origin v0.2.0
```

---

## 4. Publishing to Crates.io

> [!IMPORTANT]
> Because `dupfinder-cli` depends on `dupfinder-core`, **`dupfinder-core` must always be published first**.

```bash
# Verify authentication
cargo login <YOUR_CRATES_IO_TOKEN>

# 1. Dry run package checks
cargo publish -p dupfinder-core --dry-run
cargo publish -p dupfinder-cli --dry-run

# 2. Publish core library
cargo publish -p dupfinder-core

# Wait ~30 seconds for crates.io index propagation, then publish the CLI:
cargo publish -p dupfinder-cli
```

---

## 5. Building & Packaging Binary Distribution Artifacts

For GitHub Releases, standalone binary archives are created for all major platforms:

| Target Triple | OS / Architecture | Output Archive |
|---|---|---|
| `x86_64-apple-darwin` | macOS (Intel) | `dupfinder-v0.2.0-x86_64-apple-darwin.tar.gz` |
| `aarch64-apple-darwin` | macOS (Apple Silicon M1/M2/M3) | `dupfinder-v0.2.0-aarch64-apple-darwin.tar.gz` |
| `x86_64-unknown-linux-gnu` | Linux (x86_64) | `dupfinder-v0.2.0-x86_64-unknown-linux-gnu.tar.gz` |
| `aarch64-unknown-linux-gnu` | Linux (ARM64) | `dupfinder-v0.2.0-aarch64-unknown-linux-gnu.tar.gz` |
| `x86_64-pc-windows-msvc` | Windows (x86_64) | `dupfinder-v0.2.0-x86_64-pc-windows-msvc.zip` |

### Manual Packaging Script (macOS / Unix example)

```bash
VERSION="0.2.0"
TARGET="aarch64-apple-darwin"

# Build optimized release binary
cargo build --release --target $TARGET

# Prepare staging directory
STAGING="dupfinder-v${VERSION}-${TARGET}"
mkdir -p "$STAGING"
cp target/${TARGET}/release/dupfinder "$STAGING/"
cp README.md LICENSE "$STAGING/"

# Create compressed archive & checksum
tar -czvf "${STAGING}.tar.gz" "$STAGING"
shasum -a 256 "${STAGING}.tar.gz" > "${STAGING}.tar.gz.sha256"
```

---

## 6. GitHub Actions Automated Release Pipeline (Recommended)

To automate multi-platform builds upon pushing a `v*` tag, create `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build-and-release:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact_name: dupfinder
            archive_name: dupfinder-${{ github.ref_name }}-aarch64-apple-darwin.tar.gz
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact_name: dupfinder
            archive_name: dupfinder-${{ github.ref_name }}-x86_64-unknown-linux-gnu.tar.gz
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            artifact_name: dupfinder.exe
            archive_name: dupfinder-${{ github.ref_name }}-x86_64-pc-windows-msvc.zip
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Build release
        run: cargo build --release --target ${{ matrix.target }}
      - name: Create Release Archive
        shell: bash
        run: |
          mkdir staging
          cp target/${{ matrix.target }}/release/${{ matrix.artifact_name }} staging/
          cp README.md LICENSE staging/
          if [ "${{ runner.os }}" = "Windows" ]; then
            7z a ${{ matrix.archive_name }} ./staging/*
          else
            tar -czvf ${{ matrix.archive_name }} -C staging .
          fi
      - name: Upload Release Asset
        uses: softprops/action-gh-release@v2
        with:
          files: ${{ matrix.archive_name }}
```

---

## 7. Rollback & Emergency Patching

If a critical bug is discovered after publishing:
1. **Never mutate or re-tag an existing release tag**.
2. Create an emergency patch release (e.g. `v0.2.1`).
3. If an unusable crate version was published to crates.io, use `cargo yank`:
   ```bash
   cargo yank --version 0.2.0 dupfinder-cli
   cargo yank --version 0.2.0 dupfinder-core
   ```
