#![allow(clippy::type_complexity)]

use crate::state::{
    ToolAdoptRequest, ToolApplyRequest, ToolForgetRequest, ToolImportRequest, ToolInstallRequest,
    ToolLinkRequest, ToolUninstallRequest, ToolUnlinkRequest, ToolUpdateRequest,
};
use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use bevy_tasks::{Task, futures_lite::future};
#[cfg(host)]
pub use cli::ToolCliPlugin;
#[cfg(not(target_os = "ios"))]
pub use connection::McpConnectionPlugin;
pub use dotfiles::*;
pub use homebrew::*;
pub use manifest::*;
pub use mcp::*;
pub use npm::*;
pub use provider::*;
#[cfg(host)]
pub use query::*;
pub use registry::*;
use vmux_ecs::manifest::FeaturePlugin;
pub use vmux_macro::input;

#[doc(hidden)]
pub mod __private {
    pub use inventory;
}

extern crate self as vmux_tool;

pub(crate) struct Feature;

impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}

#[cfg(host)]
mod cli;
#[cfg(all(host, not(target_os = "ios")))]
mod command_bar;
#[cfg(not(target_os = "ios"))]
mod connection;
mod dotfiles;
mod homebrew;
#[cfg(all(host, ui))]
mod host;
mod manifest;
mod mcp;
mod npm;
mod process;
mod provider;
#[cfg(host)]
mod query;
mod registry;
#[cfg(ui)]
mod route;
pub mod state;
#[cfg(ui)]
mod ui;

#[vmux_page::page]
pub struct ToolPlugin;

impl Plugin for ToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FeaturePlugin::<Feature>::default());
        #[cfg(ui)]
        app.add_plugins(ui::ToolsPage::plugin());

        #[cfg(all(host, ui))]
        app.add_plugins(
            Self::MANIFEST
                .plugin()
                .hosted(vmux_ecs::page::HostedPage::subtree(
                    Self::URL,
                    ui::ToolsPage::PAGE.title,
                )),
        );

        app.add_plugins((
            ToolRuntimePlugin,
            npm::NpmToolPlugin,
            homebrew::HomebrewToolPlugin,
            mcp::McpToolPlugin,
            dotfiles::DotfileToolPlugin,
        ));

        #[cfg(all(host, not(target_os = "ios")))]
        app.add_plugins(command_bar::Plugin);

        #[cfg(all(host, ui))]
        app.add_plugins(host::ToolHostPlugin);
    }
}

pub struct ToolRuntimePlugin;

impl Plugin for ToolRuntimePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ToolRegistryPlugin>() {
            app.add_plugins(ToolRegistryPlugin);
        }
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
                route_external_operation::<ToolInstallRequest>,
                route_external_operation::<ToolUpdateRequest>,
                route_external_operation::<ToolUninstallRequest>,
                route_external_operation::<ToolForgetRequest>,
                route_external_operation::<ToolAdoptRequest>,
                route_external_operation::<ToolLinkRequest>,
                route_external_operation::<ToolUnlinkRequest>,
                route_external_operation::<ToolApplyRequest>,
                route_external_operation::<ToolImportRequest>,
            )
                .after(ToolOperationRouteFlush),
        )
        .add_systems(
            Update,
            (
                finish_operation::<mcp::DiscoveredMcpServers>,
                finish_operation::<mcp::ImportedMcpConfig>,
                finish_operation::<mcp::ImportedMcpServer>,
                finish_operation::<mcp::ForgottenMcpServer>,
                finish_operation::<dotfiles::DiscoveredDotfilePackages>,
                finish_operation::<dotfiles::DotfilePlan>,
                finish_operation::<dotfiles::ImportedDotfiles>,
                finish_operation::<dotfiles::ImportedAvailableDotfiles>,
                finish_operation::<dotfiles::LinkedDotfilePackage>,
                finish_operation::<dotfiles::DisabledDotfilePackage>,
                finish_operation::<dotfiles::UnlinkedDotfilePackage>,
                finish_operation::<dotfiles::AppliedEnabledDotfiles>,
                finish_operation::<dotfiles::AdoptedDotfile>,
                finish_operation::<homebrew::ImportedBrewfile>,
                finish_operation::<npm::ImportedNpmManifest>,
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

pub type ToolStoreTarget = vmux_ecs::EntityTarget<ToolStore>;
pub type ToolProviderTarget = vmux_ecs::EntityTarget<ToolProviderId>;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ToolOperationFailed(pub String);

#[derive(Component)]
pub(crate) struct ToolOperationTask<T: Component>(Task<Result<T, String>>);

fn finish_operation<T: Component>(
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

fn route_external_operation<R: Clone + Send + Sync + 'static>(
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
        manifest.write_to(&path).unwrap();

        let loaded = ToolsManifest::read(&path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "typescript"]);
        assert_eq!(loaded.dotfiles.packages, ["shell"]);
    }

    #[test]
    fn manifest_omits_empty_sections() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tools.toml");
        let mut manifest = ToolsManifest::default();
        manifest.set_package("npm", "typescript", true);

        manifest.write_to(&path).unwrap();

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

        ToolStore::new(temp.path(), temp.path())
            .migrate_legacy_storage()
            .unwrap();

        assert!(!legacy_root.exists());
        let tools_root = temp.path().join("tools");
        assert_eq!(
            ToolsManifest::read(&tools_root.join("tools.toml"))
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
            ToolsManifest::read(&path)
                .unwrap_err()
                .contains("unsupported tools manifest version: 2")
        );
    }

    #[test]
    fn brewfile_import_separates_formulae_and_casks() {
        let imported = BrewfileImport::parse(
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

        manifest.write_brewfile_to(&path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "tap \"homebrew/cask-fonts\"\n# keep this\nbrew \"fd\", link: false\nmas \"Xcode\", id: 497799835\nbrew \"ripgrep\"\ncask \"ghostty\"\n"
        );
        let mut loaded = ToolsManifest::default();
        loaded.sync_brewfile(&path).unwrap();
        assert_eq!(loaded.packages, manifest.packages);
    }

    #[test]
    fn npm_import_combines_runtime_development_and_optional_dependencies() {
        let imported = NpmManifest::parse(
            r#"{
                "dependencies": {"typescript": "^5"},
                "devDependencies": {"eslint": "^9"},
                "optionalDependencies": {"prettier": "^3"},
                "peerDependencies": {"react": "^19"}
            }"#,
        )
        .unwrap();

        assert_eq!(imported.packages, ["eslint", "prettier", "typescript"]);
    }

    #[test]
    fn npm_provider_routes_request_and_updates_the_target_store() {
        let temp = tempfile::tempdir().unwrap();
        let package_json = temp.path().join("package.json");
        std::fs::write(&package_json, r#"{"dependencies":{"typescript":"^5"}}"#).unwrap();
        let store = ToolStore::new(temp.path(), temp.path());
        let mut app = App::new();
        app.add_plugins((
            bevy_app::TaskPoolPlugin::default(),
            ToolRuntimePlugin,
            npm::NpmToolPlugin,
        ));
        app.update();
        let provider = {
            let mut providers = app
                .world_mut()
                .query::<(Entity, &ToolProviderBinding)>();
            providers
                .iter(app.world())
                .find_map(|(entity, binding)| (binding.index() == 2).then_some(entity))
                .unwrap()
        };
        let store_entity = app.world_mut().spawn(store.clone()).id();
        let operation = app
            .world_mut()
            .spawn((
                ToolOperationRequest(ToolImportRequest {
                    provider: state::ToolProvider::new("npm"),
                    value: package_json.to_string_lossy().into_owned(),
                }),
                ToolStoreTarget::new(store_entity),
                ToolProviderTarget::new(provider),
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
        assert_eq!(output.packages(), 1);
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
        app.add_plugins((
            bevy_app::TaskPoolPlugin::default(),
            ToolRuntimePlugin,
            mcp::McpToolPlugin,
        ));
        app.update();
        let provider = {
            let mut providers = app
                .world_mut()
                .query::<(Entity, &ToolProviderBinding)>();
            providers
                .iter(app.world())
                .find_map(|(entity, binding)| (binding.index() == 3).then_some(entity))
                .unwrap()
        };
        let store_entity = app.world_mut().spawn(store.clone()).id();
        let operation = app
            .world_mut()
            .spawn((
                ToolOperationRequest(ToolImportRequest {
                    provider: state::ToolProvider::new("mcp"),
                    value: String::new(),
                }),
                ToolStoreTarget::new(store_entity),
                ToolProviderTarget::new(provider),
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
        let codex = McpConfigDocument::parse(
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
        let codex = codex.servers();
        assert_eq!(codex["docs"].transport, McpTransport::Http);
        assert_eq!(
            codex["docs"].bearer_token_env_var.as_deref(),
            Some("DOCS_TOKEN")
        );
        assert_eq!(codex["local"].command.as_deref(), Some("npx"));
        assert_eq!(codex["local"].env["MODE"], "local");

        let vibe = McpConfigDocument::parse(
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
        assert_eq!(
            vibe.servers().keys().cloned().collect::<Vec<_>>(),
            ["figma"]
        );
    }

    #[test]
    fn mcp_import_normalizes_claude_json() {
        let imported = McpConfigDocument::parse(
            r#"{
                "mcpServers": {
                    "notion": {"type": "http", "url": "https://example.com/notion"},
                    "local": {"command": "uvx", "args": ["server"]}
                }
            }"#,
        )
        .unwrap();

        assert_eq!(imported.servers()["notion"].transport, McpTransport::Http);
        assert_eq!(imported.servers()["local"].transport, McpTransport::Stdio);
    }

    #[test]
    fn config_without_mcp_section_is_ignored_during_discovery() {
        assert!(
            McpConfigDocument::parse(r#"{"theme":"dark"}"#)
                .unwrap()
                .servers()
                .is_empty()
        );
        assert!(
            McpConfigDocument::parse("model = \"default\"\n")
                .unwrap()
                .servers()
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
        manifest.write_to(&manifest_path).unwrap();
        std::fs::write(&brewfile, "brew \"ripgrep\"\ncask \"ghostty\"\n").unwrap();
        std::fs::write(&package_json, r#"{"devDependencies":{"eslint":"1"}}"#).unwrap();
        std::fs::write(
            &mcp,
            r#"{"mcpServers":{"docs":{"url":"https://example.com"}}}"#,
        )
        .unwrap();

        let store = ToolStore::new(temp.path(), temp.path());
        assert_eq!(
            store.import_brewfile_to(&brewfile, &manifest_path).unwrap(),
            (1, 1)
        );
        assert_eq!(
            store
                .import_npm_manifest_to(&package_json, &manifest_path)
                .unwrap(),
            1
        );
        assert_eq!(store.import_mcp_config_to(&mcp, &manifest_path).unwrap(), 1);
        let loaded = ToolsManifest::read(&manifest_path).unwrap();
        assert_eq!(loaded.packages["npm"], ["eslint", "existing"]);
        assert!(loaded.mcp.servers.contains_key("docs"));
    }

    #[cfg(unix)]
    #[test]
    fn plan_apply_and_unlink_dotfile_package() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let store = ToolStore::new(temp.path(), &home);
        let dotfiles = store.dotfiles_dir();
        std::fs::create_dir_all(dotfiles.join("shell/.config/nushell")).unwrap();
        std::fs::write(dotfiles.join("shell/.config/nushell/config.nu"), "echo hi").unwrap();

        let plan = store.plan_dotfile_package("shell").unwrap();
        assert_eq!(plan.missing(), 1);
        assert_eq!(store.apply_dotfile_package("shell").unwrap(), 1);
        let target = home.join(".config/nushell/config.nu");
        assert!(target.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "echo hi");
        assert_eq!(store.unlink_dotfile_package("shell").unwrap(), 1);
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn disable_and_unlink_dotfile_package_updates_links_and_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let store = ToolStore::new(temp.path(), &home);
        let dotfiles = store.dotfiles_dir();
        let manifest_path = store.manifest_path();
        std::fs::create_dir_all(dotfiles.join("shell")).unwrap();
        std::fs::write(dotfiles.join("shell/.zshrc"), "managed").unwrap();
        let mut manifest = ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.write_to(&manifest_path).unwrap();
        store.apply_dotfile_package("shell").unwrap();

        assert_eq!(
            store.disable_and_unlink_dotfile_package("shell").unwrap(),
            1
        );
        assert!(!home.join(".zshrc").exists());
        assert!(
            ToolsManifest::read(&manifest_path)
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
        let store = ToolStore::new(temp.path(), &home);
        let dotfiles = store.dotfiles_dir();
        std::fs::create_dir_all(dotfiles.join("git")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(dotfiles.join("git/.gitconfig"), "managed").unwrap();
        std::fs::write(home.join(".gitconfig"), "existing").unwrap();

        let error = store.apply_dotfile_package("git").unwrap_err();
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
        let store = ToolStore::new(temp.path(), &home);
        let dotfiles = store.dotfiles_dir();
        std::fs::create_dir_all(dotfiles.join("git")).unwrap();
        std::fs::create_dir_all(dotfiles.join("shell/.config/nushell")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(dotfiles.join("git/.gitconfig"), "managed").unwrap();
        std::fs::write(dotfiles.join("shell/.config/nushell/config.nu"), "echo hi").unwrap();
        std::fs::write(home.join(".gitconfig"), "existing").unwrap();
        let mut manifest = ToolsManifest::default();
        manifest.set_dotfile_package("shell", true);
        manifest.set_dotfile_package("git", true);

        let result = store.apply_enabled_dotfiles(&manifest);

        assert!(result.unwrap_err().contains("git"));
        assert!(!home.join(".config/nushell/config.nu").exists());
    }

    #[cfg(unix)]
    #[test]
    fn adopt_moves_file_links_it_and_updates_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let store = ToolStore::new(temp.path(), &home);
        let dotfiles = store.dotfiles_dir();
        let manifest = store.manifest_path();
        std::fs::create_dir_all(home.join(".config/nushell")).unwrap();
        let source = home.join(".config/nushell/config.nu");
        std::fs::write(&source, "echo hi").unwrap();

        let destination = store.adopt_dotfile(&source, "shell").unwrap();
        assert_eq!(
            destination,
            dotfiles.join("shell/.config/nushell/config.nu")
        );
        assert!(source.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(source).unwrap(), "echo hi");
        assert_eq!(
            ToolsManifest::read(&manifest).unwrap().dotfiles.packages,
            ["shell"]
        );
    }

    #[test]
    fn dotfile_import_copies_stow_packages_and_enables_them() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("stow");
        let store = ToolStore::new(temp.path(), temp.path().join("home"));
        let dotfiles = store.dotfiles_dir();
        let manifest = store.manifest_path();
        std::fs::create_dir_all(source.join("git")).unwrap();
        std::fs::create_dir_all(source.join("shell/.config/nushell")).unwrap();
        std::fs::write(source.join("git/.gitconfig"), "git").unwrap();
        std::fs::write(source.join("shell/.config/nushell/config.nu"), "nu").unwrap();

        assert_eq!(store.import_dotfiles(&source).unwrap(), 2);
        assert_eq!(
            std::fs::read_to_string(dotfiles.join("git/.gitconfig")).unwrap(),
            "git"
        );
        assert_eq!(
            ToolsManifest::read(&manifest).unwrap().dotfiles.packages,
            ["git", "shell"]
        );
        assert!(source.join("git/.gitconfig").is_file());
    }
}
