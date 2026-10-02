use std::path::{Path, PathBuf};

#[cfg(test)]
use vmux_path::Executable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    pub command: String,
    pub args: Vec<String>,
    pub language_id: String,
    pub root_markers: Vec<String>,
}

impl ServerSpec {
    fn new(command: &str, args: &[&str], language_id: &str, markers: &[&str]) -> Self {
        Self {
            command: command.to_string(),
            args: args.iter().map(|argument| argument.to_string()).collect(),
            language_id: language_id.to_string(),
            root_markers: markers.iter().map(|marker| marker.to_string()).collect(),
        }
    }

    pub(crate) fn for_extension(extension: &str) -> Option<Self> {
        Some(match extension {
            "rs" => Self::new("rust-analyzer", &[], "rust", &["Cargo.toml", ".git"]),
            "py" | "pyi" => Self::new(
                "pyright-langserver",
                &["--stdio"],
                "python",
                &["pyproject.toml", "setup.py", ".git"],
            ),
            "ts" => Self::new(
                "typescript-language-server",
                &["--stdio"],
                "typescript",
                &["package.json", "tsconfig.json", ".git"],
            ),
            "tsx" => Self::new(
                "typescript-language-server",
                &["--stdio"],
                "typescriptreact",
                &["package.json", "tsconfig.json", ".git"],
            ),
            "js" => Self::new(
                "typescript-language-server",
                &["--stdio"],
                "javascript",
                &["package.json", ".git"],
            ),
            "jsx" => Self::new(
                "typescript-language-server",
                &["--stdio"],
                "javascriptreact",
                &["package.json", ".git"],
            ),
            "go" => Self::new("gopls", &[], "go", &["go.mod", ".git"]),
            "c" | "h" => Self::new("clangd", &[], "c", &["compile_commands.json", ".git"]),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => {
                Self::new("clangd", &[], "cpp", &["compile_commands.json", ".git"])
            }
            "lua" => Self::new("lua-language-server", &[], "lua", &[".luarc.json", ".git"]),
            "rb" => Self::new("solargraph", &["stdio"], "ruby", &["Gemfile", ".git"]),
            "zig" => Self::new("zls", &[], "zig", &["build.zig", ".git"]),
            "sh" | "bash" => {
                Self::new("bash-language-server", &["start"], "shellscript", &[".git"])
            }
            "json" => Self::new(
                "vscode-json-language-server",
                &["--stdio"],
                "json",
                &[".git"],
            ),
            "yaml" | "yml" => Self::new("yaml-language-server", &["--stdio"], "yaml", &[".git"]),
            "toml" => Self::new("taplo", &["lsp", "stdio"], "toml", &[".git"]),
            "md" | "markdown" => Self::new("marksman", &["server"], "markdown", &[".git"]),
            "java" => Self::new("jdtls", &[], "java", &["pom.xml", "build.gradle", ".git"]),
            _ => return None,
        })
    }

    pub(crate) fn resolve(
        extension: &str,
        overrides: &std::collections::BTreeMap<String, Self>,
    ) -> Option<Self> {
        overrides
            .get(extension)
            .cloned()
            .or_else(|| Self::for_extension(extension))
    }

    pub(crate) fn preferred_package(extension: &str) -> Option<&'static str> {
        Some(match extension {
            "rs" => "rust-analyzer",
            "py" | "pyi" => "pyright",
            "ts" | "tsx" | "js" | "jsx" => "typescript-language-server",
            "go" => "gopls",
            "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" => "clangd",
            "lua" => "lua-language-server",
            "rb" => "solargraph",
            "zig" => "zls",
            "sh" | "bash" => "bash-language-server",
            "json" => "json-lsp",
            "yaml" | "yml" => "yaml-language-server",
            "toml" => "taplo",
            "md" | "markdown" => "marksman",
            "java" => "jdtls",
            _ => return None,
        })
    }

    pub(crate) fn workspace_root(&self, start: &Path) -> PathBuf {
        let mut directory = Some(start);
        while let Some(path) = directory {
            for marker in &self.root_markers {
                if path.join(marker).exists() {
                    return path.to_path_buf();
                }
            }
            directory = path.parent();
        }
        start.to_path_buf()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LintFormat {
    Ruff,
    Eslint,
    Shellcheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinterSpec {
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) format: LintFormat,
}

impl LinterSpec {
    fn new(command: &str, args: &[&str], format: LintFormat) -> Self {
        Self {
            command: command.to_string(),
            args: args.iter().map(|argument| argument.to_string()).collect(),
            format,
        }
    }

    pub(crate) fn for_extension(extension: &str) -> Option<Self> {
        Some(match extension {
            "py" | "pyi" => Self::new(
                "ruff",
                &["check", "--output-format", "json"],
                LintFormat::Ruff,
            ),
            "js" | "jsx" | "ts" | "tsx" => {
                Self::new("eslint", &["--format", "json"], LintFormat::Eslint)
            }
            "sh" | "bash" => Self::new("shellcheck", &["--format", "json"], LintFormat::Shellcheck),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_map_to_servers() {
        assert_eq!(
            ServerSpec::for_extension("rs").unwrap().command,
            "rust-analyzer"
        );
        assert_eq!(ServerSpec::for_extension("rs").unwrap().language_id, "rust");
        assert_eq!(
            ServerSpec::for_extension("tsx").unwrap().language_id,
            "typescriptreact"
        );
        assert_eq!(ServerSpec::for_extension("cpp").unwrap().language_id, "cpp");
        assert!(ServerSpec::for_extension("xyzzy").is_none());
    }

    #[test]
    fn known_extensions_map_to_preferred_packages() {
        for (extension, package) in [
            ("rs", "rust-analyzer"),
            ("py", "pyright"),
            ("tsx", "typescript-language-server"),
            ("go", "gopls"),
            ("cpp", "clangd"),
            ("lua", "lua-language-server"),
            ("rb", "solargraph"),
            ("zig", "zls"),
            ("sh", "bash-language-server"),
            ("json", "json-lsp"),
            ("yaml", "yaml-language-server"),
            ("toml", "taplo"),
            ("md", "marksman"),
            ("java", "jdtls"),
        ] {
            assert_eq!(ServerSpec::preferred_package(extension), Some(package));
        }
        assert_eq!(ServerSpec::preferred_package("xyzzy"), None);
    }

    #[test]
    fn executable_lookup_finds_a_real_binary() {
        assert!(Executable::find("cargo").is_some());
        assert!(Executable::find("definitely-not-a-real-binary-zzz").is_none());
    }

    #[test]
    fn workspace_root_finds_marker_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();
        let nested = root.join("crates").join("a").join("src");
        std::fs::create_dir_all(&nested).unwrap();
        let found = ServerSpec::new("", &[], "", &["Cargo.toml", ".git"]).workspace_root(&nested);
        assert_eq!(found, root);
    }

    #[test]
    fn workspace_root_falls_back_to_start() {
        let tmp = tempfile::tempdir().unwrap();
        let start = tmp.path().join("no").join("markers");
        std::fs::create_dir_all(&start).unwrap();
        assert_eq!(
            ServerSpec::new("", &[], "", &["Cargo.toml"]).workspace_root(&start),
            start
        );
    }

    #[test]
    fn linters_map_by_extension() {
        assert_eq!(LinterSpec::for_extension("py").unwrap().command, "ruff");
        assert_eq!(
            LinterSpec::for_extension("ts").unwrap().format,
            LintFormat::Eslint
        );
        assert_eq!(
            LinterSpec::for_extension("sh").unwrap().command,
            "shellcheck"
        );
        assert!(LinterSpec::for_extension("rs").is_none());
    }

    #[test]
    fn override_takes_precedence_over_builtin() {
        let mut ov = std::collections::BTreeMap::new();
        ov.insert(
            "rs".to_string(),
            ServerSpec {
                command: "my-ra".into(),
                args: vec![],
                language_id: "rust".into(),
                root_markers: vec![".git".into()],
            },
        );
        assert_eq!(ServerSpec::resolve("rs", &ov).unwrap().command, "my-ra");
        assert_eq!(ServerSpec::resolve("go", &ov).unwrap().command, "gopls");
        assert!(ServerSpec::resolve("zzz", &ov).is_none());
    }
}
