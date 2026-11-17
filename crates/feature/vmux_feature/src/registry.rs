use std::collections::HashMap;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ScheduleLabel;

use crate::cli::CliCommandManifest;
use crate::manifest::{FeatureId, FeatureManifest, FeatureManifestSource};

pub(crate) struct RegistryPlugin;

impl Plugin for RegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(Freeze).add_systems(Freeze, freeze);
    }

    fn finish(&self, app: &mut App) {
        if app.world().contains_resource::<Registry>() {
            return;
        }
        app.world_mut().run_schedule(Freeze);
    }
}

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct Freeze;

fn freeze(
    registry: Option<Res<Registry>>,
    sources: Query<(&FeatureId, &Name, &FeatureManifest)>,
    mut commands: Commands,
) {
    if registry.is_some() {
        return;
    }
    let mut entries = HashMap::new();
    for (id, name, manifest) in &sources {
        entries.insert(
            *id,
            Entry {
                owner: name.as_str().to_string(),
                manifest: manifest.clone(),
            },
        );
    }
    commands.insert_resource(
        Registry::new(entries).unwrap_or_else(|error| panic!("invalid feature registry: {error}")),
    );
}

struct Entry {
    owner: String,
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
        ordered.sort_by(|first, second| first.owner.cmp(&second.owner));
        for entry in ordered {
            claims.include(&entry.owner, &entry.manifest)?;
        }
        for owner in entries.keys() {
            for binding in inventory::iter::<crate::PageBindingRegistration> {
                binding.validate_owner(*owner)?;
            }
            for binding in inventory::iter::<crate::CommandBindingRegistration> {
                binding.validate_owner(*owner)?;
            }
        }
        Ok(Self(entries))
    }
}

#[derive(Default)]
struct Claims(HashMap<(&'static str, String), String>);

impl Claims {
    fn include(&mut self, owner: &str, manifest: &FeatureManifest) -> Result<(), String> {
        for flag in &manifest.flags {
            self.claim("flag", &flag.key, owner)?;
        }
        for provider in &manifest.auth_providers {
            self.claim("auth provider", &provider.id, owner)?;
        }
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

    fn cli(&mut self, owner: &str, commands: &[CliCommandManifest]) -> Result<(), String> {
        for command in commands {
            self.claim("CLI command", &command.id, owner)?;
            self.cli(owner, &command.commands)?;
        }
        Ok(())
    }

    fn claim(&mut self, kind: &'static str, key: &str, owner: &str) -> Result<(), String> {
        if key.trim().is_empty() {
            return Err(format!("{kind} owned by `{owner}` requires an identifier"));
        }
        if let Some(first) = self.0.insert((kind, key.to_owned()), owner.to_owned()) {
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
    struct MissingBinding;
    struct UnpermittedPage;

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

    impl FeatureManifestSource for MissingBinding {
        const SOURCE: &'static str =
            r#"(commands: [(id: "present", label: "Present", group: "File")])"#;
    }

    impl FeatureManifestSource for UnpermittedPage {
        const SOURCE: &'static str = r#"(pages: [(
            url: "vmux://restricted/", title: "Restricted", permissions: ["Allowed"],
        )])"#;
    }

    inventory::submit! {
        crate::CommandBindingRegistration::of::<Feature>(Some("example.open"))
    }

    inventory::submit! {
        crate::CommandBindingRegistration::of::<MissingBinding>(Some("absent"))
    }

    inventory::submit! {
        crate::PageBindingRegistration::of::<UnpermittedPage>(None, &["Denied"])
    }

    #[test]
    #[should_panic(expected = "does not permit subscribed UI state Denied")]
    fn registry_finish_refuses_page_subscriptions_before_the_host_runs() {
        let mut app = App::new();
        app.add_plugins(FeaturePlugin::<UnpermittedPage>::default());
        app.finish();
    }

    #[test]
    #[should_panic(expected = "feature must declare command absent exactly once")]
    fn registry_finish_refuses_an_owned_command_selector_missing_from_its_declaration() {
        let mut app = App::new();
        app.add_plugins(FeaturePlugin::<MissingBinding>::default());
        app.finish();
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
