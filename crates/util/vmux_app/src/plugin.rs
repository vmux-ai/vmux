#[cfg(feature = "ecs")]
use bevy_app::{App, Plugin};
use vmux_macro::app_plugin;

#[app_plugin]
pub enum VmuxPlugin {
    #[plugin(feature = "ecs", desktop, daemon, cli, mcp)]
    Ecs(VmuxEcsPlugin),
    #[plugin(feature = "jobs", desktop, requires(ecs))]
    Jobs(vmux_ecs::JobsPlugin),
    #[plugin(feature = "account", desktop, requires(ecs))]
    Account(vmux_account::AccountPlugin),
    #[plugin(feature = "account-local", desktop, requires(account))]
    AccountLocal(vmux_account::local::LocalPlugin),
    #[plugin(feature = "release-flags", desktop, requires(ecs))]
    ReleaseFlags(crate::feature_flags::ReleaseFlagsPlugin),
    #[plugin(
        feature = "account-cloud",
        desktop,
        requires(account_local, release_flags)
    )]
    AccountCloud(vmux_account::cloud::AccountCloudPlugin),
    #[plugin(feature = "profile-account", desktop, requires(account_local, runtime))]
    ProfileAccount(crate::profile_account::ProfileAccountPlugin),
    #[plugin(feature = "cli", cli, mcp, requires(ecs))]
    CliTools(crate::VmuxToolPlugin),
    #[plugin(feature = "daemon", daemon, requires(ecs))]
    ServiceDaemon(crate::SessionHostRuntimePlugin),
    #[plugin(feature = "project", desktop, requires(ecs))]
    Project(vmux_project::ProjectPlugin),
    #[plugin(feature = "extension-core", desktop, requires(ecs))]
    ExtensionCore(vmux_extension::ExtensionPlugin),
    #[plugin(feature = "lsp", requires(ecs))]
    Lsp(vmux_lsp::LspPlugin),
    #[plugin(feature = "flex", desktop, requires(ecs))]
    Flex(vmux_flex::FlexPlugin),
    #[plugin(feature = "runtime", desktop, requires(ecs, flex))]
    Runtime(VmuxRuntimePlugin),
    #[plugin(
        feature = "feature-flags",
        desktop,
        requires(runtime, jobs, release_flags)
    )]
    FeatureFlags(crate::feature_flags::FeatureFlagsPlugin),
    #[plugin(feature = "command", desktop, requires(runtime))]
    Command(vmux_command::CommandPlugin),
    #[plugin(feature = "setting", desktop, requires(runtime))]
    Setting(vmux_setting::SettingsPlugin),
    #[plugin(feature = "session", desktop, mobile)]
    Session(vmux_session::SessionClientPlugin),
    #[plugin(feature = "layout", desktop, requires(ecs))]
    Layout(vmux_layout::LayoutPlugin),
    #[plugin(
        feature = "layout-runtime",
        desktop,
        requires(layout, command, setting)
    )]
    LayoutRuntime(vmux_layout::LayoutRuntimePlugin),
    #[plugin(feature = "bookmark", desktop, requires(layout_runtime))]
    Bookmark(vmux_bookmark::BookmarkPlugin),
    #[plugin(feature = "capture", desktop, requires(tool))]
    Input(vmux_input::InputPlugin),
    #[plugin(feature = "vault", desktop, requires(layout_runtime))]
    Vault(vmux_vault::VaultPlugin),
    #[plugin(feature = "terminal", desktop, requires(layout_runtime, service))]
    Terminal(vmux_terminal::TerminalPlugin),
    #[plugin(feature = "editor", desktop, requires(layout_runtime, project, lsp))]
    Editor(vmux_editor::EditorPlugin),
    #[plugin(feature = "extension", desktop, requires(browser, extension_core))]
    Extension(vmux_extension::ExtensionHostPlugin),
    #[plugin(feature = "git", desktop, requires(layout_runtime))]
    Git(vmux_git::GitPlugin),
    #[plugin(feature = "gallery", desktop, requires(layout_runtime))]
    Gallery(vmux_gallery::GalleryPlugin),
    #[plugin(
        feature = "acp",
        desktop,
        requires(command, editor, history, knowledge, service, session, space, terminal)
    )]
    Acp(vmux_acp::AcpPlugin),
    #[plugin(feature = "knowledge", desktop, requires(runtime))]
    Knowledge(vmux_knowledge::KnowledgePlugin),
    #[plugin(feature = "history", desktop, requires(runtime))]
    History(vmux_history::HistoryPlugin),
    #[plugin(feature = "container", desktop, requires(layout_runtime))]
    Container(vmux_container::ContainerPlugin),
    #[plugin(feature = "simulator", desktop, requires(layout_runtime))]
    Simulator(vmux_simulator::SimulatorPlugin),
    #[plugin(feature = "shortcut", desktop, requires(runtime))]
    Shortcut(vmux_shortcut::ShortcutPlugin),
    #[plugin(feature = "team", desktop, requires(runtime, profile_account))]
    Team(vmux_team::TeamPlugin),
    #[plugin(feature = "space", desktop, requires(layout_runtime))]
    Space(vmux_space::SpacePlugin),
    #[plugin(feature = "service", desktop, requires(runtime))]
    Service(vmux_service::plugin::ServicePlugin),
    #[plugin(feature = "remote", desktop, requires(layout_runtime, service))]
    Remote(vmux_remote::RemotePlugin),
    #[plugin(feature = "start", desktop, requires(runtime))]
    Start(vmux_start::StartPlugin),
    #[plugin(feature = "tool", desktop, requires(layout_runtime))]
    Tool(vmux_tool::ToolPlugin),
    #[plugin(feature = "browser", desktop, requires(layout_runtime))]
    Browser(vmux_browser::BrowserPlugin),
    #[plugin(feature = "mobile", mobile)]
    MobilePages(crate::VmuxMobilePlugin),
}

#[cfg(feature = "ecs")]
pub struct VmuxEcsPlugin;

#[cfg(feature = "ecs")]
impl Plugin for VmuxEcsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((vmux_ecs::PrimitivesPlugin, vmux_ecs::OperationPlugin));
    }
}

#[cfg(all(feature = "runtime", not(target_os = "ios")))]
pub struct VmuxRuntimePlugin;

#[cfg(all(feature = "runtime", not(target_os = "ios")))]
impl Plugin for VmuxRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_flex::WindowViewportPlugin,
            vmux_ecs::profile::ProfilePlugin,
            vmux_ecs::page::PagePlugin,
            vmux_ecs::wake::WakePlugin,
        ));
    }
}

#[cfg(all(test, any(feature = "layout", feature = "acp")))]
mod tests {
    use super::*;
    #[cfg(feature = "layout-runtime")]
    use bevy_ecs::prelude::{ChildOf, Name};
    #[cfg(feature = "layout-runtime")]
    use vmux_layout::{Pane, SelectRequest};

    #[cfg(feature = "layout-runtime")]
    #[test]
    fn core_selection_remains_headless_when_product_adapters_are_compiled() {
        let mut app = App::new();
        app.add_plugins(VmuxPlugin::builder().layout(true).build());
        let root = app
            .world_mut()
            .spawn(Name::new("Independent application"))
            .id();
        let pane = app.world_mut().spawn((Pane, ChildOf(root))).id();
        app.world_mut().write_message(SelectRequest { root, pane });
        app.update();
        assert!(app.world().get::<vmux_ecs::LastActivatedAt>(pane).is_some());
        assert!(!app.is_plugin_added::<vmux_layout::LayoutRuntimePlugin>());
    }

    #[cfg(feature = "layout")]
    #[test]
    fn enabling_layout_enables_ecs() {
        let options = VmuxPluginOptions::none().layout(true);

        assert!(options.layout);
        assert!(options.ecs);
    }

    #[cfg(feature = "layout")]
    #[test]
    fn disabling_ecs_disables_layout() {
        let options = VmuxPluginOptions::none().layout(true).ecs(false);

        assert!(!options.ecs);
        assert!(!options.layout);
    }

    #[cfg(feature = "acp")]
    #[test]
    fn enabling_acp_enables_transitive_dependencies() {
        let options = VmuxPluginOptions::none().acp(true);

        assert!(options.acp);
        assert!(options.command);
        assert!(options.ecs);
        assert!(options.layout);
        assert!(options.editor);
        assert!(options.history);
        assert!(options.knowledge);
        assert!(options.service);
        assert!(options.session);
        assert!(options.space);
        assert!(options.terminal);
    }

    #[cfg(feature = "acp")]
    #[test]
    fn disabling_layout_disables_transitive_dependents() {
        let options = VmuxPluginOptions::none().acp(true).layout(false);

        assert!(!options.layout);
        assert!(!options.editor);
        assert!(!options.space);
        assert!(!options.terminal);
        assert!(!options.acp);
        assert!(options.ecs);
        assert!(options.history);
        assert!(options.knowledge);
        assert!(options.service);
    }
}

#[cfg(all(test, feature = "account-local", feature = "jobs"))]
mod composition_tests {
    use super::*;
    use bevy_ecs::prelude::{Name, With};
    use std::time::{Duration, Instant};

    #[test]
    fn shared_jobs_compose_with_account_local_and_restore_profiles() {
        for shared_jobs in [true, false] {
            let data = tempfile::tempdir().unwrap();
            let directory = data.path().join("profiles/personal");
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("display_name"), "Restored profile").unwrap();
            let mut app = App::new();
            app.add_plugins(
                VmuxPlugin::builder()
                    .account_local(true)
                    .jobs(shared_jobs)
                    .build(),
            );
            let root = app
                .world_mut()
                .spawn(vmux_account::Root::at(data.path(), "personal"))
                .id();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                app.update();
                if app.world().get::<vmux_ecs::Ready>(root).is_some() {
                    break;
                }
                if let Some(error) = app.world().get::<vmux_account::Error>(root) {
                    panic!("account load failed: {:?}", error.0);
                }
                assert!(Instant::now() < deadline, "account load timed out");
                std::thread::sleep(Duration::from_millis(2));
            }
            let world = app.world_mut();
            let (id, name) = world
                .query_filtered::<(&vmux_ecs::ProfileId, &Name), With<vmux_account::Profile>>()
                .single(world)
                .unwrap();
            assert_eq!(id.0, "personal");
            assert_eq!(name.as_str(), "Restored profile");
        }
    }
}

#[cfg(all(test, feature = "daemon"))]
mod profile_tests {
    use super::*;
    use bevy_app::Update;
    use bevy_ecs::prelude::*;
    use vmux_ecs::{
        ActivateRequest, LastActivatedAt, Operation, OperationCompleted, OperationState,
    };
    use vmux_feature::manifest::FeatureManifest;

    #[test]
    fn headless_profiles_preserve_core_lifecycles_and_select_their_cli_catalog() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _entered = runtime.enter();
        let profiles = [
            (VmuxPluginOptions::daemon(), false),
            #[cfg(feature = "cli")]
            (VmuxPluginOptions::cli(), true),
            #[cfg(feature = "mcp")]
            (VmuxPluginOptions::mcp(), true),
        ];
        for (options, tools) in profiles {
            let mut app = App::new();
            app.add_plugins(VmuxPlugin::builder().options(options).build());
            app.finish();
            app.cleanup();
            if !tools {
                app.world_mut()
                    .write_message(vmux_session::CreateRequest::new(
                        "Background task",
                        "Renderer-free mutation",
                        std::path::PathBuf::from("/tmp/headless-session"),
                        None,
                    ));
                app.world_mut().run_schedule(Update);
                let world = app.world_mut();
                let (id, name) = world
                    .query_filtered::<(&vmux_session::SessionId, &Name), With<vmux_session::Session>>()
                    .single(world)
                    .unwrap();
                assert_eq!(name.as_str(), "Background task");
                let id = id.clone();
                app.world_mut().write_message(vmux_session::RenameRequest {
                    id: id.clone(),
                    name: "Renamed without a page".into(),
                });
                app.world_mut().run_schedule(Update);
                let world = app.world_mut();
                let (current, name) = world
                    .query_filtered::<(&vmux_session::SessionId, &Name), With<vmux_session::Session>>()
                    .single(world)
                    .unwrap();
                assert_eq!(current, &id);
                assert_eq!(name.as_str(), "Renamed without a page");
            }
            #[cfg(feature = "runtime")]
            assert!(!app.is_plugin_added::<VmuxRuntimePlugin>());
            let mut catalog = Vec::new();
            let world = app.world_mut();
            for feature in world.query::<&FeatureManifest>().iter(world) {
                if let Some(cli) = &feature.cli {
                    for command in &cli.commands {
                        catalog.push(command.id.clone());
                    }
                }
            }
            catalog.sort();
            if tools {
                assert_eq!(
                    catalog,
                    [
                        "container",
                        "mcp",
                        "mcp.proxy",
                        "remote.pair",
                        "service",
                        "tool"
                    ]
                );
            } else {
                assert!(catalog.is_empty());
            }
            let root = app.world_mut().spawn(LastActivatedAt(10)).id();
            let child = app
                .world_mut()
                .spawn((LastActivatedAt(10), ChildOf(root)))
                .id();
            app.world_mut().trigger(ActivateRequest { entity: child });
            app.world_mut().flush();
            let root_time = app.world().get::<LastActivatedAt>(root).unwrap().0;
            assert!(root_time > 10);
            assert_eq!(
                app.world().get::<LastActivatedAt>(child).unwrap().0,
                root_time
            );
            let operation = app
                .world_mut()
                .spawn(Operation::bundle("profile.work").unwrap())
                .id();
            app.world_mut().trigger(OperationCompleted {
                entity: operation,
                result: Ok(()),
            });
            assert_eq!(
                app.world().get::<OperationState>(operation),
                Some(&OperationState::Succeeded)
            );
            app.world_mut().trigger(OperationCompleted {
                entity: operation,
                result: Err("late completion".into()),
            });
            assert_eq!(
                app.world().get::<OperationState>(operation),
                Some(&OperationState::Succeeded)
            );
        }
    }
}

#[cfg(all(test, feature = "account-cloud", feature = "jobs"))]
mod account_tests {
    use super::*;
    use bevy_app::Update;
    use bevy_ecs::prelude::*;
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};
    use vmux_feature::snapshot::Document;

    #[test]
    fn cloud_composition_revokes_availability_without_an_accepted_release_snapshot() {
        let mut app = App::new();
        app.add_plugins(VmuxPlugin::builder().jobs(true).account_cloud(true).build());
        app.finish();
        app.cleanup();
        app.world_mut()
            .spawn(vmux_feature::Provider::bundle("production"));
        let root = app
            .world_mut()
            .spawn((vmux_account::Root, vmux_account::CloudAvailable))
            .id();
        app.world_mut().run_schedule(Update);
        assert!(
            app.world()
                .get::<vmux_account::CloudAvailable>(root)
                .is_none()
        );
    }

    #[test]
    fn cloud_gate_keeps_its_release_source_when_administration_adds_another_provider() {
        let mut app = App::new();
        app.add_plugins(VmuxPlugin::builder().jobs(true).account_cloud(true).build());
        #[cfg(feature = "feature-flags")]
        app.add_plugins(crate::feature_flags::FeatureFlagsPlugin);
        app.finish();
        app.cleanup();
        app.world_mut().run_schedule(bevy_app::Startup);
        let world = app.world_mut();
        let release = world
            .query_filtered::<&ChildOf, With<vmux_account::AccountCloudFlag>>()
            .single(world)
            .unwrap()
            .parent();
        let environment = world.get::<Name>(release).unwrap().as_str().to_string();
        app.world_mut()
            .entity_mut(release)
            .remove::<vmux_feature::http::Endpoint>();
        let other = app
            .world_mut()
            .spawn(vmux_feature::Provider::bundle("administration-only"))
            .id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        for (index, enabled) in [true, false].into_iter().enumerate() {
            for (provider, name, decision) in [
                (release, environment.as_str(), enabled),
                (other, "administration-only", !enabled),
            ] {
                app.world_mut().write_message(vmux_feature::Receive {
                    provider,
                    generation: 0,
                    document: Document {
                        schema_version: 1,
                        environment: name.into(),
                        revision: index as u64 + 1,
                        issued_at: now,
                        valid_until: now + 600,
                        flags: BTreeMap::from([("account.cloud".into(), decision)]),
                    },
                });
            }
            app.world_mut().run_schedule(Update);
            let world = app.world_mut();
            let current = world
                .query_filtered::<&vmux_feature::Current, With<vmux_account::AccountCloudFlag>>()
                .single(world)
                .unwrap();
            assert_eq!(current.0, enabled);
        }
    }
}
