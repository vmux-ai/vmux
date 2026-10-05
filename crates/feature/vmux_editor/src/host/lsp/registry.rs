use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::Deserialize;
use vmux_ecs::manifest::FeatureManifest;
#[cfg(test)]
use vmux_path::Executable;

pub(crate) fn add(app: &mut App) {
    app.add_systems(Startup, load);
}

#[derive(Deserialize)]
struct EditorPolicies {
    lsp: LspRegistry,
}

#[derive(Component, Debug, Clone, Deserialize)]
pub(crate) struct LspRegistry {
    servers: Vec<ServerSpec>,
    linters: Vec<LinterSpec>,
}

fn load(
    manifests: Query<(Entity, &FeatureManifest), Added<FeatureManifest>>,
    mut commands: Commands,
) {
    for (entity, manifest) in &manifests {
        let policies = manifest
            .policies::<EditorPolicies>()
            .expect("editor feature manifest contains valid policies");
        if let Some(policies) = policies {
            commands.entity(entity).insert(policies.lsp);
        }
    }
}

impl LspRegistry {
    pub(crate) fn server(
        &self,
        extension: &str,
        overrides: &std::collections::BTreeMap<String, ServerSpec>,
    ) -> Option<ServerSpec> {
        overrides.get(extension).cloned().or_else(|| {
            self.servers
                .iter()
                .find(|server| server.supports(extension))
                .cloned()
        })
    }

    pub(crate) fn package(&self, extension: &str) -> Option<&str> {
        self.servers
            .iter()
            .find(|server| server.supports(extension))?
            .package
            .as_deref()
    }

    pub(crate) fn linter(&self, extension: &str) -> Option<LinterSpec> {
        self.linters
            .iter()
            .find(|linter| linter.supports(extension))
            .cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServerSpec {
    #[serde(default)]
    pub(crate) extensions: Vec<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub language_id: String,
    #[serde(default)]
    pub root_markers: Vec<String>,
    #[serde(default)]
    pub(crate) package: Option<String>,
}

impl ServerSpec {
    pub fn new(
        command: String,
        args: Vec<String>,
        language_id: String,
        root_markers: Vec<String>,
    ) -> Self {
        Self {
            extensions: Vec::new(),
            command,
            args,
            language_id,
            root_markers,
            package: None,
        }
    }

    fn supports(&self, extension: &str) -> bool {
        self.extensions
            .iter()
            .any(|candidate| candidate == extension)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum LintFormat {
    Ruff,
    Eslint,
    Shellcheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct LinterSpec {
    extensions: Vec<String>,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    pub(crate) format: LintFormat,
}

impl LinterSpec {
    fn supports(&self, extension: &str) -> bool {
        self.extensions
            .iter()
            .any(|candidate| candidate == extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> LspRegistry {
        FeatureManifest::of::<crate::Feature>()
            .policies::<EditorPolicies>()
            .unwrap()
            .unwrap()
            .lsp
    }

    #[test]
    fn known_extensions_map_to_servers() {
        let registry = registry();
        let overrides = Default::default();
        assert_eq!(
            registry.server("rs", &overrides).unwrap().command,
            "rust-analyzer"
        );
        assert_eq!(
            registry.server("rs", &overrides).unwrap().language_id,
            "rust"
        );
        assert_eq!(
            registry.server("tsx", &overrides).unwrap().language_id,
            "typescriptreact"
        );
        assert_eq!(
            registry.server("cpp", &overrides).unwrap().language_id,
            "cpp"
        );
        assert!(registry.server("xyzzy", &overrides).is_none());
    }

    #[test]
    fn known_extensions_map_to_preferred_packages() {
        let registry = registry();
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
            assert_eq!(registry.package(extension), Some(package));
        }
        assert_eq!(registry.package("xyzzy"), None);
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
        let found = ServerSpec {
            extensions: Vec::new(),
            command: String::new(),
            args: Vec::new(),
            language_id: String::new(),
            root_markers: vec!["Cargo.toml".into(), ".git".into()],
            package: None,
        }
        .workspace_root(&nested);
        assert_eq!(found, root);
    }

    #[test]
    fn workspace_root_falls_back_to_start() {
        let tmp = tempfile::tempdir().unwrap();
        let start = tmp.path().join("no").join("markers");
        std::fs::create_dir_all(&start).unwrap();
        assert_eq!(
            ServerSpec {
                extensions: Vec::new(),
                command: String::new(),
                args: Vec::new(),
                language_id: String::new(),
                root_markers: vec!["Cargo.toml".into()],
                package: None,
            }
            .workspace_root(&start),
            start
        );
    }

    #[test]
    fn linters_map_by_extension() {
        let registry = registry();
        assert_eq!(registry.linter("py").unwrap().command, "ruff");
        assert_eq!(registry.linter("ts").unwrap().format, LintFormat::Eslint);
        assert_eq!(registry.linter("sh").unwrap().command, "shellcheck");
        assert!(registry.linter("rs").is_none());
    }

    #[test]
    fn override_takes_precedence_over_builtin() {
        let registry = registry();
        let mut ov = std::collections::BTreeMap::new();
        ov.insert(
            "rs".to_string(),
            ServerSpec {
                extensions: vec!["rs".into()],
                command: "my-ra".into(),
                args: vec![],
                language_id: "rust".into(),
                root_markers: vec![".git".into()],
                package: None,
            },
        );
        assert_eq!(registry.server("rs", &ov).unwrap().command, "my-ra");
        assert_eq!(registry.server("go", &ov).unwrap().command, "gopls");
        assert!(registry.server("zzz", &ov).is_none());
    }
}
