# Agent Guidelines for `my-dup-detector`

This document provides operational context, invariants, and validation standards for AI coding agents working on this codebase.

---

## 1. Architecture & Crate Responsibilities

The project is structured as a Cargo workspace with two primary crates:

- **`dupfinder-core`** (`dupfinder-core/`):
  - Headless library containing all duplicate detection algorithms, BLAKE3 hashing, parallel filesystem traversal (Rayon / walkdir), ignore/filtering rules (globset), hash caching, and reporting data structures.
  - **Invariants**:
    - Must remain completely CLI-agnostic.
    - Must **never** print directly to `stdout` or `stderr` (use the `ProgressReporter` trait or return structured results/errors).
    - Must be directly testable via unit and integration tests.
- **`dupfinder-cli`** (`dupfinder-cli/`):
  - CLI binary (`dupfinder`) handling argument parsing (`clap`), user progress bars and spinners (`indicatif`), and output rendering (text / JSON).
  - **Invariants**:
    - Keep core algorithms and filesystem logic inside `dupfinder-core`.
    - Only handle argument mapping, progress bar bridging, and command dispatching.

---

## 2. Core Implementation Rules & Invariants

1. **Cross-Platform Path Safety**:
   - Always use `std::path::Path` and `std::path::PathBuf`.
   - Never concatenate paths with string literals (`/` or `\`).
   - Use the `dirs` crate for platform-specific cache and config directories (`~/Library/Caches` on macOS, `~/.cache` on Linux, `%LOCALAPPDATA%` on Windows).
2. **Hashing & Performance**:
   - Respect the 3-stage dedup pipeline: (1) Size grouping -> (2) Partial BLAKE3 prefix hash -> (3) Full BLAKE3 hash.
   - Use streaming reads into BLAKE3 to prevent high memory consumption on large files.
   - Ensure parallel operations utilize Rayon effectively without blocking the main thread.
3. **Error Handling**:
   - Use `thiserror` for typed, library-level error variants in `dupfinder_core::errors::DupError`.
   - Use `anyhow` for top-level application error context and exit handling in `dupfinder-cli`.
4. **Safety & Non-Destructive Operation**:
   - The tool is designed for duplicate detection and analysis. Any file deletion or cleanup features must have explicit dry-run protection, safety checks, and confirmation steps.
5. **Symlink Handling**:
   - Symlink traversal and broken link detection must handle cross-platform differences (Unix vs Windows permissions) and avoid circular references.

---

## 3. Mandatory Verification Checklist

Before completing any task or code modification, run the following verification sequence in order:

```bash
# 1. Compile check across all workspace targets
cargo check --workspace --all-targets

# 2. Run all unit and integration tests
cargo test --workspace

# 3. Lint check (no warnings allowed)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 4. Code formatting check
cargo fmt --all -- --check
```

---

## 4. Key References & Documentation

- **Development Guide**: `DEVELOP.md` (human developer setup, manual CLI usage, and test commands)
- **Release Procedures**: `RELEASING.md` (version bump policy, changelog formatting, and release workflow)
- **Version History**: `CHANGELOG.md`
- **Architectural Specifications & Roadmaps**:
  - `docs/plans/01-mvp-init-plan.md`: Core MVP architecture and pipeline design.
  - `docs/plans/02-ignore-configuration-and-build-artifacts-plan.md`: Ignore presets and filter mechanisms.
  - `docs/plans/03-product-roadmap-and-feature-proposals.md`: Product roadmap and planned features.
