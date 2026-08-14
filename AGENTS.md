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

1. **Strict Cross-Platform Compatibility (macOS, Linux, Windows)**:
   - **MANDATORY**: All code, CLI commands, filesystem operations, and error recovery must work identically across macOS, Linux, and Windows.
   - Always use `std::path::Path` and `std::path::PathBuf`. Never hardcode path separators (`/` or `\`) or assume Unix root directory layouts.
   - Never write test fixtures that rely on platform-specific paths or shell scripts.
   - Use cross-platform abstractions (e.g. `dirs` for cache/config locations, `trash` for OS Recycle Bin / Trash integration).
   - For platform-specific features (e.g. macOS APFS `clonefile`, Linux Btrfs/XFS `FICLONE`, Windows NTFS junctions / hardlinks), always provide conditional compilation (`#[cfg(...)]`) and graceful fallback or clear messaging on unsupported platforms.
   - Handle permission and symlink quirks gracefully (e.g. Windows Developer Mode vs elevated privilege requirements for symlinks).
   - Ensure terminal input/output and TUI rendering (`crossterm` / `ratatui`) function seamlessly across Unix PTYs and Windows ConPTY.
2. **Hashing & Performance**:
   - Respect the 3-stage dedup pipeline: (1) Size grouping -> (2) Partial BLAKE3 prefix hash -> (3) Full BLAKE3 hash.
   - Use streaming reads into BLAKE3 to prevent high memory consumption on large files.
   - Ensure parallel operations utilize Rayon effectively without blocking the main thread.
3. **Error Handling**:
   - Use `thiserror` for typed, library-level error variants in `dupfinder_core::errors::DupError`.
   - Use `anyhow` for top-level application error context and exit handling in `dupfinder-cli`.
4. **Safety, Non-Destructive Operation & System Path Protection**:
   - **STRICT INVARIANT**: The utility must NEVER delete, mutate, or hardlink system files, OS internals, installed applications, or user credential/config directories.
   - **Path Blacklist Engine**: Strictly forbid operations targeting root directories (`/`, `C:\`), system hierarchies (`/System`, `/Library`, `/Applications`, `/usr`, `/etc`, `/bin`, `/sbin`, `/var`, `/proc`, `/sys`, `C:\Windows`, `C:\Program Files`, `C:\Program Files (x86)`, `C:\ProgramData`), toolchains (`.cargo`, `.rustup`, `node_modules`), and security/config vaults (`.ssh`, `.gnupg`, `.aws`, `.env`, `.git`).
   - **Hidden File Protection**: Hidden files/folders (`.*`) are excluded from deletion by default.
   - **Structural Sentinel Protection**: Empty file cleanup must protect critical structural files (e.g. `__init__.py`, `.gitkeep`, `.keep`, `.placeholder`).
   - **Canonicalization Check**: Always canonicalize paths (`fs::canonicalize`) before evaluating safety boundaries to prevent path traversal bypasses.
   - **Original Preservation Invariant**: A duplicate group must NEVER delete all copies; the designated original file MUST exist, be verified, and be preserved.
   - **Trash-First Policy**: All deletions must default to the OS Recycle Bin / Trash via the `trash` crate. Permanent unlinking requires explicit `--permanent` + interactive confirmation. If the volume lacks trash support (e.g. NAS/USB), never silently fall back to permanent deletion without explicit user approval.
   - **Mandatory Dry-Run Option**: Every remediation command must support `--dry-run` to preview operations without touching the disk.
5. **Concurrency & Filesystem Race Safety (TOCTOU & Linking Invariants)**:
   - **Pre-Execution Inode Re-Stat**: Immediately prior to deleting or linking, re-stat file metadata (`size`, `mtime`, `device`, `inode`, `file_type`) to verify it has not been replaced by a symlink or mutated.
   - **Same-Inode Invariant**: Detect when files already share an inode (`st_dev`, `st_ino`) before attempting hardlinking to prevent inode self-destruction and duplicate accounting.
   - **Atomic Replacement Pattern**: Always create temporary links before renaming over duplicate targets during hardlinking/reflinking.
   - **No Shell Interpolation**: Never pass file paths through a shell wrapper (`sh -c`, `cmd.exe`) when launching external tools.
6. **Symlink Handling**:
   - Symlink traversal and broken link detection must handle cross-platform differences (Unix vs Windows permissions) and avoid circular references. Never dereference symlinks during deletion.
7. **No Elevated Privileges (Root / Administrator Protection)**:
   - **MANDATORY**: The utility and AI Agent tasks must operate exclusively with standard, unprivileged user accounts.
   - **Never run agent commands with `sudo` or elevated privileges**.
   - The CLI must inspect runtime privilege levels: if running as `root` (Unix `UID 0`) or Windows Administrator during any remediation or cleanup action, it must display a prominent security warning regarding unintended system-wide side effects and require explicit confirmation / flag (`--allow-root` / `--allow-admin`).

---

## 3. Mandatory Pre-Commit Verification Checklist

**STRICT PRE-COMMIT RULE**:
- **NEVER** run `git commit` or finalize changes unless all tests are passing and the full verification sequence completes with exit code 0 (0 failed tests, 0 clippy warnings, clean formatting).
- Run and confirm the following verification sequence in order before every commit:

```bash
# 1. Compile check across all workspace targets
cargo check --workspace --all-targets

# 2. Run all unit and integration tests (MUST PASS 100%)
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
