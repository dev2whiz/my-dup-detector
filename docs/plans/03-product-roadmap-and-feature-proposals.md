# Product Roadmap & Feature Proposals (`03-product-roadmap-and-feature-proposals.md`)

## 1. Executive Summary & Product Review

`dupfinder` is a high-performance CLI tool built with Rust, designed for fast duplicate detection across developer workspaces, media stores, and system directories. It leverages parallel traversal (`rayon`), a 3-stage chunked hashing pipeline (size → 4KB BLAKE3 prefix → full BLAKE3), and smart multi-language ignore presets to drastically cut down scan times and noise.

### Current Value Proposition
- **High Performance & Low Noise**: Handles large directories (10k+ files) in seconds with zero-configuration build/dependency filtering.
- **Precision**: Exact byte-level deduplication via cryptographic BLAKE3 hashes with mtime/size cache persistence.
- **Diagnostic Safety**: Non-destructive reporting format across text, JSON, and terminal output.

### The Primary Product Gap: The Remediation Bottleneck
Currently, `dupfinder` is strictly a **diagnostic / reporting tool**. When a scan completes (such as discovering 26 duplicate groups and 150 KB+ of redundant files), users are presented with actionable data but **no built-in tools to resolve it**. Users must manually navigate paths, evaluate timestamps, and run `rm` or `ln` commands manually.

---

## 2. Feature Priority & Value Matrix

```
                      High Impact
                           ▲
                           │  [Theme 1.1] Interactive & Safe Remediation
                           │  [Theme 1.2] Hardlink / Reflink (CoW) Dedup
                           │  [Theme 2.1] Terminal TUI Inspector
                           │  [Theme 3.1] Similar Image (Perceptual Hash)
                           │
       [Theme 4.1] HTML    │  [Theme 1.3] Shell Script Generation
       Visual Dashboard    │  [Theme 1.4] Heuristic Keep Policies
                           │  [Theme 4.2] CI/CD Gate Mode
                           │
  ─────────────────────────┼─────────────────────────► Low Effort
  High Effort              │
                           │  [Theme 5.1] Persistent SQLite Cache
                           ▼
                      Low Impact
```

---

## 3. Comprehensive Feature Proposals

### Theme 1: Safe Remediation & Storage Reclamation (Priority: Highest)

#### 1.1 Safe Deletion & Interactive Cleanup (`dupfinder clean`)
* **Problem**: Manually removing duplicate or empty files is tedious and prone to accidental deletion of active files.
* **Proposed Solution**:
  * **Interactive Mode (`dupfinder clean --interactive` / `-i`)**: Step through duplicate sets interactively:
    * `[k]` Keep original (delete duplicates)
    * `[d]` Delete specific files
    * `[s]` Skip group
    * `[v]` View side-by-side preview or diff
    * `[a]` Apply action to all remaining groups
  * **Safe Trash Deletion (`--trash`, default on interactive)**: Move files to the OS Recycle Bin / Trash using the cross-platform `trash` crate instead of permanently unlinking.
  * **Dry-Run Mode (`--dry-run`)**: Output an exact execution plan detailing which files would be moved or deleted and the exact amount of disk space to be reclaimed.
  * **Empty Item Deletion (`--clean-empty-files`, `--clean-empty-dirs`)**: Batch removal of zero-byte files and recursively empty directories.

#### 1.2 Hardlink & Copy-on-Write (Reflink) Deduplication (`--hardlink`, `--reflink`)
* **Problem**: In software projects and media archives, duplicate files may be required at distinct directory paths for relative imports or build structures. Deleting them breaks projects.
* **Proposed Solution**:
  * **Hardlinking (`dupfinder clean --hardlink`)**: Replace duplicate files with POSIX / NTFS hard links (`std::fs::hard_link`) pointing to the original file inode. This reclaims 100% of the redundant disk space while preserving file availability at all paths.
  * **Reflink / APFS Clonefile (`dupfinder clean --reflink`)**: Utilize native Copy-on-Write clones (macOS `clonefile`, Linux Btrfs/XFS `FICLONE`) for zero-space copies that remain independently mutable without affecting the original.

#### 1.3 Remediation Script Generation (`--generate-script <path>`)
* **Problem**: System administrators, DevOps engineers, and cautious developers want to inspect, audit, or schedule cleanup commands before execution.
* **Proposed Solution**:
  * Output standalone Bash (`.sh`) or PowerShell (`.ps1`) scripts containing verified `rm` or `ln -f` commands with built-in safety guards (`set -euo pipefail`).

#### 1.4 Configurable "Original File" Selection Policies (`--keep-policy`)
* **Problem**: The current heuristics strictly designate the oldest file by `mtime` as `ORIGINAL`. Users often have canonical directory structures or prefer shortest path lengths.
* **Proposed Solution**:
  * Configurable selection rules:
    * `--keep-policy oldest` (default)
    * `--keep-policy newest`
    * `--keep-policy shortest-path` / `longest-path`
    * `--keep-prefer-dir <dir>` (files under preferred path are always retained as original)
    * `--keep-prefer-pattern <glob>` (e.g. `*/master/*`, `*/canonical/*`)

---

### Theme 2: Interactive Terminal User Experience

#### 2.1 Rich Terminal UI (`dupfinder tui`)
* **Problem**: Inspecting large multi-gigabyte scans in text mode requires continuous scrolling and lacks quick comparison capabilities.
* **Proposed Solution**:
  * Interactive terminal interface built with `ratatui` and `crossterm`.
  * **Dual-Pane Interface**:
    * **Left Pane**: Tree/list of duplicate groups, file size, reclaimable capacity, and match confidence.
    * **Right Pane**: File metadata comparison, text diff preview, hex dump, or file metadata (EXIF/dimensions/permissions).
  * **Keyboard Shortcuts**: `[Space]` select/deselect, `[D]` mark for deletion, `[L]` mark for hardlinking, `[O]` open in external editor or file manager, `[Enter]` execute actions.

---

### Theme 3: Multi-Modal & Near-Duplicate Detection

#### 3.1 Perceptual & Similar Image Detection (`--similar-images`)
* **Problem**: Photographers and media collectors accumulate duplicates with slight compression variations, format differences (PNG vs JPG vs WebP), or minor crops.
* **Proposed Solution**:
  * Compute perceptual hashes (pHash / dHash / aHash) via `image_hasher`.
  * Configurable Hamming distance / similarity threshold (e.g., `--similarity 90%`).
  * Group visually identical images regardless of format or metadata differences.

#### 3.2 Near-Duplicate Code & Document Matching (`--similar-text`)
* **Problem**: Copy-pasted modules, duplicated templates, or boilerplate code with slight comment or variable modifications consume space and create maintenance drift.
* **Proposed Solution**:
  * Tokenized MinHash or SimHash algorithms to group text files with >85% similarity.

---

### Theme 4: Visual & Enterprise Reporting

#### 4.1 Standalone Interactive HTML Report (`-f html -o report.html`)
* **Problem**: Terminal output is ephemeral, and JSON requires custom tooling to inspect visually.
* **Proposed Solution**:
  * Generate a single self-contained HTML file (embedded CSS/JS, zero network calls).
  * Interactive components:
    * Donut / Bar charts of space waste broken down by directory, file type, and group size.
    * Filterable, sortable data table with instant search and one-click copy paths.
    * Quick-action export for deletion scripts.

#### 4.2 CI/CD Quality Gate Mode (`dupfinder check`)
* **Problem**: Repositories frequently bloat when developers accidentally commit redundant test fixtures, vendor bundles, or assets.
* **Proposed Solution**:
  * Dedicated check command: `dupfinder check [paths] --max-wasted-size 1MB --fail-on-duplicates`.
  * Returns distinct exit codes (0 = clean, 1 = duplicate threshold exceeded, 2 = scan error) for GitHub Actions and pre-commit hooks.

---

### Theme 5: Scalability & Persistent Storage

#### 5.1 Embedded SQLite Metadata & Hash Store
* **Problem**: For multi-terabyte drives with millions of files, loading and serializing a single flat JSON cache file can incur memory and startup overhead.
* **Proposed Solution**:
  * Optional SQLite backend (`rusqlite`) for transactional, incremental cache updates and complex ad-hoc querying.

---

## 4. Phased Implementation Roadmap

| Milestone | Target Scope | Key Deliverables |
| :--- | :--- | :--- |
| **Phase 1: Remediation & Safety Engine (v0.2.0)** | • Interactive & batch cleanup (`dupfinder clean`)<br>• Hardlink & Reflink (CoW) dedup<br>• OS Trash / Recycle bin safety<br>• Shell script generator (`--generate-script`)<br>• Configurable keep policies (`--keep-policy`) | Converts `dupfinder` into an end-to-end remediation solution. |
| **Phase 2: Terminal UI & Visual Analytics (v0.3.0)** | • Interactive TUI (`dupfinder tui` with `ratatui`)<br>• Standalone interactive HTML report (`-f html`)<br>• CI/CD `check` command | Delivers rich visual workflows and automated workflow governance. |
| **Phase 3: Multi-Modal Detection (v0.4.0)** | • Perceptual image hashing (`--similar-images`)<br>• Fuzzy document/source code matching (`--similar-text`) | Expands target audience to designers, media managers, and multi-media archives. |

---

## 5. Next Steps

1. **Review & Prioritize**: Stakeholders review the proposed roadmap and confirm prioritization for Phase 1 vs Phase 2.
2. **Draft Technical Specification for Phase 1**: Detail CLI syntax, error recovery, rollback protections, and cross-platform filesystem handling for the `clean` subcommand.
