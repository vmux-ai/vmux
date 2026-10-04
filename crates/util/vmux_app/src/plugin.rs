#[cfg(feature = "ecs")]
use bevy_app::{App, Plugin};
use vmux_macro::app_plugin;

#[app_plugin]
pub enum VmuxPlugin {
    #[plugin(feature = "ecs", desktop)]
    Ecs(VmuxEcsPlugin),
    #[plugin(feature = "command", desktop, requires(ecs))]
    Command(vmux_command::CommandPlugin),
    #[plugin(feature = "setting", desktop, requires(ecs))]
    Setting(vmux_setting::SettingsPlugin),
    #[plugin(feature = "session", desktop)]
    Session(vmux_session::SessionPlugin),
    #[plugin(feature = "chat", desktop, mobile)]
    Chat(vmux_chat::ChatPlugin),
    #[plugin(feature = "layout", desktop, requires(command, setting))]
    Layout(vmux_layout::LayoutPlugin),
    #[plugin(feature = "bookmark", desktop, requires(layout))]
    Bookmark(vmux_bookmark::BookmarkPlugin),
    #[plugin(feature = "capture", desktop, requires(tool))]
    Input(vmux_input::InputPlugin),
    #[plugin(feature = "vault", desktop, requires(layout))]
    Vault(vmux_vault::VaultPlugin),
    #[plugin(feature = "terminal", desktop, requires(layout, service))]
    Terminal(vmux_terminal::TerminalPlugin),
    #[plugin(feature = "editor", desktop, requires(layout))]
    Editor(vmux_editor::EditorPlugin),
    #[plugin(feature = "browser", desktop, requires(layout))]
    Browser(vmux_browser::BrowserPlugin),
    #[plugin(feature = "extension", desktop, requires(browser))]
    Extension(vmux_extension::ExtensionPlugin),
    #[plugin(feature = "git", desktop, requires(layout))]
    Git(vmux_git::GitPlugin),
    #[plugin(
        feature = "agent",
        desktop,
        requires(
            chat, command, editor, history, knowledge, service, session, space, terminal
        )
    )]
    Agent(vmux_agent::AgentPlugin),
    #[plugin(feature = "knowledge", desktop, requires(ecs))]
    Knowledge(vmux_knowledge::KnowledgePlugin),
    #[plugin(feature = "history", desktop, requires(ecs))]
    History(vmux_history::HistoryPlugin),
    #[plugin(feature = "simulator", desktop, requires(layout))]
    Simulator(vmux_simulator::SimulatorPlugin),
    #[plugin(feature = "shortcut", desktop, requires(ecs))]
    Shortcut(vmux_shortcut::ShortcutPlugin),
    #[plugin(feature = "team", desktop, requires(ecs))]
    Team(vmux_team::TeamPlugin),
    #[plugin(feature = "space", desktop, requires(layout))]
    Space(vmux_space::SpacePlugin),
    #[plugin(feature = "service", desktop, requires(ecs))]
    Service(vmux_service::plugin::ServicePlugin),
    #[plugin(feature = "remote", desktop, requires(layout, service))]
    Remote(vmux_remote::RemotePlugin),
    #[plugin(feature = "start", desktop, requires(ecs))]
    Start(vmux_start::StartPlugin),
    #[plugin(feature = "tool", desktop, requires(layout))]
    Tool(vmux_tool::ToolPlugin),
    #[plugin(feature = "mobile", mobile)]
    MobilePages(crate::VmuxMobilePlugin),
}

#[cfg(feature = "ecs")]
pub struct VmuxEcsPlugin;

#[cfg(feature = "ecs")]
impl Plugin for VmuxEcsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_flex::FlexPlugin,
            vmux_ecs::EcsPlugin,
            vmux_ecs::page::PagePlugin,
        ));
    }
}

#[cfg(all(test, any(feature = "layout", feature = "agent")))]
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

    #[cfg(feature = "agent")]
    #[test]
    fn enabling_agent_enables_transitive_dependencies() {
        let options = VmuxPluginOptions::none().agent(true);

        assert!(options.agent);
        assert!(options.chat);
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

    #[cfg(feature = "agent")]
    #[test]
    fn disabling_layout_disables_transitive_dependents() {
        let options = VmuxPluginOptions::none().agent(true).layout(false);

        assert!(!options.layout);
        assert!(!options.editor);
        assert!(!options.space);
        assert!(!options.terminal);
        assert!(!options.agent);
        assert!(options.ecs);
        assert!(options.history);
        assert!(options.knowledge);
        assert!(options.service);
    }
}
