//! Ignore rules, preset profiles, and `.dupignore` parser for `dupfinder`.

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};

/// Predefined ignore presets for common development ecosystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnorePreset {
    /// Comprehensive: All common build dirs, vendor dirs, jars/bytecode, and OS metadata.
    Default,
    /// Compilation outputs and intermediate object files.
    BuildOnly,
    /// Downloaded third-party module and dependency directories.
    Dependencies,
    /// JVM archive artifacts (`*.jar`, `*.war`, `*.ear`, `*.class`, `*.aar`).
    JarsOnly,
    /// VCS and OS metadata only (`.git`, `.DS_Store`, etc.).
    Minimal,
    /// No presets (empty).
    None,
}

impl IgnorePreset {
    /// Parse preset name from string.
    pub fn from_str_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "default" | "all" => Some(Self::Default),
            "build" | "build-only" | "builds" => Some(Self::BuildOnly),
            "deps" | "dependencies" | "modules" => Some(Self::Dependencies),
            "jars" | "jar" | "jvm" => Some(Self::JarsOnly),
            "minimal" | "vcs" => Some(Self::Minimal),
            "none" | "empty" => Some(Self::None),
            _ => None,
        }
    }
}

/// Catalog entry describing a category of ignored files/directories.
pub struct PresetCategory {
    pub name: &'static str,
    pub description: &'static str,
    pub dirs: &'static [&'static str],
    pub patterns: &'static [&'static str],
}

/// Built-in category catalog.
pub const PRESET_CATALOG: &[PresetCategory] = &[
    PresetCategory {
        name: "JVM (Java, Kotlin, Scala, Gradle, Maven)",
        description: "Build outputs, caches, dependency repos, and compiled archives",
        dirs: &[
            "target", "build", ".gradle", ".m2", "out", "bin", ".bsp", ".metals", ".sbt",
        ],
        patterns: &["*.jar", "*.war", "*.ear", "*.class", "*.aar"],
    },
    PresetCategory {
        name: "JavaScript / TypeScript / Node / Web",
        description: "Node modules, framework build outputs, and package caches",
        dirs: &[
            "node_modules",
            ".next",
            ".nuxt",
            ".turbo",
            ".parcel-cache",
            "dist",
            "build",
            "out",
            ".svelte-kit",
            ".cache",
            ".yarn/cache",
            "bower_components",
        ],
        patterns: &["*.tsbuildinfo"],
    },
    PresetCategory {
        name: "Python",
        description: "Bytecode cache, virtual environments, eggs, and build artifacts",
        dirs: &[
            "__pycache__",
            ".venv",
            "venv",
            "env",
            ".pytest_cache",
            ".mypy_cache",
            ".ruff_cache",
            ".tox",
            ".nox",
            ".eggs",
            "*.egg-info",
            "build",
            "dist",
        ],
        patterns: &["*.pyc", "*.pyo", "*.pyd"],
    },
    PresetCategory {
        name: "Rust",
        description: "Cargo build target directory and compiled artifacts",
        dirs: &["target"],
        patterns: &["*.rlib", "*.rmeta"],
    },
    PresetCategory {
        name: "C / C++ / CMake",
        description: "Build folders, object files, and compiled libraries",
        dirs: &[
            "build",
            "cmake-build-debug",
            "cmake-build-release",
            "CMakeFiles",
            "obj",
            "bin",
        ],
        patterns: &[
            "*.o", "*.obj", "*.a", "*.lib", "*.so", "*.dylib", "*.dll", "*.exe", "*.pdb",
        ],
    },
    PresetCategory {
        name: "Go",
        description: "Vendored packages and compiled binaries",
        dirs: &["vendor", "bin", "pkg"],
        patterns: &[],
    },
    PresetCategory {
        name: ".NET / C# / F#",
        description: "Bin/obj folders, NuGet packages, and debug symbols",
        dirs: &["bin", "obj", "packages", ".vs"],
        patterns: &["*.nupkg", "*.pdb"],
    },
    PresetCategory {
        name: "PHP & Ruby",
        description: "Composer and Bundler vendor directories",
        dirs: &["vendor", "vendor/bundle", ".bundle"],
        patterns: &[],
    },
    PresetCategory {
        name: "Mobile & Apple (Dart, Flutter, Swift)",
        description: "Flutter tool caches, CocoaPods, and Xcode DerivedData",
        dirs: &[
            ".dart_tool",
            "build",
            ".pub-cache",
            ".build",
            "DerivedData",
            "Pods",
        ],
        patterns: &[],
    },
    PresetCategory {
        name: "VCS & OS Metadata",
        description: "Version control directories, OS thumbnails, and editor metadata",
        dirs: &[
            ".git",
            ".svn",
            ".hg",
            ".DS_Store",
            "Thumbs.db",
            ".idea",
            ".vscode",
        ],
        patterns: &[
            ".DS_Store",
            "Thumbs.db",
            "desktop.ini",
            "*.tmp",
            "*.swp",
            "*~",
        ],
    },
];

/// Compiled collection of ignore rules (directory names, path patterns, glob sets).
#[derive(Debug, Clone)]
pub struct IgnoreRules {
    /// Directory names to skip when encountered.
    pub exclude_dirs: Vec<String>,
    /// Compiled glob set for file matching.
    pub exclude_globs: Option<GlobSet>,
    /// Raw patterns for debugging or inspection.
    pub raw_patterns: Vec<String>,
}

impl Default for IgnoreRules {
    fn default() -> Self {
        Self::from_preset(IgnorePreset::Default, false)
    }
}

impl IgnoreRules {
    /// Create an empty set of ignore rules.
    pub fn empty() -> Self {
        Self {
            exclude_dirs: Vec::new(),
            exclude_globs: None,
            raw_patterns: Vec::new(),
        }
    }

    /// Construct `IgnoreRules` from a preset profile.
    ///
    /// If `include_jars` is true, JVM JAR/archive patterns will be excluded from the ignore list
    /// (meaning JARs will be scanned).
    pub fn from_preset(preset: IgnorePreset, include_jars: bool) -> Self {
        let mut dirs = Vec::new();
        let mut patterns = Vec::new();

        match preset {
            IgnorePreset::None => {}
            IgnorePreset::Minimal => {
                dirs.extend_from_slice(&[
                    ".git",
                    ".svn",
                    ".hg",
                    ".DS_Store",
                    "Thumbs.db",
                    ".idea",
                    ".vscode",
                ]);
                patterns.extend_from_slice(&[
                    ".DS_Store",
                    "Thumbs.db",
                    "desktop.ini",
                    "*.tmp",
                    "*.swp",
                    "*~",
                ]);
            }
            IgnorePreset::BuildOnly => {
                dirs.extend_from_slice(&[
                    "target",
                    "build",
                    "dist",
                    "out",
                    "bin",
                    "obj",
                    "CMakeFiles",
                    "cmake-build-debug",
                    "cmake-build-release",
                    ".next",
                    ".nuxt",
                    ".turbo",
                    ".parcel-cache",
                    ".svelte-kit",
                ]);
                patterns.extend_from_slice(&[
                    "*.o", "*.obj", "*.a", "*.lib", "*.so", "*.dylib", "*.dll", "*.exe", "*.pdb",
                    "*.rlib", "*.rmeta", "*.pyc", "*.pyo",
                ]);
                if !include_jars {
                    patterns.extend_from_slice(&["*.class", "*.jar", "*.war", "*.ear", "*.aar"]);
                }
            }
            IgnorePreset::Dependencies => {
                dirs.extend_from_slice(&[
                    "node_modules",
                    ".venv",
                    "venv",
                    "env",
                    "vendor",
                    ".gradle",
                    ".m2",
                    "packages",
                    "Pods",
                    ".pub-cache",
                    ".yarn/cache",
                    "bower_components",
                ]);
                if !include_jars {
                    patterns.extend_from_slice(&["*.jar", "*.aar", "*.nupkg"]);
                }
            }
            IgnorePreset::JarsOnly => {
                dirs.extend_from_slice(&[".gradle", ".m2"]);
                patterns.extend_from_slice(&["*.jar", "*.war", "*.ear", "*.class", "*.aar"]);
            }
            IgnorePreset::Default => {
                for category in PRESET_CATALOG {
                    dirs.extend_from_slice(category.dirs);
                    for pattern in category.patterns {
                        if include_jars
                            && (*pattern == "*.jar"
                                || *pattern == "*.war"
                                || *pattern == "*.ear"
                                || *pattern == "*.class"
                                || *pattern == "*.aar")
                        {
                            continue;
                        }
                        patterns.push(*pattern);
                    }
                }
            }
        }

        let mut rules = Self::empty();
        for dir in dirs {
            rules.add_dir(dir);
        }
        for pattern in patterns {
            rules.add_pattern(pattern);
        }
        rules.compile_globs();
        rules
    }

    /// Add a directory to the exclusion list (deduplicated).
    pub fn add_dir(&mut self, dir: &str) {
        let trimmed = dir.trim();
        if !trimmed.is_empty() && !self.exclude_dirs.iter().any(|d| d == trimmed) {
            self.exclude_dirs.push(trimmed.to_string());
        }
    }

    /// Add a glob pattern to the exclusion list (deduplicated).
    pub fn add_pattern(&mut self, pattern: &str) {
        let trimmed = pattern.trim();
        if !trimmed.is_empty() && !self.raw_patterns.iter().any(|p| p == trimmed) {
            self.raw_patterns.push(trimmed.to_string());
        }
    }

    /// Parse lines in `.dupignore` format (supports comments `#`, blank lines, trailing `/` for dirs).
    pub fn parse_ignore_content(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Trailing slash indicates directory only
            if let Some(dir) = line.strip_suffix('/') {
                self.add_dir(dir);
            } else if line.contains('/') || line.contains('*') || line.contains('?') {
                self.add_pattern(line);
            } else {
                // If it has no file extension and no wildcards, treat as both or directory
                if !line.contains('.') {
                    self.add_dir(line);
                }
                self.add_pattern(line);
            }
        }
        self.compile_globs();
    }

    /// Load ignore rules from a file path.
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let content = fs::read_to_string(path)?;
        self.parse_ignore_content(&content);
        Ok(())
    }

    /// Compile the raw glob patterns into a `GlobSet`.
    pub fn compile_globs(&mut self) {
        if self.raw_patterns.is_empty() {
            self.exclude_globs = None;
            return;
        }

        let mut builder = GlobSetBuilder::new();
        for pattern in &self.raw_patterns {
            if let Ok(glob) = Glob::new(pattern) {
                builder.add(glob);
            }
        }

        self.exclude_globs = builder.build().ok();
    }

    /// Check if a directory name or path should be skipped.
    pub fn is_dir_excluded(&self, dir_name: &str) -> bool {
        self.exclude_dirs.iter().any(|d| d == dir_name)
    }

    /// Check if a file path is excluded by glob patterns.
    pub fn is_path_excluded(&self, path: &Path) -> bool {
        if let Some(ref globs) = self.exclude_globs {
            if globs.is_match(path) {
                return true;
            }
            // Also test just the file name component
            if let Some(file_name) = path.file_name() {
                if globs.is_match(file_name) {
                    return true;
                }
            }
        }
        false
    }
}

/// Get the standard path for the user's global `.dupignore` or `dupignore` config file.
pub fn get_global_ignore_path() -> Option<PathBuf> {
    dirs::config_dir().map(|mut p| {
        p.push("dupfinder");
        p.push("dupignore");
        p
    })
}

/// Generate default starter content for a `.dupignore` file.
pub fn generate_starter_ignore_content() -> String {
    let mut content = String::new();
    content.push_str("# dupfinder ignore configuration file (.dupignore)\n");
    content.push_str(
        "# Lines starting with '#' are comments. Trailing '/' specifies directories.\n\n",
    );

    for category in PRESET_CATALOG {
        content.push_str(&format!("# ─── {} ───\n", category.name));
        content.push_str(&format!("# {}\n", category.description));
        for dir in category.dirs {
            content.push_str(&format!("{}/\n", dir));
        }
        for pattern in category.patterns {
            content.push_str(&format!("{}\n", pattern));
        }
        content.push('\n');
    }

    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_from_str() {
        assert_eq!(
            IgnorePreset::from_str_name("default"),
            Some(IgnorePreset::Default)
        );
        assert_eq!(
            IgnorePreset::from_str_name("jvm"),
            Some(IgnorePreset::JarsOnly)
        );
        assert_eq!(
            IgnorePreset::from_str_name("deps"),
            Some(IgnorePreset::Dependencies)
        );
        assert_eq!(
            IgnorePreset::from_str_name("build"),
            Some(IgnorePreset::BuildOnly)
        );
        assert_eq!(
            IgnorePreset::from_str_name("minimal"),
            Some(IgnorePreset::Minimal)
        );
        assert_eq!(
            IgnorePreset::from_str_name("none"),
            Some(IgnorePreset::None)
        );
        assert_eq!(IgnorePreset::from_str_name("invalid"), None);
    }

    #[test]
    fn test_default_preset_filtering() {
        let rules = IgnoreRules::from_preset(IgnorePreset::Default, false);
        assert!(rules.is_dir_excluded("node_modules"));
        assert!(rules.is_dir_excluded("target"));
        assert!(rules.is_dir_excluded(".gradle"));
        assert!(rules.is_dir_excluded("__pycache__"));
        assert!(rules.is_dir_excluded(".git"));

        assert!(rules.is_path_excluded(Path::new("app.jar")));
        assert!(rules.is_path_excluded(Path::new("/path/to/module.class")));
        assert!(rules.is_path_excluded(Path::new("script.pyc")));
        assert!(rules.is_path_excluded(Path::new("libcore.rlib")));
        assert!(rules.is_path_excluded(Path::new("main.o")));

        assert!(!rules.is_path_excluded(Path::new("main.rs")));
        assert!(!rules.is_path_excluded(Path::new("index.ts")));
    }

    #[test]
    fn test_include_jars_override() {
        let rules = IgnoreRules::from_preset(IgnorePreset::Default, true);
        assert!(!rules.is_path_excluded(Path::new("app.jar")));
        assert!(!rules.is_path_excluded(Path::new("library.war")));
        assert!(rules.is_path_excluded(Path::new("main.o")));
        assert!(rules.is_dir_excluded("node_modules"));
    }

    #[test]
    fn test_parse_ignore_content() {
        let mut rules = IgnoreRules::empty();
        let content = "
        # Custom ignore
        my_custom_cache/
        *.backup
        temp_*
        ";
        rules.parse_ignore_content(content);
        assert!(rules.is_dir_excluded("my_custom_cache"));
        assert!(rules.is_path_excluded(Path::new("data.backup")));
        assert!(rules.is_path_excluded(Path::new("temp_file.txt")));
        assert!(!rules.is_path_excluded(Path::new("normal.txt")));
    }
}
