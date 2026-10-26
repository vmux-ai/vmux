use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

use bevy_app::{App, Plugin};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::{Component, Resource, World};

use crate::{CliCommandManifest, FeatureManifest, FeatureManifestSource};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FeatureId(TypeId);

impl FeatureId {
    pub fn of<M: FeatureManifestSource>() -> Self {
        Self(TypeId::of::<M>())
    }
}

#[derive(Component)]
pub struct FeatureManifestOwner<M>(PhantomData<fn() -> M>);

#[derive(Clone)]
pub struct FeatureRegistryEntry {
    id: FeatureId,
    owner: &'static str,
    manifest: FeatureManifest,
    project: fn(&mut World, &FeatureRegistryEntry),
}

impl FeatureRegistryEntry {
    pub fn id(&self) -> FeatureId {
        self.id
    }

    pub fn owner(&self) -> &'static str {
        self.owner
    }

    pub fn manifest(&self) -> &FeatureManifest {
        &self.manifest
    }

    fn of<M: FeatureManifestSource>() -> Self {
        Self {
            id: FeatureId::of::<M>(),
            owner: std::any::type_name::<M>(),
            manifest: FeatureManifest::of::<M>(),
            project: project::<M>,
        }
    }
}

#[derive(Resource, Default)]
struct FeatureDeclarations {
    entries: Vec<FeatureRegistryEntry>,
}

impl FeatureDeclarations {
    fn register<M: FeatureManifestSource>(&mut self) {
        let id = FeatureId::of::<M>();
        if self.entries.iter().any(|entry| entry.id == id) {
            return;
        }
        self.entries.push(FeatureRegistryEntry::of::<M>());
    }

    fn freeze(self) -> Result<FeatureRegistry, FeatureRegistryError> {
        FeatureRegistry::new(self.entries)
    }
}

#[derive(Resource)]
pub struct FeatureRegistry {
    entries: Vec<FeatureRegistryEntry>,
    indexes: HashMap<FeatureId, usize>,
}

impl FeatureRegistry {
    pub fn get(&self, id: FeatureId) -> Option<&FeatureManifest> {
        let index = *self.indexes.get(&id)?;
        Some(&self.entries[index].manifest)
    }

    pub fn manifest<M: FeatureManifestSource>(&self) -> Option<&FeatureManifest> {
        self.get(FeatureId::of::<M>())
    }

    pub fn entries(&self) -> impl Iterator<Item = &FeatureRegistryEntry> {
        self.entries.iter()
    }

    pub fn manifests(&self) -> impl Iterator<Item = &FeatureManifest> {
        self.entries.iter().map(FeatureRegistryEntry::manifest)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn new(entries: Vec<FeatureRegistryEntry>) -> Result<Self, FeatureRegistryError> {
        let mut claims = HashMap::new();
        let mut indexes = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            Self::claim(&mut claims, "feature", entry.owner, entry.owner)?;
            Self::validate(entry.owner, &entry.manifest, &mut claims)?;
            indexes.insert(entry.id, index);
        }
        Ok(Self { entries, indexes })
    }

    fn validate(
        owner: &'static str,
        manifest: &FeatureManifest,
        claims: &mut HashMap<(&'static str, String), &'static str>,
    ) -> Result<(), FeatureRegistryError> {
        for page in &manifest.pages {
            Self::claim(claims, "page", &page.url, owner)?;
        }
        for command in &manifest.commands {
            Self::claim(claims, "command", &command.id, owner)?;
            for alias in &command.aliases {
                Self::claim(claims, "command", alias, owner)?;
            }
        }
        for command in &manifest.command_bar {
            Self::claim(claims, "command bar entry", &command.id, owner)?;
        }
        for engine in &manifest.search_engines {
            Self::claim(claims, "search engine", &engine.id, owner)?;
        }
        for provider in &manifest.tool_providers {
            Self::claim(claims, "tool provider", &provider.id, owner)?;
        }
        for tool in &manifest.tools {
            Self::claim(claims, "tool", &tool.name, owner)?;
            for alias in &tool.aliases {
                Self::claim(claims, "tool", alias, owner)?;
            }
        }
        for server in &manifest.mcp_servers {
            Self::claim(claims, "MCP server", &server.id, owner)?;
        }
        for provider in &manifest.auth_providers {
            Self::claim(claims, "auth provider", &provider.id, owner)?;
        }
        if let Some(cli) = &manifest.cli {
            Self::validate_cli(&cli.commands, owner, claims)?;
        }
        Ok(())
    }

    fn validate_cli(
        commands: &[CliCommandManifest],
        owner: &'static str,
        claims: &mut HashMap<(&'static str, String), &'static str>,
    ) -> Result<(), FeatureRegistryError> {
        for command in commands {
            Self::claim(claims, "CLI command", &command.id, owner)?;
            Self::validate_cli(&command.commands, owner, claims)?;
        }
        Ok(())
    }

    fn claim(
        claims: &mut HashMap<(&'static str, String), &'static str>,
        kind: &'static str,
        key: &str,
        owner: &'static str,
    ) -> Result<(), FeatureRegistryError> {
        if key.is_empty() {
            return Err(FeatureRegistryError::empty(kind, owner));
        }
        let claim = (kind, key.to_owned());
        if let Some(first_owner) = claims.insert(claim, owner) {
            return Err(FeatureRegistryError::conflict(
                kind,
                key,
                first_owner,
                owner,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FeatureRegistryError {
    kind: &'static str,
    key: String,
    first_owner: &'static str,
    second_owner: Option<&'static str>,
}

impl FeatureRegistryError {
    fn empty(kind: &'static str, owner: &'static str) -> Self {
        Self {
            kind,
            key: String::new(),
            first_owner: owner,
            second_owner: None,
        }
    }

    fn conflict(
        kind: &'static str,
        key: &str,
        first_owner: &'static str,
        second_owner: &'static str,
    ) -> Self {
        Self {
            kind,
            key: key.to_owned(),
            first_owner,
            second_owner: Some(second_owner),
        }
    }
}

impl fmt::Display for FeatureRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(second_owner) = self.second_owner {
            return write!(
                formatter,
                "duplicate {} `{}` owned by `{}` and `{}`",
                self.kind, self.key, self.first_owner, second_owner
            );
        }
        write!(
            formatter,
            "{} owned by `{}` requires a non-empty identifier",
            self.kind, self.first_owner
        )
    }
}

pub struct FeatureRegistryPlugin;

impl Plugin for FeatureRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FeatureDeclarations>();
    }

    fn finish(&self, app: &mut App) {
        let declarations = app
            .world_mut()
            .remove_resource::<FeatureDeclarations>()
            .unwrap_or_default();
        let registry = declarations
            .freeze()
            .unwrap_or_else(|error| panic!("invalid feature registry: {error}"));
        for entry in registry.entries() {
            (entry.project)(app.world_mut(), entry);
        }
        app.insert_resource(registry);
    }
}

pub trait AppFeatureExt {
    fn register_feature<M: FeatureManifestSource>(&mut self) -> &mut Self;
}

impl AppFeatureExt for App {
    fn register_feature<M: FeatureManifestSource>(&mut self) -> &mut Self {
        if !self.is_plugin_added::<FeatureRegistryPlugin>() {
            self.add_plugins(FeatureRegistryPlugin);
        }
        self.world_mut()
            .resource_mut::<FeatureDeclarations>()
            .register::<M>();
        self
    }
}

fn project<M: FeatureManifestSource>(world: &mut World, entry: &FeatureRegistryEntry) {
    world.spawn((
        Name::new(entry.owner),
        entry.id,
        FeatureManifestOwner::<M>(PhantomData),
        entry.manifest.clone(),
    ));
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use bevy_app::App;
    use bevy_ecs::prelude::With;

    use super::*;

    struct Feature;
    struct OtherFeature;
    struct ConflictingFeature;
    struct WorkspaceManifests(Vec<PathBuf>);

    impl WorkspaceManifests {
        fn discover() -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .expect("workspace root")
                .join("crates");
            let mut manifests = Vec::new();
            Self::walk(&root, &mut manifests);
            manifests.sort();
            Self(manifests)
        }

        fn walk(path: &Path, manifests: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(path).expect("workspace crate directory") {
                let path = entry.expect("workspace crate entry").path();
                if path.is_dir() {
                    Self::walk(&path, manifests);
                    continue;
                }
                if path.file_name().is_some_and(|name| name == "feature.ron") {
                    manifests.push(path);
                }
            }
        }
    }

    impl FeatureManifestSource for Feature {
        const SOURCE: &'static str = r#"(
            pages: [(url: "vmux://example/", title: "Example")],
            commands: [(id: "example.open", label: "Open", group: "File")],
        )"#;
    }

    impl FeatureManifestSource for OtherFeature {
        const SOURCE: &'static str = r#"(
            pages: [(url: "vmux://other/", title: "Other")],
            commands: [(id: "other.open", label: "Open", group: "File")],
        )"#;
    }

    impl FeatureManifestSource for ConflictingFeature {
        const SOURCE: &'static str = r#"(
            commands: [(id: "example.open", label: "Open", group: "File")],
        )"#;
    }

    #[test]
    fn repeated_registration_freezes_one_entry_and_projection() {
        let mut app = App::new();
        app.register_feature::<Feature>()
            .register_feature::<Feature>();
        app.finish();

        assert_eq!(app.world().resource::<FeatureRegistry>().len(), 1);
        let projections = app
            .world_mut()
            .query_filtered::<&FeatureManifest, With<FeatureManifestOwner<Feature>>>()
            .iter(app.world())
            .count();
        assert_eq!(projections, 1);
    }

    #[test]
    fn distinct_features_with_distinct_ownership_freeze() {
        let mut app = App::new();
        app.register_feature::<Feature>()
            .register_feature::<OtherFeature>();
        app.finish();

        let registry = app.world().resource::<FeatureRegistry>();
        assert_eq!(registry.len(), 2);
        assert_eq!(
            registry.manifest::<OtherFeature>().unwrap().pages[0].url,
            "vmux://other/"
        );
    }

    #[test]
    #[should_panic(expected = "duplicate command `example.open`")]
    fn conflicting_ownership_fails_before_runtime() {
        let mut app = App::new();
        app.register_feature::<Feature>()
            .register_feature::<ConflictingFeature>();
        app.finish();
    }

    #[test]
    fn workspace_manifests_have_unique_ownership() {
        let manifests = WorkspaceManifests::discover();
        assert!(manifests.0.len() >= 20);

        let mut claims = HashMap::new();
        for path in manifests.0 {
            let source = std::fs::read_to_string(&path).expect("read feature manifest");
            let manifest = FeatureManifest::parse(&source);
            let owner = Box::leak(path.display().to_string().into_boxed_str());
            FeatureRegistry::claim(&mut claims, "feature", owner, owner)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            FeatureRegistry::validate(owner, &manifest, &mut claims)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        }
    }
}
