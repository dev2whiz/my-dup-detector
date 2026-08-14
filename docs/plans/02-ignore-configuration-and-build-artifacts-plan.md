# Ignore Configuration & Multi-Language Build Artifact Filtering Specification

## 1. Overview & Motivation

`dupfinder` is designed to be a high-performance, cross-platform duplicate file detection tool. In real-world software development environments, scanning projects without filtering results in severe performance penalties and noise from:
- Downloaded third-party modules (`node_modules`, `vendor/`, `.venv/`, `.gradle/caches/`, `.m2/repository/`)
- Compiled code and build outputs (`target/`, `build/`, `dist/`, `bin/`, `obj/`, `out/`)
- JVM archives and compiled classes (`*.jar`, `*.war`, `*.ear`, `*.class`, `*.aar`)
- Native object files and shared libraries (`*.o`, `*.obj`, `*.so`, `*.dylib`, `*.dll`)
- Intermediate IDE and OS metadata (`.idea/`, `.vscode/`, `.DS_Store`, `Thumbs.db`)

This document outlines the architecture, evaluation, and design for:
1. Comprehensive built-in filtering for major programming languages and ecosystems.
2. A flexible, layered default ignore configuration file system (`.dupignore` and global config).

---

## 2. Evaluation: Feasibility & Suitability

### 2.1 Suitability
- **Noise Reduction**: Prevents reporting thousands of identical vendor packages, downloaded dependencies, or common library JARs as duplicates.
- **Drastic Speedup**: Eliminates 80-95% of filesystem traversal and BLAKE3 hashing operations across repositories.
- **Ergonomics**: Developers do not need to supply dozens of CLI `--exclude` arguments on every run.

### 2.2 Feasibility
- **Zero Heavyweight Overhead**: Utilizes standard glob matching (`globset`) and OS directory discovery (`dirs`), already present in the workspace dependencies.
- **Safe by Default, Configurable on Demand**: Provides sane multi-language presets with clear override flags (`--no-default-ignores`, `--include-jars`, `--exclude-preset`).

---

## 3. Architecture & Ecosystem Presets

### 3.1 Supported Ecosystems

| Language / Framework | Ignored Directories | Ignored Patterns / Files |
|---|---|---|
| **JVM (Java, Kotlin, Scala, Gradle, Maven)** | `target`, `build`, `.gradle`, `.m2`, `out`, `bin`, `.bsp`, `.metals`, `.sbt` | `*.jar`, `*.war`, `*.ear`, `*.class`, `*.aar` |
| **JavaScript / TypeScript / Node / Web** | `node_modules`, `.next`, `.nuxt`, `.turbo`, `.parcel-cache`, `dist`, `build`, `out`, `.svelte-kit`, `.cache`, `.yarn/cache`, `bower_components` | `*.tsbuildinfo` |
| **Python** | `__pycache__`, `.venv`, `venv`, `env`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.tox`, `.nox`, `build`, `dist` | `*.pyc`, `*.pyo`, `*.pyd`, `*.egg-info` |
| **Rust** | `target` | `*.rlib`, `*.rmeta` |
| **C / C++ / CMake** | `build`, `cmake-build-debug`, `cmake-build-release`, `CMakeFiles`, `obj`, `bin` | `*.o`, `*.obj`, `*.a`, `*.lib`, `*.so`, `*.dylib`, `*.dll`, `*.exe`, `*.pdb` |
| **Go** | `vendor`, `bin`, `pkg` | — |
| **.NET / C# / F#** | `bin`, `obj`, `packages`, `.vs` | `*.nupkg`, `*.pdb` |
| **PHP & Ruby** | `vendor`, `vendor/bundle`, `.bundle` | — |
| **Dart & Flutter & Swift** | `.dart_tool`, `build`, `.pub-cache`, `.build`, `DerivedData`, `Pods` | — |
| **VCS & OS Metadata** | `.git`, `.svn`, `.hg`, `.DS_Store`, `Thumbs.db`, `.idea`, `.vscode` | `.DS_Store`, `Thumbs.db`, `desktop.ini`, `*.tmp`, `*.swp`, `*~` |

---

## 4. Layered Configuration Resolution

When executing a scan, ignore rules are merged following a 5-layer hierarchy:
1. **Built-in Presets**: Multi-language defaults active unless `--no-default-ignores` or `--exclude-preset minimal` is specified.
2. **Global User Ignore File**: Located at `~/.config/dupfinder/dupignore` (Linux/macOS) or `%APPDATA%\dupfinder\dupignore` (Windows).
3. **Project-Level Ignore File**: `<scan_root>/.dupignore` automatically parsed when scanning a directory.
4. **Custom Ignore Files**: Specified via repeated `--ignore-file <path>` arguments.
5. **CLI Overrides**: Explicit `--exclude <pattern>`, `--exclude-dir <dir>`, `--include-jars`, and `--no-default-ignores`.

---

## 5. CLI Interface

### New Flags on `dupfinder scan`
- `--exclude-preset <preset>`: Select preset (`default`, `build`, `deps`, `jars`, `minimal`, `none`).
- `--no-default-ignores`: Disable all built-in presets.
- `--ignore-file <path>`: Load an explicit custom ignore file.
- `--include-jars`: Scan JAR files even when using the default preset.
- `--no-global-ignore`: Skip global config ignore file.
- `--no-project-ignore`: Skip `<scan_root>/.dupignore`.

### New Subcommand: `dupfinder ignore`
- `dupfinder ignore show-defaults` / `list`: Display all active presets and default patterns.
- `dupfinder ignore init [--global | --local]`: Create a starter `.dupignore` file.
- `dupfinder ignore path`: Display the global configuration directory and ignore file path.
