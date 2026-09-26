#![allow(clippy::type_complexity)]

#[cfg(not(target_os = "ios"))]
mod connection;
mod dotfiles;
#[cfg(ui)]
mod extension;
mod homebrew;
mod manifest;
mod mcp;
mod npm;
#[cfg(ui)]
mod ui;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::{Task, futures_lite::future};
use vmux_core::tool::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolLinkRequest, ToolUninstallRequest, ToolUnlinkRequest, ToolUpdateRequest,
};

#[cfg(not(target_os = "ios"))]
pub use connection::McpConnectionPlugin;
pub use dotfiles::*;
pub use homebrew::*;
pub use manifest::*;
pub use mcp::*;
pub use npm::*;

pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(ui)]
        app.add_plugins((ui::ToolsPage::plugin(), extension::ExtensionsPage::plugin()));

        app.add_plugins((
            ToolRuntimePlugin,
            npm::NpmToolPlugin,
            homebrew::HomebrewToolPlugin,
            mcp::McpToolPlugin,
            dotfiles::DotfileToolPlugin,
        ));

        #[cfg(all(host, ui))]
        app.add_systems(bevy_app::Startup, spawn_tool_page);
    }
}

#[cfg(all(host, ui))]
fn spawn_tool_page(mut commands: Commands) {
    commands.spawn((
        ui::ToolsPage::MANIFEST,
        vmux_core::host::page::NativelyHosted::subtree(
            ui::ToolsPage::URL,
            ui::ToolsPage::NATIVE.title,
        ),
    ));
}

pub struct ToolRuntimePlugin;

impl Plugin for ToolRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (ToolOperationRouteSet, ToolOperationRouteFlush).chain(),
        )
        .add_systems(
            Update,
            bevy_ecs::schedule::ApplyDeferred.in_set(ToolOperationRouteFlush),
        )
        .add_systems(
            Update,
            (
                route_external_tool_operation::<ToolInstallRequest>,
                route_external_tool_operation::<ToolUpdateRequest>,
                route_external_tool_operation::<ToolUninstallRequest>,
                route_external_tool_operation::<ToolForgetRequest>,
                route_external_tool_operation::<ToolAdoptRequest>,
                route_external_tool_operation::<ToolLinkRequest>,
                route_external_tool_operation::<ToolUnlinkRequest>,
                route_external_tool_operation::<ToolApplyRequest>,
                route_external_tool_operation::<ToolImportRequest>,
            )
                .after(ToolOperationRouteFlush),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
pub struct ToolOperationRouteSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, SystemSet)]
struct ToolOperationRouteFlush;

#[derive(Component, Clone)]
pub struct ToolOperationRequest<R: Send + Sync + 'static>(pub R);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExternalToolOperation;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolStoreOperation;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolOperationFinished;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ToolOperationSucceeded(pub String);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolStoreTarget(pub Entity);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ToolOperationFailed(pub String);

#[derive(Component)]
pub(crate) struct ToolOperationTask<T: Component>(Task<Result<T, String>>);

pub(crate) fn finish_tool_operation<T: Component>(
    mut operations: Query<(Entity, &mut ToolOperationTask<T>)>,
    mut commands: Commands,
) {
    for (entity, mut operation) in &mut operations {
        let Some(result) = future::block_on(future::poll_once(&mut operation.0)) else {
            continue;
        };
        let mut entity = commands.entity(entity);
        entity.remove::<ToolOperationTask<T>>();
        match result {
            Ok(output) => {
                entity.insert(output);
            }
            Err(error) => {
                entity.insert((ToolOperationFinished, ToolOperationFailed(error)));
            }
        }
    }
}

fn route_external_tool_operation<R: Clone + Send + Sync + 'static>(
    requests: Query<
        Entity,
        (
            Added<ToolStoreTarget>,
            With<ToolOperationRequest<R>>,
            Without<ToolStoreOperation>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &requests {
        commands.entity(entity).insert(ExternalToolOperation);
    }
}

#[cfg(test)]
use homebrew::{sync_manifest_from_brewfile, write_brewfile_to};
#[cfg(test)]
use manifest::migrate_legacy_storage_in;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip_normalizes_packages() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tools.toml");
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "typescript", true);
        manifest.set_package("npm", "eslint", true);
        manifest.set_package("npm", "typescript", true);
        manifest.set_dotfile_package("shell", true);
        write_manifest_to(&path, &manifest).unwrap();

        let loaded = load_manifest_from(&path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "typescript"]);
        assert_eq!(loaded.dotfiles.packages, ["shell"]);
    }

    #[test]
    fn manifest_omits_empty_sections() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tools.toml");
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "typescript", true);

        write_manifest_to(&path, &manifest).unwrap();

        let source = std::fs::read_to_string(path).unwrap();
        assert!(source.contains("[packages]"));
        assert!(!source.contains("[mcp"));
        assert!(!source.contains("[dotfiles]"));
    }

    #[test]
    fn legacy_registry_storage_moves_to_tools() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_root = temp.path().join("registry");
        std::fs::create_dir_all(legacy_root.join("dotfiles/shell")).unwrap();
        std::fs::write(
            legacy_root.join("registry.toml"),
            "version = 1\n[packages]\nnpm = [\"typescript\"]\n",
        )
        .unwrap();
        std::fs::write(legacy_root.join("dotfiles/shell/.zshrc"), "export VMUX=1").unwrap();

        migrate_legacy_storage_in(temp.path()).unwrap();

        assert!(!legacy_root.exists());
        let tools_root = temp.path().join("tools");
        assert_eq!(
            load_manifest_from(&tools_root.join("tools.toml"))
                .unwrap()
                .packages["npm"],
            ["typescript"]
        );
        assert_eq!(
            std::fs::read_to_string(tools_root.join("dotfiles/shell/.zshrc")).unwrap(),
            "export VMUX=1"
        );
    }

    #[test]
    fn unsupported_manifest_versions_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tools.toml");
        std::fs::write(&path, "version = 2\n").unwrap();
        assert!(
            load_manifest_from(&path)
                .unwrap_err()
                .contains("unsupported tools manifest version: 2")
        );
    }

    #[test]
    fn brewfile_import_separates_formulae_and_casks() {
        let imported = parse_brewfile(
            r#"
tap "homebrew/cask-fonts"
brew "ripgrep"
brew 'openssl@3', link: false
cask "ghostty"
brew "ripgrep"
"#,
        );

        assert_eq!(imported.formulae, ["openssl@3", "ripgrep"]);
        assert_eq!(imported.casks, ["ghostty"]);
    }

    #[test]
    fn managed_brewfile_round_trips_homebrew_desired_state() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Brewfile");
        std::fs::write(
            &path,
            "tap \"homebrew/cask-fonts\"\n# keep this\nbrew \"fd\", link: false\nbrew \"old\"\nmas \"Xcode\", id: 497799835\n",
        )
        .unwrap();
        let mut manifest = ToolsManifest::default();
        manifest.set_package("homebrew-formula", "ripgrep", true);
        manifest.set_package("homebrew-formula", "fd", true);
        manifest.set_package("homebrew-cask", "ghostty", true);

        write_brewfile_to(&path, &manifest).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "tap \"homebrew/cask-fonts\"\n# keep this\nbrew \"fd\", link: false\nmas \"Xcode\", id: 497799835\nbrew \"ripgrep\"\ncask \"ghostty\"\n"
        );
        let mut loaded = ToolsManifest::default();
        sync_manifest_from_brewfile(&mut loaded, &path).unwrap();
        assert_eq!(loaded.packages, manifest.packages);
    }

    #[test]
    fn npm_import_combines_runtime_development_and_optional_dependencies() {
        let imported = parse_npm_manifest(
            r#"{
                "dependencies": {"typescript": "^5"},
                "devDependencies": {"eslint": "^9"},
                "optionalDependencies": {"prettier": "^3"},
                "peerDependencies": {"react": "^19"}
            }"#,
        )
        .unwrap();

        assert_eq!(imported, ["eslint", "prettier", "typescript"]);
    }

    #[test]
    fn npm_provider_routes_request_and_updates_the_target_store() {
        let temp = tempfile::tempdir().unwrap();
        let package_json = temp.path().join("package.json");
        std::fs::write(&package_json, r#"{"dependencies":{"typescript":"^5"}}"#).unwrap();
        let store = ToolStore::new(temp.path(), temp.path());
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolPlugin));
        let store_entity = app.world_mut().spawn(store.clone()).id();
        let operation = app
            .world_mut()
            .spawn((
                ToolOperationRequest(ToolImportRequest {
                    provider: vmux_core::tool::ToolProvider::Npm,
                    value: package_json.to_string_lossy().into_owned(),
                }),
                ToolStoreTarget(store_entity),
            ))
            .id();

        for _ in 0..100 {
            app.update();
            if app
                .world()
                .get::<ToolOperationSucceeded>(operation)
                .is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        let output = app.world().get::<ImportedNpmManifest>(operation).unwrap();
        assert_eq!(output, &ImportedNpmManifest { packages: 1 });
        assert_eq!(
            app.world().get::<ToolOperationSucceeded>(operation),
            Some(&ToolOperationSucceeded(
                "imported 1 NPM package(s)".to_string()
            )),
        );
        assert!(
            app.world()
                .get::<ExternalToolOperation>(operation)
                .is_none()
        );
        assert_eq!(store.load().unwrap().packages["npm"], ["typescript"]);
    }

    #[test]
    fn mcp_provider_discovers_and_imports_through_ecs() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(".mcp.json"),
            r#"{"mcpServers":{"docs":{"url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();
        let store = ToolStore::new(config, home);
        let mut app = App::new();
        app.add_plugins((bevy_app::TaskPoolPlugin::default(), ToolPlugin));
        let store_entity = app.world_mut().spawn(store.clone()).id();
        let operation = app
            .world_mut()
            .spawn((
                ToolOperationRequest(ToolImportRequest {
                    provider: vmux_core::tool::ToolProvider::Mcp,
                    value: String::new(),
                }),
                ToolStoreTarget(store_entity),
            ))
            .id();

        for _ in 0..100 {
            app.update();
            if app
                .world()
                .get::<ToolOperationSucceeded>(operation)
                .is_some()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert_eq!(
            app.world().get::<ToolOperationSucceeded>(operation),
            Some(&ToolOperationSucceeded(
                "imported 1 MCP server(s)".to_string()
            )),
        );
        assert!(
            app.world()
                .get::<ExternalToolOperation>(operation)
                .is_none()
        );
        assert_eq!(
            store.load().unwrap().mcp.servers["docs"].url.as_deref(),
            Some("https://example.com/mcp")
        );
    }

    #[test]
    fn mcp_import_normalizes_codex_and_vibe_formats() {
        let codex = parse_mcp_config(
            r#"
[mcp_servers.docs]
url = "https://example.com/mcp"
bearer_token_env_var = "DOCS_TOKEN"

[mcp_servers.local]
command = "npx"
args = ["-y", "server"]
[mcp_servers.local.env]
MODE = "local"
"#,
        )
        .unwrap();
        assert_eq!(codex["docs"].transport, McpTransport::Http);
        assert_eq!(
            codex["docs"].bearer_token_env_var.as_deref(),
            Some("DOCS_TOKEN")
        );
        assert_eq!(codex["local"].command.as_deref(), Some("npx"));
        assert_eq!(codex["local"].env["MODE"], "local");

        let vibe = parse_mcp_config(
            r#"
[[mcp_servers]]
name = "figma"
transport = "http"
url = "https://example.com/figma"

[[mcp_servers]]
name = "vmux"
transport = "stdio"
command = "vmux"
"#,
        )
        .unwrap();
        assert_eq!(vibe.keys().cloned().collect::<Vec<_>>(), ["figma"]);
    }

    #[test]
    fn mcp_import_normalizes_claude_json() {
        let imported = parse_mcp_config(
            r#"{
                "mcpServers": {
                    "notion": {"type": "http", "url": "https://example.com/notion"},
                    "local": {"command": "uvx", "args": ["server"]}
                }
            }"#,
        )
        .unwrap();

        assert_eq!(imported["notion"].transport, McpTransport::Http);
        assert_eq!(imported["local"].transport, McpTransport::Stdio);
    }

    #[test]
    fn config_without_mcp_section_is_ignored_during_discovery() {
        assert!(parse_mcp_config(r#"{"theme":"dark"}"#).unwrap().is_empty());
        assert!(
            parse_mcp_config("model = \"default\"\n")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn file_imports_merge_without_removing_existing_desired_state() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_path = temp.path().join("tools.toml");
        let brewfile = temp.path().join("Brewfile");
        let package_json = temp.path().join("package.json");
        let mcp = temp.path().join("mcp.json");
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "existing", true);
        write_manifest_to(&manifest_path, &manifest).unwrap();
        std::fs::write(&brewfile, "brew \"ripgrep\"\ncask \"ghostty\"\n").unwrap();
        std::fs::write(&package_json, r#"{"devDependencies":{"eslint":"1"}}"#).unwrap();
        std::fs::write(
            &mcp,
            r#"{"mcpServers":{"docs":{"url":"https://example.com"}}}"#,
        )
        .unwrap();

        assert_eq!(
            import_brewfile_to(&brewfile, &manifest_path).unwrap(),
            (1, 1)
        );
        assert_eq!(
            import_npm_manifest_to(&package_json, &manifest_path).unwrap(),
            1
        );
        assert_eq!(import_mcp_config_to(&mcp, &manifest_path).unwrap(), 1);
        let loaded = load_manifest_from(&manifest_path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "existing"]);
        assert!(loaded.mcp.servers.contains_key("docs"));
    }

    #[cfg(unix)]
    #[test]
    fn plan_apply_and_unlink_dotfile_package() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("tools/dotfiles");
        std::fs::create_dir_all(dotfiles.join("shell/.config/nushell")).unwrap();
        std::fs::write(dotfiles.join("shell/.config/nushell/config.nu"), "echo hi").unwrap();

        let plan = plan_dotfile_package_in(&dotfiles, &home, "shell").unwrap();
        assert_eq!(plan.missing(), 1);
        assert_eq!(
            apply_dotfile_package_in(&dotfiles, &home, "shell").unwrap(),
            1
        );
        let target = home.join(".config/nushell/config.nu");
        assert!(target.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "echo hi");
        assert_eq!(
            unlink_dotfile_package_in(&dotfiles, &home, "shell").unwrap(),
            1
        );
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn disable_and_unlink_dotfile_package_updates_links_and_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("tools/dotfiles");
        let manifest_path = temp.path().join("tools/tools.toml");
        std::fs::create_dir_all(dotfiles.join("shell")).unwrap();
        std::fs::write(dotfiles.join("shell/.zshrc"), "managed").unwrap();
        let mut manifest = ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        write_manifest_to(&manifest_path, &manifest).unwrap();
        apply_dotfile_package_in(&dotfiles, &home, "shell").unwrap();

        assert_eq!(
            disable_and_unlink_dotfile_package_in(&manifest_path, &dotfiles, &home, "shell")
                .unwrap(),
            1
        );
        assert!(!home.join(".zshrc").exists());
        assert!(
            load_manifest_from(&manifest_path)
                .unwrap()
                .dotfiles
                .packages
                .is_empty()
        );
    }

    #[cfg(unix)]
    #[test]
    fn conflicts_block_the_entire_apply() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("tools/dotfiles");
        std::fs::create_dir_all(dotfiles.join("git")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(dotfiles.join("git/.gitconfig"), "managed").unwrap();
        std::fs::write(home.join(".gitconfig"), "existing").unwrap();

        let error = apply_dotfile_package_in(&dotfiles, &home, "git").unwrap_err();
        assert!(error.contains("1 conflict"));
        assert_eq!(
            std::fs::read_to_string(home.join(".gitconfig")).unwrap(),
            "existing"
        );
    }

    #[cfg(unix)]
    #[test]
    fn enabled_packages_are_preflighted_before_any_links_are_created() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("tools/dotfiles");
        std::fs::create_dir_all(dotfiles.join("git")).unwrap();
        std::fs::create_dir_all(dotfiles.join("shell/.config/nushell")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(dotfiles.join("git/.gitconfig"), "managed").unwrap();
        std::fs::write(dotfiles.join("shell/.config/nushell/config.nu"), "echo hi").unwrap();
        std::fs::write(home.join(".gitconfig"), "existing").unwrap();
        let mut manifest = ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.set_dotfile_package("git", true);

        let result = apply_enabled_dotfiles_in(&manifest, &dotfiles, &home);

        assert!(result.unwrap_err().contains("git"));
        assert!(!home.join(".config/nushell/config.nu").exists());
    }

    #[cfg(unix)]
    #[test]
    fn adopt_moves_file_links_it_and_updates_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let dotfiles = temp.path().join("tools/dotfiles");
        let manifest = temp.path().join("tools/tools.toml");
        std::fs::create_dir_all(home.join(".config/nushell")).unwrap();
        let source = home.join(".config/nushell/config.nu");
        std::fs::write(&source, "echo hi").unwrap();

        let destination = adopt_dotfile_in(&dotfiles, &home, &manifest, &source, "shell").unwrap();
        assert_eq!(
            destination,
            dotfiles.join("shell/.config/nushell/config.nu")
        );
        assert!(source.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "echo hi");
        assert_eq!(
            load_manifest_from(&manifest).unwrap().dotfiles.packages,
            ["shell"]
        );
    }

    #[test]
    fn dotfile_import_copies_stow_packages_and_enables_them() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("stow");
        let dotfiles = temp.path().join("tools/dotfiles");
        let manifest = temp.path().join("tools/tools.toml");
        std::fs::create_dir_all(source.join("git")).unwrap();
        std::fs::create_dir_all(source.join("shell/.config/nushell")).unwrap();
        std::fs::write(source.join("git/.gitconfig"), "git").unwrap();
        std::fs::write(source.join("shell/.config/nushell/config.nu"), "nu").unwrap();

        assert_eq!(
            import_dotfiles_to(&source, &dotfiles, &manifest).unwrap(),
            2
        );
        assert_eq!(
            std::fs::read_to_string(dotfiles.join("git/.gitconfig")).unwrap(),
            "git"
        );
        assert_eq!(
            load_manifest_from(&manifest).unwrap().dotfiles.packages,
            ["git", "shell"]
        );
        assert!(source.join("git/.gitconfig").is_file());
    }
}
