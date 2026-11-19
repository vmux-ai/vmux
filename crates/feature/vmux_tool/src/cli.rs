use std::io;
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::cli::{CliInvocation, CliResult};
use vmux_feature::manifest::FeaturePlugin;

use crate::DotfileLinkState;
use crate::homebrew::ImportedBrewfile;
use crate::npm::ImportedNpmManifest;
use crate::storage::{ReadRequest, ReadResult, StoragePlugin, StoreLease};
use crate::{
    ImportBrewfile, ImportNpmManifest, ToolOperationFailed, ToolRuntimePlugin, ToolStore,
    ToolStoreTarget, ToolsManifest,
};

pub struct ToolCliPlugin;

impl Plugin for ToolCliPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StoragePlugin>() {
            app.add_plugins(StoragePlugin);
        }
        if !app.is_plugin_added::<ToolRuntimePlugin>() {
            app.add_plugins(ToolRuntimePlugin);
        }
        if !app.is_plugin_added::<crate::homebrew::HomebrewToolPlugin>() {
            app.add_plugins(crate::homebrew::HomebrewToolPlugin);
        }
        if !app.is_plugin_added::<crate::npm::NpmToolPlugin>() {
            app.add_plugins(crate::npm::NpmToolPlugin);
        }
        app.add_plugins(FeaturePlugin::<crate::Feature>::default())
            .add_systems(
                Update,
                (
                    route,
                    execute_status,
                    execute_apply,
                    execute_homebrew_import,
                    execute_npm_import,
                    execute_dotfile_import,
                    execute_adopt,
                    execute_unlink,
                    complete_imports,
                )
                    .chain(),
            );
    }
}

#[derive(Component)]
struct ToolStatusRequest;

#[derive(Component)]
struct ToolApplyRequest;

#[derive(Component)]
struct HomebrewImportRequest(PathBuf);

#[derive(Component)]
struct NpmImportRequest(PathBuf);

#[derive(Component)]
struct WaitingNpm;

#[derive(Component)]
struct WaitingBrewfile;

#[derive(Component)]
struct DotfileImportRequest(Option<PathBuf>);

#[derive(Component)]
struct DotfileAdoptRequest {
    path: PathBuf,
    package: String,
}

#[derive(Component)]
struct DotfileUnlinkRequest(String);

fn route(
    invocations: Query<(Entity, &CliInvocation, Option<&ToolStore>), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation, store) in &invocations {
        let store = store.cloned().unwrap_or_else(ToolStore::current);
        let mut entity = commands.entity(entity);
        match invocation.command.as_str() {
            "tool.status" => {
                entity.insert((
                    ToolStatusRequest,
                    store.clone(),
                    ReadRequest { retain_lease: true },
                ));
            }
            "tool.apply" => {
                entity.insert((
                    ToolApplyRequest,
                    store.clone(),
                    ReadRequest { retain_lease: true },
                ));
            }
            "tool.import.homebrew" => {
                if let Some(path) = invocation.value_os("path") {
                    entity.insert((HomebrewImportRequest(PathBuf::from(path)), store.clone()));
                } else {
                    entity.insert(CliResult(Err("Brewfile path is required".into())));
                }
            }
            "tool.import.npm" => {
                if let Some(path) = invocation.value_os("path") {
                    entity.insert((NpmImportRequest(PathBuf::from(path)), store.clone()));
                } else {
                    entity.insert(CliResult(Err("package.json path is required".into())));
                }
            }
            "tool.import.dotfiles" => {
                entity.insert(DotfileImportRequest(
                    invocation.value_os("path").map(PathBuf::from),
                ));
            }
            "tool.adopt" => {
                if let (Some(path), Some(package)) =
                    (invocation.value_os("path"), invocation.value("package"))
                {
                    entity.insert(DotfileAdoptRequest {
                        path: PathBuf::from(path),
                        package: package.to_string(),
                    });
                }
            }
            "tool.unlink" => {
                if let Some(package) = invocation.value("package") {
                    entity.insert(DotfileUnlinkRequest(package.to_string()));
                }
            }
            _ => {}
        }
    }
}

fn execute_status(
    requests: Query<(Entity, &ToolStore, &ReadResult), With<ToolStatusRequest>>,
    mut commands: Commands,
) {
    for (entity, store, loaded) in &requests {
        let result = match &loaded.0 {
            Ok(manifest) => status(store, manifest),
            Err(error) => Err(io::Error::other(error.clone())),
        };
        commands
            .entity(entity)
            .insert(CliResult::from(result))
            .remove::<(ToolStatusRequest, ReadResult, StoreLease, ToolStore)>();
    }
}

fn execute_apply(
    requests: Query<(Entity, &ToolStore, &ReadResult), With<ToolApplyRequest>>,
    mut commands: Commands,
) {
    for (entity, store, loaded) in &requests {
        let result = match &loaded.0 {
            Ok(manifest) => store
                .apply_enabled_dotfiles(manifest)
                .map_err(io::Error::other),
            Err(error) => Err(io::Error::other(error.clone())),
        };
        commands
            .entity(entity)
            .remove::<(ToolApplyRequest, ReadResult, StoreLease, ToolStore)>();
        match result {
            Ok(linked) => {
                println!("linked {linked} file(s)");
                commands.entity(entity).insert(CliResult::default());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn execute_homebrew_import(
    requests: Query<(Entity, &HomebrewImportRequest, &ToolStore), Added<HomebrewImportRequest>>,
    mut commands: Commands,
) {
    for (entity, request, store) in &requests {
        let store = commands.spawn((store.clone(), ChildOf(entity))).id();
        commands
            .entity(entity)
            .remove::<HomebrewImportRequest>()
            .insert((
                WaitingBrewfile,
                ImportBrewfile::new(request.0.clone()),
                ToolStoreTarget::new(store),
            ));
    }
}

fn execute_npm_import(
    requests: Query<(Entity, &NpmImportRequest, &ToolStore), Added<NpmImportRequest>>,
    mut commands: Commands,
) {
    for (entity, request, store) in &requests {
        let store = commands.spawn((store.clone(), ChildOf(entity))).id();
        commands
            .entity(entity)
            .remove::<NpmImportRequest>()
            .insert((
                WaitingNpm,
                ImportNpmManifest::new(request.0.clone()),
                ToolStoreTarget::new(store),
            ));
    }
}

fn complete_imports(
    requests: Query<
        (
            Entity,
            Option<&ImportedNpmManifest>,
            Option<&ImportedBrewfile>,
            Option<&ToolOperationFailed>,
            Option<&vmux_ecs::OperationState>,
        ),
        (
            Or<(With<WaitingNpm>, With<WaitingBrewfile>)>,
            Without<ReadRequest>,
            Without<crate::storage::WriteRequest>,
            Without<crate::storage::FileReadRequest>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, npm, brew, failure, state) in &requests {
        let result = if state == Some(&vmux_ecs::OperationState::Canceled) {
            CliResult(Err("tool operation was canceled".into()))
        } else if let Some(failure) = failure {
            CliResult(Err(failure.0.clone()))
        } else if let Some(npm) = npm {
            println!("imported {} NPM package(s)", npm.packages);
            CliResult::default()
        } else if let Some(brew) = brew {
            println!(
                "imported {} formulae and {} casks",
                brew.formulae, brew.casks
            );
            CliResult::default()
        } else {
            continue;
        };
        commands.entity(entity).insert(result).remove::<(
            WaitingNpm,
            WaitingBrewfile,
            ImportNpmManifest,
            ImportBrewfile,
        )>();
    }
}

fn execute_dotfile_import(
    requests: Query<(Entity, &DotfileImportRequest), Added<DotfileImportRequest>>,
    mut commands: Commands,
) {
    let store = ToolStore::current();
    for (entity, request) in &requests {
        let result = match &request.0 {
            Some(path) => store.import_dotfiles(path).map_err(io::Error::other),
            None => import_dotfile_packages(&store),
        };
        match result {
            Ok(imported) => {
                println!("imported {imported} dotfile package(s)");
                commands.entity(entity).insert(CliResult::default());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn execute_adopt(
    requests: Query<(Entity, &DotfileAdoptRequest), Added<DotfileAdoptRequest>>,
    mut commands: Commands,
) {
    let store = ToolStore::current();
    for (entity, request) in &requests {
        match store
            .adopt_dotfile(&request.path, &request.package)
            .map_err(io::Error::other)
        {
            Ok(destination) => {
                println!("{}", destination.display());
                commands.entity(entity).insert(CliResult::default());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn execute_unlink(
    requests: Query<(Entity, &DotfileUnlinkRequest), Added<DotfileUnlinkRequest>>,
    mut commands: Commands,
) {
    let store = ToolStore::current();
    for (entity, request) in &requests {
        match store
            .disable_and_unlink_dotfile_package(&request.0)
            .map_err(io::Error::other)
        {
            Ok(removed) => {
                println!("unlinked {removed} file(s)");
                commands.entity(entity).insert(CliResult::default());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn import_dotfile_packages(store: &ToolStore) -> io::Result<usize> {
    let packages = store.dotfile_packages().map_err(io::Error::other)?;
    let mut manifest = store.load().map_err(io::Error::other)?;
    let mut imported = 0;
    for package in packages {
        imported += usize::from(!manifest.dotfiles.packages.contains(&package));
        manifest.set_dotfile_package(&package, true);
    }
    store.save(&manifest).map_err(io::Error::other)?;
    Ok(imported)
}

fn status(store: &ToolStore, manifest: &ToolsManifest) -> io::Result<()> {
    println!("{}", store.root().display());
    for (provider, packages) in &manifest.packages {
        println!("{provider} ({})", packages.len());
        for package in packages {
            println!("  {package}");
        }
    }
    if !manifest.mcp.servers.is_empty() {
        println!("mcp ({})", manifest.mcp.servers.len());
        for (name, server) in &manifest.mcp.servers {
            println!("  {name} · {:?}", server.transport);
        }
    }
    let mut packages = store.dotfile_packages().map_err(io::Error::other)?;
    for package in &manifest.dotfiles.packages {
        if !packages.contains(package) {
            packages.push(package.clone());
        }
    }
    packages.sort();
    if !packages.is_empty() {
        println!("dotfiles ({})", packages.len());
    }
    for package in packages {
        let managed = manifest.dotfiles.packages.contains(&package);
        match store.plan_dotfile_package(&package) {
            Ok(plan) => println!(
                "  {}{} · {} linked · {} missing · {} conflicts",
                package,
                if managed { " [managed]" } else { "" },
                plan.links
                    .iter()
                    .filter(|link| link.state == DotfileLinkState::Linked)
                    .count(),
                plan.links
                    .iter()
                    .filter(|link| link.state == DotfileLinkState::Missing)
                    .count(),
                plan.links
                    .iter()
                    .filter(|link| link.state == DotfileLinkState::Conflict)
                    .count(),
            ),
            Err(error) => println!("  {package} · {error}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::time::{Duration, Instant};

    #[test]
    fn cli_imports_use_the_selected_store_and_preserve_existing_packages_and_brewfile_comments() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(directory.path(), directory.path());
        let mut original = ToolsManifest::default();
        original.set_package("npm", "preserved", true);
        original.write_to(&store.manifest_path()).unwrap();
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        for (command, path, contents) in [
            (
                "tool.import.homebrew",
                directory.path().join("input.Brewfile"),
                "tap \"homebrew/cask-fonts\"\n# keep\nbrew \"ripgrep\", link: false\n",
            ),
            (
                "tool.import.npm",
                directory.path().join("package.json"),
                r#"{"dependencies":{"eslint":"1"}}"#,
            ),
        ] {
            std::fs::write(&path, contents).unwrap();
            let entity = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: command.into(),
                        arguments: BTreeMap::from([("path".into(), vec![OsString::from(path)])]),
                    },
                ))
                .id();
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.world().get::<CliResult>(entity).is_none() {
                app.update();
                assert!(Instant::now() < deadline, "CLI import did not complete");
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(app.world().get::<CliResult>(entity).unwrap().0, Ok(0));
            app.world_mut().despawn(entity);
        }
        let saved = ToolsManifest::read(&store.manifest_path()).unwrap();
        assert_eq!(saved.packages["npm"], ["eslint", "preserved"]);
        assert_eq!(saved.packages["homebrew-formula"], ["ripgrep"]);
        assert_eq!(
            std::fs::read_to_string(store.brewfile_path()).unwrap(),
            "tap \"homebrew/cask-fonts\"\n# keep\nbrew \"ripgrep\", link: false\n"
        );
        assert!(!store.is_busy());
    }

    #[test]
    fn missing_cli_import_path_refuses_without_creating_storage() {
        let directory = tempfile::tempdir().unwrap();
        let store = ToolStore::new(directory.path(), directory.path());
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolCliPlugin));
        for command in ["tool.import.homebrew", "tool.import.npm"] {
            let entity = app
                .world_mut()
                .spawn((
                    store.clone(),
                    CliInvocation {
                        command: command.into(),
                        arguments: BTreeMap::new(),
                    },
                ))
                .id();
            app.update();
            assert!(app.world().get::<CliResult>(entity).unwrap().0.is_err());
        }
        assert!(!store.root().exists());
        assert!(!store.is_busy());
    }
}
