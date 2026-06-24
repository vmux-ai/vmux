use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSpec {
    pub command: String,
    pub args: Vec<String>,
    pub language_id: String,
    pub root_markers: Vec<String>,
}

fn spec(command: &str, args: &[&str], language_id: &str, markers: &[&str]) -> ServerSpec {
    ServerSpec {
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        language_id: language_id.to_string(),
        root_markers: markers.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn builtin_spec(ext: &str) -> Option<ServerSpec> {
    Some(match ext {
        "rs" => spec("rust-analyzer", &[], "rust", &["Cargo.toml", ".git"]),
        "py" | "pyi" => spec(
            "pyright-langserver",
            &["--stdio"],
            "python",
            &["pyproject.toml", "setup.py", ".git"],
        ),
        "ts" => spec(
            "typescript-language-server",
            &["--stdio"],
            "typescript",
            &["package.json", "tsconfig.json", ".git"],
        ),
        "tsx" => spec(
            "typescript-language-server",
            &["--stdio"],
            "typescriptreact",
            &["package.json", "tsconfig.json", ".git"],
        ),
        "js" => spec(
            "typescript-language-server",
            &["--stdio"],
            "javascript",
            &["package.json", ".git"],
        ),
        "jsx" => spec(
            "typescript-language-server",
            &["--stdio"],
            "javascriptreact",
            &["package.json", ".git"],
        ),
        "go" => spec("gopls", &[], "go", &["go.mod", ".git"]),
        "c" | "h" => spec("clangd", &[], "c", &["compile_commands.json", ".git"]),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => {
            spec("clangd", &[], "cpp", &["compile_commands.json", ".git"])
        }
        "lua" => spec("lua-language-server", &[], "lua", &[".luarc.json", ".git"]),
        "rb" => spec("solargraph", &["stdio"], "ruby", &["Gemfile", ".git"]),
        "zig" => spec("zls", &[], "zig", &["build.zig", ".git"]),
        "sh" | "bash" => spec("bash-language-server", &["start"], "shellscript", &[".git"]),
        "json" => spec(
            "vscode-json-language-server",
            &["--stdio"],
            "json",
            &[".git"],
        ),
        "yaml" | "yml" => spec("yaml-language-server", &["--stdio"], "yaml", &[".git"]),
        "toml" => spec("taplo", &["lsp", "stdio"], "toml", &[".git"]),
        "md" | "markdown" => spec("marksman", &["server"], "markdown", &[".git"]),
        "java" => spec("jdtls", &[], "java", &["pom.xml", "build.gradle", ".git"]),
        _ => return None,
    })
}

pub fn resolve_spec(
    ext: &str,
    overrides: &std::collections::BTreeMap<String, ServerSpec>,
) -> Option<ServerSpec> {
    overrides.get(ext).cloned().or_else(|| builtin_spec(ext))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintFormat {
    Ruff,
    Eslint,
    Shellcheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinterSpec {
    pub command: String,
    pub args: Vec<String>,
    pub format: LintFormat,
}

fn linter(command: &str, args: &[&str], format: LintFormat) -> LinterSpec {
    LinterSpec {
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        format,
    }
}

pub fn linter_for(ext: &str) -> Option<LinterSpec> {
    Some(match ext {
        "py" | "pyi" => linter(
            "ruff",
            &["check", "--output-format", "json"],
            LintFormat::Ruff,
        ),
        "js" | "jsx" | "ts" | "tsx" => linter("eslint", &["--format", "json"], LintFormat::Eslint),
        "sh" | "bash" => linter("shellcheck", &["--format", "json"], LintFormat::Shellcheck),
        _ => return None,
    })
}

pub fn executable_on_path(command: &str) -> bool {
    let p = Path::new(command);
    if p.is_absolute() {
        return p.is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(command).is_file())
}

pub fn workspace_root(start: &Path, markers: &[String]) -> PathBuf {
    let mut dir = Some(start);
    while let Some(d) = dir {
        for m in markers {
            if d.join(m).exists() {
                return d.to_path_buf();
            }
        }
        dir = d.parent();
    }
    start.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_extensions_map_to_servers() {
        assert_eq!(builtin_spec("rs").unwrap().command, "rust-analyzer");
        assert_eq!(builtin_spec("rs").unwrap().language_id, "rust");
        assert_eq!(builtin_spec("tsx").unwrap().language_id, "typescriptreact");
        assert_eq!(builtin_spec("cpp").unwrap().language_id, "cpp");
        assert!(builtin_spec("xyzzy").is_none());
    }

    #[test]
    fn executable_lookup_finds_a_real_binary() {
        assert!(executable_on_path("cargo"));
        assert!(!executable_on_path("definitely-not-a-real-binary-zzz"));
    }

    #[test]
    fn workspace_root_finds_marker_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();
        let nested = root.join("crates").join("a").join("src");
        std::fs::create_dir_all(&nested).unwrap();
        let found = workspace_root(&nested, &["Cargo.toml".into(), ".git".into()]);
        assert_eq!(found, root);
    }

    #[test]
    fn workspace_root_falls_back_to_start() {
        let tmp = tempfile::tempdir().unwrap();
        let start = tmp.path().join("no").join("markers");
        std::fs::create_dir_all(&start).unwrap();
        assert_eq!(workspace_root(&start, &["Cargo.toml".into()]), start);
    }

    #[test]
    fn linters_map_by_extension() {
        assert_eq!(linter_for("py").unwrap().command, "ruff");
        assert_eq!(linter_for("ts").unwrap().format, LintFormat::Eslint);
        assert_eq!(linter_for("sh").unwrap().command, "shellcheck");
        assert!(linter_for("rs").is_none());
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
        assert_eq!(resolve_spec("rs", &ov).unwrap().command, "my-ra");
        assert_eq!(resolve_spec("go", &ov).unwrap().command, "gopls");
        assert!(resolve_spec("zzz", &ov).is_none());
    }
}
