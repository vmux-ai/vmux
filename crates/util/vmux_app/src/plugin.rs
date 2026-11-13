#[cfg(feature = "ecs")]
use bevy_app::{App, Plugin};
use vmux_macro::app_plugin;

#[app_plugin]
pub enum VmuxPlugin {
    #[plugin(feature = "ecs", desktop, daemon, cli, mcp)]
    Ecs(VmuxEcsPlugin),
    #[plugin(feature = "cli", cli, mcp, requires(ecs))]
    CliTools(crate::VmuxToolPlugin),
    #[plugin(feature = "daemon", daemon, requires(ecs))]
    ServiceDaemon(
        vmux_service::server::ServiceDaemonPlugin,
        vmux_acp::AcpServicePlugin,
    ),
    #[plugin(feature = "project", desktop, requires(ecs))]
    Project(vmux_project::ProjectPlugin),
    #[plugin(feature = "extension-core", desktop, requires(ecs))]
    ExtensionCore(vmux_extension::ExtensionPlugin),
    #[plugin(feature = "jobs", desktop, requires(ecs))]
    Jobs(vmux_ecs::JobsPlugin),
    #[plugin(feature = "flex", desktop, requires(ecs))]
    Flex(vmux_flex::FlexPlugin),
    #[plugin(feature = "pane", desktop, requires(ecs))]
    Pane(vmux_pane::PanePlugin),
    #[plugin(feature = "runtime", desktop, requires(ecs, flex))]
    Runtime(VmuxRuntimePlugin),
    #[plugin(feature = "command", desktop, requires(runtime))]
    Command(vmux_command::CommandPlugin),
    #[plugin(feature = "setting", desktop, requires(runtime))]
    Setting(vmux_setting::SettingsPlugin),
    #[plugin(feature = "session", desktop, mobile)]
    Session(vmux_session::SessionPlugin),
    #[plugin(feature = "layout", desktop, requires(command, setting, pane))]
    Layout(vmux_layout::LayoutPlugin),
    #[plugin(feature = "bookmark", desktop, requires(layout))]
    Bookmark(vmux_bookmark::BookmarkPlugin),
    #[plugin(feature = "capture", desktop, requires(tool))]
    Input(vmux_input::InputPlugin),
    #[plugin(feature = "vault", desktop, requires(layout))]
    Vault(vmux_vault::VaultPlugin),
    #[plugin(feature = "terminal", desktop, requires(layout, service))]
    Terminal(vmux_terminal::TerminalPlugin),
    #[plugin(feature = "editor", desktop, requires(layout, project))]
    Editor(vmux_editor::EditorPlugin),
    #[plugin(feature = "extension", desktop, requires(browser, extension_core))]
    Extension(vmux_extension::ExtensionHostPlugin),
    #[plugin(feature = "git", desktop, requires(layout))]
    Git(vmux_git::GitPlugin),
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
    #[plugin(feature = "container", desktop, requires(layout))]
    Container(vmux_container::ContainerPlugin),
    #[plugin(feature = "simulator", desktop, requires(layout))]
    Simulator(vmux_simulator::SimulatorPlugin),
    #[plugin(feature = "shortcut", desktop, requires(runtime))]
    Shortcut(vmux_shortcut::ShortcutPlugin),
    #[plugin(feature = "team", desktop, requires(runtime))]
    Team(vmux_team::TeamPlugin),
    #[plugin(feature = "space", desktop, requires(layout))]
    Space(vmux_space::SpacePlugin),
    #[plugin(feature = "service", desktop, requires(runtime))]
    Service(vmux_service::plugin::ServicePlugin),
    #[plugin(feature = "remote", desktop, requires(layout, service))]
    Remote(vmux_remote::RemotePlugin),
    #[plugin(feature = "start", desktop, requires(runtime))]
    Start(vmux_start::StartPlugin),
    #[plugin(feature = "tool", desktop, requires(layout))]
    Tool(vmux_tool::ToolPlugin),
    #[plugin(feature = "browser", desktop, requires(layout))]
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

#[cfg(all(test, feature = "daemon"))]
mod profile_tests {
    use super::*;
    use bevy_ecs::prelude::*;
    use vmux_ecs::manifest::FeatureManifest;
    use vmux_ecs::{
        ActivateRequest, LastActivatedAt, Operation, OperationCompleted, OperationState,
    };

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
                    ["container", "mcp", "remote.pair", "service", "tool"]
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
