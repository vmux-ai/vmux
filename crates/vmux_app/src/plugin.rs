#[cfg(feature = "core")]
use bevy_app::{App, Plugin};
use vmux_macro::app_plugin;

#[app_plugin]
pub enum VmuxPlugin {
    #[plugin(feature = "core", desktop)]
    Core(VmuxCorePlugin),
    #[plugin(feature = "layout", desktop)]
    Layout(vmux_layout::LayoutPlugin),
    #[plugin(feature = "core", option = core, desktop)]
    KeyStroke(vmux_core::input::KeyStrokePlugin),
    #[plugin(feature = "terminal", desktop)]
    Terminal(vmux_terminal::TerminalPlugin),
    #[plugin(feature = "editor", desktop)]
    Editor(vmux_editor::EditorPlugin),
    #[plugin(feature = "git", desktop)]
    Git(vmux_git::GitPlugin),
    #[plugin(feature = "agent", desktop)]
    Agent(vmux_agent::AgentPlugin),
    #[plugin(feature = "knowledge", desktop)]
    Knowledge(vmux_knowledge::KnowledgePlugin),
    #[plugin(feature = "history", desktop)]
    History(vmux_history::HistoryPlugin),
    #[plugin(feature = "simulator", desktop)]
    Simulator(vmux_simulator::SimulatorPlugin),
    #[plugin(feature = "shortcut", desktop)]
    Shortcut(vmux_shortcut::ShortcutPlugin),
    #[plugin(feature = "team", desktop)]
    Team(vmux_team::TeamPlugin),
    #[plugin(feature = "space", desktop)]
    Space(vmux_space::SpacePlugin),
    #[plugin(feature = "service", desktop)]
    Service(vmux_service::plugin::ServicePlugin),
    #[plugin(feature = "start", desktop)]
    Start(vmux_start::StartPlugin),
    #[plugin(feature = "browser", desktop)]
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
