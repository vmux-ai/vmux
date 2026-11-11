#[cfg(feature = "ecs")]
use bevy_app::{App, Plugin};
use vmux_macro::app_plugin;

#[app_plugin]
pub enum VmuxPlugin {
    #[plugin(feature = "ecs", desktop)]
    Ecs(VmuxEcsPlugin),
    #[plugin(feature = "project", desktop, requires(ecs))]
    Project(vmux_project::ProjectPlugin),
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
    #[plugin(feature = "extension", desktop, requires(browser))]
    Extension(vmux_extension::ExtensionPlugin),
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
