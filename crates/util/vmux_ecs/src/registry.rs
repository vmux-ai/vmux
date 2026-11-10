use std::collections::HashMap;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;

use crate::cli::CliCommandManifest;
use crate::manifest::{FeatureId, FeatureManifest, FeatureManifestSource};

pub(crate) struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Declarations>();
    }

    fn finish(&self, app: &mut App) {
        let Some(declarations) = app.world_mut().remove_resource::<Declarations>() else {
            return;
        };
        let registry = Registry::new(declarations.0)
            .unwrap_or_else(|error| panic!("invalid feature registry: {error}"));
        app.insert_resource(registry);
    }
}

#[derive(Resource, Default)]
pub(crate) struct Declarations(HashMap<FeatureId, Entry>);

impl Declarations {
    pub(crate) fn register<M: FeatureManifestSource>(&mut self) -> Option<FeatureManifest> {
        let id = FeatureId::of::<M>();
        if self.0.contains_key(&id) {
            return None;
        }
        let manifest = FeatureManifest::of::<M>();
        self.0.insert(
            id,
            Entry {
                owner: std::any::type_name::<M>(),
                manifest: manifest.clone(),
            },
        );
        Some(manifest)
    }
}

struct Entry {
    owner: &'static str,
    manifest: FeatureManifest,
}

#[derive(Resource)]
pub struct Registry(HashMap<FeatureId, Entry>);

impl Registry {
    pub fn manifest<M: FeatureManifestSource>(&self) -> Option<&FeatureManifest> {
        Some(&self.0.get(&FeatureId::of::<M>())?.manifest)
    }

    fn new(entries: HashMap<FeatureId, Entry>) -> Result<Self, String> {
        let mut claims = Claims::default();
        let mut ordered = entries.values().collect::<Vec<_>>();
        ordered.sort_by_key(|entry| entry.owner);
        for entry in ordered {
            claims.include(entry.owner, &entry.manifest)?;
        }
        Ok(Self(entries))
    }
}

#[derive(Default)]
struct Claims(HashMap<(&'static str, String), &'static str>);

impl Claims {
    fn include(&mut self, owner: &'static str, manifest: &FeatureManifest) -> Result<(), String> {
        for page in &manifest.pages {
            self.claim("page", &page.url, owner)?;
        }
        for command in &manifest.commands {
            self.claim("command", &command.id, owner)?;
            for alias in &command.aliases {
                self.claim("command", alias, owner)?;
            }
        }
        for command in &manifest.command_bar {
            self.claim("command bar entry", &command.id, owner)?;
        }
        for engine in &manifest.search_engines {
            self.claim("search engine", &engine.id, owner)?;
        }
        for provider in &manifest.tool_providers {
            self.claim("tool provider", &provider.id, owner)?;
        }
        for tool in &manifest.tools {
            self.claim("tool", &tool.name, owner)?;
            for alias in &tool.aliases {
                self.claim("tool", alias, owner)?;
            }
        }
        for server in &manifest.mcp_servers {
            self.claim("MCP server", &server.id, owner)?;
        }
        if let Some(cli) = &manifest.cli {
            self.cli(owner, &cli.commands)?;
        }
        Ok(())
    }

    fn cli(&mut self, owner: &'static str, commands: &[CliCommandManifest]) -> Result<(), String> {
        for command in commands {
            self.claim("CLI command", &command.id, owner)?;
            self.cli(owner, &command.commands)?;
        }
        Ok(())
    }

    fn claim(&mut self, kind: &'static str, key: &str, owner: &'static str) -> Result<(), String> {
        if key.trim().is_empty() {
            return Err(format!("{kind} owned by `{owner}` requires an identifier"));
        }
        if let Some(first) = self.0.insert((kind, key.to_owned()), owner) {
            return Err(format!(
                "duplicate {kind} `{key}` owned by `{first}` and `{owner}`"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{FeatureManifestOwner, FeaturePlugin};

    struct Feature;
    struct Conflict;

    impl FeatureManifestSource for Feature {
        const SOURCE: &'static str = r#"(
            commands: [(id: "example.open", aliases: ["open"], label: "Open", group: "File")],
        )"#;
    }

    impl FeatureManifestSource for Conflict {
        const SOURCE: &'static str = r#"(
            commands: [(id: "open", label: "Open", group: "File")],
        )"#;
    }

    #[test]
    fn repeated_registration_freezes_one_manifest_and_projection() {
        let mut app = App::new();
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            FeaturePlugin::<Feature>::default(),
        ));
        app.finish();
        app.finish();
        assert!(
            app.world()
                .resource::<Registry>()
                .manifest::<Feature>()
                .is_some()
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<FeatureManifestOwner<Feature>>>()
                .iter(app.world())
                .count(),
            1
        );
        assert!(!app.world().contains_resource::<Declarations>());
    }

    #[test]
    #[should_panic(expected = "duplicate command `open`")]
    fn aliases_cannot_claim_another_feature_command() {
        let mut app = App::new();
        app.add_plugins((
            FeaturePlugin::<Feature>::default(),
            FeaturePlugin::<Conflict>::default(),
        ));
        app.finish();
    }

    #[test]
    fn bundled_manifests_have_unique_ownership() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        let mut claims = Claims::default();
        let mut count = 0;
        for group in ["feature", "util", "app"] {
            for entry in std::fs::read_dir(crates.join(group)).unwrap() {
                let path = entry.unwrap().path().join("src/feature.ron");
                if !path.is_file() {
                    continue;
                }
                let owner = Box::leak(path.display().to_string().into_boxed_str());
                let manifest = FeatureManifest::parse(&std::fs::read_to_string(&path).unwrap());
                claims.include(owner, &manifest).unwrap();
                count += 1;
            }
        }
        assert!(count >= 20);
    }
}
