#[cfg(feature = "core")]
use bevy_app::{App, Plugin};
use vmux_macro::app_plugin;

#[app_plugin]
pub enum VmuxPlugin {
    #[plugin(feature = "core", desktop)]
    Core(VmuxCorePlugin),
    #[plugin(feature = "layout", desktop, requires(core))]
    Layout(vmux_layout::LayoutPlugin),
    #[plugin(feature = "core", option = core, desktop)]
    KeyStroke(vmux_core::input::KeyStrokePlugin),
    #[plugin(feature = "terminal", desktop, requires(layout, service))]
    Terminal(vmux_terminal::TerminalPlugin),
    #[plugin(feature = "editor", desktop, requires(layout))]
    Editor(vmux_editor::EditorPlugin),
    #[plugin(feature = "git", desktop, requires(layout))]
    Git(vmux_git::GitPlugin),
    #[plugin(
        feature = "agent",
        desktop,
        requires(editor, history, knowledge, service, space, terminal)
    )]
    Agent(vmux_agent::AgentPlugin),
    #[plugin(feature = "knowledge", desktop, requires(core))]
    Knowledge(vmux_knowledge::KnowledgePlugin),
    #[plugin(feature = "history", desktop, requires(core))]
    History(vmux_history::HistoryPlugin),
    #[plugin(feature = "simulator", desktop, requires(layout))]
    Simulator(vmux_simulator::SimulatorPlugin),
    #[plugin(feature = "shortcut", desktop, requires(core))]
    Shortcut(vmux_shortcut::ShortcutPlugin),
    #[plugin(feature = "team", desktop, requires(core))]
    Team(vmux_team::TeamPlugin),
    #[plugin(feature = "space", desktop, requires(layout))]
    Space(vmux_space::SpacePlugin),
    #[plugin(feature = "service", desktop, requires(core))]
    Service(vmux_service::plugin::ServicePlugin),
    #[plugin(feature = "start", desktop, requires(core))]
    Start(vmux_start::StartPlugin),
    #[plugin(feature = "browser", desktop, requires(layout))]
    Browser(vmux_browser::BrowserPlugin),
    #[plugin(feature = "mobile", mobile)]
    MobilePages(crate::VmuxMobilePlugin),
}

#[cfg(feature = "core")]
pub struct VmuxCorePlugin;

#[cfg(feature = "core")]
impl Plugin for VmuxCorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            vmux_flex::FlexPlugin,
            vmux_core::CorePlugin,
            vmux_core::page::PagePlugin,
            vmux_command::CommandPlugin,
            vmux_setting::SettingsPlugin,
        ));
    }
}

#[cfg(all(test, any(feature = "layout", feature = "agent")))]
mod tests {
    use super::*;

    #[cfg(feature = "layout")]
    #[test]
    fn enabling_layout_enables_core() {
        let options = VmuxPluginOptions::none().layout(true);

        assert!(options.layout);
        assert!(options.core);
    }

    #[cfg(feature = "layout")]
    #[test]
    fn disabling_core_disables_layout() {
        let options = VmuxPluginOptions::none().layout(true).core(false);

        assert!(!options.core);
        assert!(!options.layout);
    }

    #[cfg(feature = "agent")]
    #[test]
    fn enabling_agent_enables_transitive_dependencies() {
        let options = VmuxPluginOptions::none().agent(true);

        assert!(options.agent);
        assert!(options.core);
        assert!(options.layout);
        assert!(options.editor);
        assert!(options.history);
        assert!(options.knowledge);
        assert!(options.service);
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
        assert!(options.core);
        assert!(options.history);
        assert!(options.knowledge);
        assert!(options.service);
    }
}
