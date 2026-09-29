use std::io;
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_core::cli::{CliInvocation, CliManifestPlugin, CliResult};

use crate::DotfileLinkState;

pub struct ToolCliPlugin;

impl Plugin for ToolCliPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CliManifestPlugin::from_feature(include_str!("feature.ron")))
            .add_systems(
                Update,
                (
                    route_tool_cli,
                    execute_status,
                    execute_apply,
                    execute_homebrew_import,
                    execute_npm_import,
                    execute_mcp_import,
                    execute_dotfile_import,
                    execute_adopt,
                    execute_unlink,
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
struct McpImportRequest(Option<PathBuf>);

#[derive(Component)]
struct DotfileImportRequest(Option<PathBuf>);

#[derive(Component)]
struct DotfileAdoptRequest {
    path: PathBuf,
    package: String,
}

#[derive(Component)]
struct DotfileUnlinkRequest(String);

fn route_tool_cli(
    invocations: Query<(Entity, &CliInvocation), Added<CliInvocation>>,
    mut commands: Commands,
) {
    for (entity, invocation) in &invocations {
        let mut entity = commands.entity(entity);
        match invocation.command.as_str() {
            "tool.status" => {
                entity.insert(ToolStatusRequest);
            }
            "tool.apply" => {
                entity.insert(ToolApplyRequest);
            }
            "tool.import.homebrew" => {
                if let Some(path) = invocation.value_os("path") {
                    entity.insert(HomebrewImportRequest(PathBuf::from(path)));
                }
            }
            "tool.import.npm" => {
                if let Some(path) = invocation.value_os("path") {
                    entity.insert(NpmImportRequest(PathBuf::from(path)));
                }
            }
            "tool.import.mcp" => {
                entity.insert(McpImportRequest(
                    invocation.value_os("path").map(PathBuf::from),
                ));
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

fn execute_status(requests: Query<Entity, Added<ToolStatusRequest>>, mut commands: Commands) {
    for entity in &requests {
        commands
            .entity(entity)
            .insert(CliResult::from_unit(status()));
    }
}

fn execute_apply(requests: Query<Entity, Added<ToolApplyRequest>>, mut commands: Commands) {
    let store = crate::ToolStore::current();
    for entity in &requests {
        let result = store.load().map_err(io::Error::other).and_then(|manifest| {
            store
                .apply_enabled_dotfiles(&manifest)
                .map_err(io::Error::other)
        });
        match result {
            Ok(linked) => {
                println!("linked {linked} file(s)");
                commands.entity(entity).insert(CliResult::success());
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
    requests: Query<(Entity, &HomebrewImportRequest), Added<HomebrewImportRequest>>,
    mut commands: Commands,
) {
    let store = crate::ToolStore::current();
    for (entity, request) in &requests {
        let result = store.import_brewfile(&request.0).map_err(io::Error::other);
        match result {
            Ok((formulae, casks)) => {
                println!("imported {formulae} formulae and {casks} casks");
                commands.entity(entity).insert(CliResult::success());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn execute_npm_import(
    requests: Query<(Entity, &NpmImportRequest), Added<NpmImportRequest>>,
    mut commands: Commands,
) {
    let store = crate::ToolStore::current();
    for (entity, request) in &requests {
        let result = store
            .import_npm_manifest(&request.0)
            .map_err(io::Error::other);
        match result {
            Ok(imported) => {
                println!("imported {imported} NPM package(s)");
                commands.entity(entity).insert(CliResult::success());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn execute_mcp_import(
    requests: Query<(Entity, &McpImportRequest), Added<McpImportRequest>>,
    mut commands: Commands,
) {
    let store = crate::ToolStore::current();
    for (entity, request) in &requests {
        let result = match &request.0 {
            Some(path) => store.import_mcp_config(path),
            None => store.import_default_mcp_configs(),
        }
        .map_err(io::Error::other);
        match result {
            Ok(imported) => {
                println!("imported {imported} MCP server(s)");
                commands.entity(entity).insert(CliResult::success());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn execute_dotfile_import(
    requests: Query<(Entity, &DotfileImportRequest), Added<DotfileImportRequest>>,
    mut commands: Commands,
) {
    let store = crate::ToolStore::current();
    for (entity, request) in &requests {
        let result = match &request.0 {
            Some(path) => store.import_dotfiles(path).map_err(io::Error::other),
            None => import_dotfile_packages(&store),
        };
        match result {
            Ok(imported) => {
                println!("imported {imported} dotfile package(s)");
                commands.entity(entity).insert(CliResult::success());
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
    let store = crate::ToolStore::current();
    for (entity, request) in &requests {
        match store
            .adopt_dotfile(&request.path, &request.package)
            .map_err(io::Error::other)
        {
            Ok(destination) => {
                println!("{}", destination.display());
                commands.entity(entity).insert(CliResult::success());
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
    let store = crate::ToolStore::current();
    for (entity, request) in &requests {
        match store
            .disable_and_unlink_dotfile_package(&request.0)
            .map_err(io::Error::other)
        {
            Ok(removed) => {
                println!("unlinked {removed} file(s)");
                commands.entity(entity).insert(CliResult::success());
            }
            Err(error) => {
                commands
                    .entity(entity)
                    .insert(CliResult(Err(error.to_string())));
            }
        }
    }
}

fn import_dotfile_packages(store: &crate::ToolStore) -> io::Result<usize> {
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

fn status() -> io::Result<()> {
    let store = crate::ToolStore::current();
    let manifest = store.load().map_err(io::Error::other)?;
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
