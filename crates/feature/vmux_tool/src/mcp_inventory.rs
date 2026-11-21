use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use bevy::prelude::*;
use vmux_ecs::{EntityTarget, JobRequest, JobResult, Operation, OperationRetained, OperationState};

use crate::inventory::{Manifest, Scan};
use crate::io::ReadFile;
use crate::observation::{
    Accepted, Diagnostics, Finished, Observation, ObserveProviderRequest, ObservedTool,
};
use crate::state::{ToolOperationKind, ToolStatus};
use crate::{McpConfigDocument, McpServerManifest, ToolProviderId, ToolStore, ToolTaskSet};

pub(crate) struct McpInventoryPlugin;

impl Plugin for McpInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(observe).add_systems(
            Update,
            (read, decode, publish).chain().in_set(ToolTaskSet::Start),
        );
    }
}

#[derive(Component)]
struct Observing;

#[derive(Component)]
struct Source(PathBuf);

#[derive(Component)]
struct SourceFile;

#[derive(Component)]
struct SourceDone;

fn observe(
    trigger: On<ObserveProviderRequest>,
    providers: Query<&ToolProviderId>,
    observations: Query<(&ChildOf, &OperationState), With<Observation>>,
    scans: Query<&ToolStore, With<Scan>>,
    mut commands: Commands,
) {
    let request = trigger.event();
    if !providers
        .get(request.entity)
        .is_ok_and(|provider| provider.0.id() == "mcp")
    {
        return;
    }
    let Ok((parent, state)) = observations.get(request.observation) else {
        return;
    };
    if *state != OperationState::Running {
        return;
    }
    let Ok(store) = scans.get(parent.parent()) else {
        return;
    };
    commands
        .entity(request.observation)
        .insert((Accepted, Observing, Diagnostics::default()));
    for path in [
        ".codex/config.toml",
        ".claude.json",
        ".vibe/config.toml",
        ".gemini/settings.json",
        ".mcp.json",
    ] {
        let path = store.home().join(path);
        commands.spawn((
            Name::new("MCP configuration source"),
            Operation::bundle("tool.mcp.observe").unwrap().running(),
            OperationRetained,
            ChildOf(request.observation),
            EntityTarget::<Observation>::new(request.observation),
            Source(path.clone()),
            SourceFile,
            JobRequest(ReadFile {
                path,
                limit: 1024 * 1024,
            }),
        ));
    }
}

fn read(
    sources: Query<
        (
            Entity,
            &Source,
            &ChildOf,
            &OperationState,
            &JobResult<ReadFile>,
        ),
        Without<SourceDone>,
    >,
    mut diagnostics: Query<&mut Diagnostics>,
    mut commands: Commands,
) {
    for (entity, source, parent, state, result) in &sources {
        if *state != OperationState::Running {
            continue;
        }
        commands.entity(entity).remove::<JobResult<ReadFile>>();
        let result = match &result.0 {
            Ok(Ok(bytes)) => match &bytes.0 {
                Some(bytes) => std::str::from_utf8(bytes)
                    .map_err(|error| error.to_string())
                    .and_then(McpConfigDocument::parse)
                    .map(Some),
                None => Ok(None),
            },
            Ok(Err(error)) | Err(error) => Err(error.clone()),
        };
        match result {
            Ok(Some(document)) => {
                commands.entity(entity).insert(document);
            }
            Ok(None) => {
                commands
                    .entity(entity)
                    .insert((SourceDone, OperationState::Succeeded));
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert((SourceDone, OperationState::Succeeded));
                if let Ok(mut diagnostics) = diagnostics.get_mut(parent.parent()) {
                    diagnostics
                        .0
                        .push(format!("MCP Servers: {}: {error}", source.0.display()));
                }
            }
        }
    }
}

fn decode(
    sources: Query<
        (
            Entity,
            &Source,
            &ChildOf,
            &OperationState,
            &McpConfigDocument,
        ),
        Without<SourceDone>,
    >,
    mut diagnostics: Query<&mut Diagnostics>,
    mut commands: Commands,
) {
    for (entity, source, parent, state, document) in &sources {
        if *state != OperationState::Running {
            continue;
        }
        commands
            .entity(entity)
            .remove::<McpConfigDocument>()
            .insert((SourceDone, OperationState::Succeeded));
        let entries = match document.entries() {
            Ok(entries) => entries,
            Err(error) => {
                if let Ok(mut diagnostics) = diagnostics.get_mut(parent.parent()) {
                    diagnostics
                        .0
                        .push(format!("MCP Servers: {}: {error}", source.0.display()));
                }
                continue;
            }
        };
        let mut definitions = BTreeMap::new();
        let mut failed = false;
        for (name, value) in entries {
            if name == "vmux"
                || value.get("enabled").and_then(serde_json::Value::as_bool) == Some(false)
            {
                continue;
            }
            match McpServerManifest::decode(&value) {
                Ok(definition) => {
                    definitions.insert(name, definition);
                }
                Err(error) => {
                    if let Ok(mut diagnostics) = diagnostics.get_mut(parent.parent()) {
                        diagnostics
                            .0
                            .push(format!("MCP Servers: {}: {error}", source.0.display()));
                    }
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            continue;
        }
        for (name, definition) in definitions {
            commands.spawn((
                crate::observation::Id(name.clone()),
                Name::new(name),
                definition,
                Source(source.0.clone()),
                ChildOf(parent.parent()),
            ));
        }
    }
}

fn publish(
    observations: Query<(Entity, &ChildOf, &OperationState), (With<Observing>, Without<Finished>)>,
    sources: Query<(&ChildOf, Has<SourceDone>), With<SourceFile>>,
    definitions: Query<(
        &crate::observation::Id,
        &McpServerManifest,
        &Source,
        &ChildOf,
    )>,
    manifests: Query<&Manifest>,
    mut commands: Commands,
) {
    for (entity, parent, state) in &observations {
        if *state != OperationState::Running {
            continue;
        }
        if sources
            .iter()
            .any(|(owner, done)| owner.parent() == entity && !done)
        {
            continue;
        }
        let Ok(manifest) = manifests.get(parent.parent()) else {
            continue;
        };
        let mut names = BTreeSet::new();
        for (id, _, _, owner) in &definitions {
            if owner.parent() == entity {
                names.insert(id.0.clone());
            }
        }
        names.extend(manifest.desired.mcp.servers.keys().cloned());
        for name in names {
            let mut external = None;
            let mut paths = Vec::new();
            let mut conflict = false;
            for (id, definition, source, owner) in &definitions {
                if owner.parent() != entity || id.0 != name {
                    continue;
                }
                if let Some(first) = external {
                    conflict |= first != definition;
                } else {
                    external = Some(definition);
                }
                paths.push(source.0.to_string_lossy().into_owned());
            }
            let managed = manifest.desired.mcp.servers.contains_key(&name);
            let Some(definition) = manifest.desired.mcp.servers.get(&name).or(external) else {
                continue;
            };
            let detail = definition.description(&paths.join(", "), managed, conflict);
            commands.spawn((
                ObservedTool::new(
                    name.clone(),
                    name,
                    None,
                    None,
                    detail,
                    if conflict {
                        ToolStatus::Conflict
                    } else {
                        ToolStatus::Installed
                    },
                    if managed {
                        vec![ToolOperationKind::Forget]
                    } else {
                        Vec::new()
                    },
                ),
                ChildOf(entity),
            ));
        }
        commands.entity(entity).insert(Finished { error: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_definitions_merge_conflicts_without_mutating_managed_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        std::fs::create_dir_all(home.join(".gemini")).unwrap();
        std::fs::create_dir_all(home.join(".vibe")).unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::write(home.join(".claude.json"),
            r#"{"mcpServers":{"shared":{"url":"https://example.com/one"},"external":{"command":"server"},"vmux":{},"disabled":{"enabled":false}}}"#).unwrap();
        std::fs::write(
            home.join(".gemini/settings.json"),
            r#"{"mcpServers":{"shared":{"url":"https://example.com/two"}}}"#,
        )
        .unwrap();
        std::fs::write(home.join(".codex/config.toml"),
            "[mcp_servers.docs]\nurl = 'https://example.com/mcp'\nbearer_token_env_var = 'DOCS_TOKEN'\n[mcp_servers.local]\ncommand = 'npx'\nargs = ['-y', 'server']\n[mcp_servers.local.env]\nMODE = 'local'\n").unwrap();
        std::fs::write(home.join(".vibe/config.toml"),
            "[[mcp_servers]]\nname = 'figma'\ntransport = 'http'\nurl = 'https://example.com/figma'\n[[mcp_servers]]\nname = 'vmux'\n[[mcp_servers]]\nname = 'disabled'\nenabled = false\n").unwrap();
        std::fs::write(
            home.join(".mcp.json"),
            r#"{"mcpServers":{"a-partial":{"command":"server"},"z-invalid":{"type":"http"}}}"#,
        )
        .unwrap();
        let store = ToolStore::new(directory.path().join("config"), home);
        std::fs::create_dir_all(store.root()).unwrap();
        let original = "version = 1\n[mcp.servers.managed]\ntransport = 'http'\nurl = 'https://example.com/managed'\n";
        std::fs::write(store.manifest_path(), original).unwrap();
        let mut app = App::new();
        app.add_plugins((
            bevy::app::TaskPoolPlugin::default(),
            crate::ToolRuntimePlugin,
            crate::mcp::McpToolPlugin,
        ));
        let registry = app
            .world_mut()
            .spawn((
                crate::inventory::Registry::default(),
                crate::catalog::Catalog::default(),
                store.clone(),
                crate::ToolsManifest::default(),
                crate::inventory::Sequence::default(),
            ))
            .id();
        app.world_mut().spawn((
            ToolProviderId(crate::state::ToolProvider::new("mcp")),
            crate::observation::Handler,
        ));
        app.world_mut().trigger(crate::inventory::RefreshRequest {
            entity: registry,
            refresh_catalogs: false,
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !app
            .world()
            .get::<crate::catalog::Catalog>(registry)
            .unwrap()
            .loaded
        {
            app.update();
            assert!(
                std::time::Instant::now() < deadline,
                "MCP observations did not drain"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut view =
            bevy::ecs::system::SystemState::<crate::catalog::Inventory>::new(app.world_mut());
        let snapshot = view.get(app.world()).unwrap().snapshot(registry).unwrap();
        let items = &snapshot.categories[0].items;
        assert_eq!(
            items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["docs", "external", "figma", "local", "managed", "shared"]
        );
        assert!(!items[1].managed);
        assert!(items[1].operations.is_empty());
        assert!(items[4].managed);
        assert_eq!(items[4].operations, [ToolOperationKind::Forget]);
        assert_eq!(items[5].status, ToolStatus::Conflict);
        assert_eq!(snapshot.conflicts, 1);
        assert!(snapshot.error.contains(".mcp.json"));
        assert_eq!(
            std::fs::read_to_string(store.manifest_path()).unwrap(),
            original
        );
        assert!(store.lease().is_some());
    }
}
