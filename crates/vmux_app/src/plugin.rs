use bevy_app::{App, Plugin};

use crate::VmuxPluginOptions;

pub struct VmuxPluginBuilder {
    options: VmuxPluginOptions,
}

impl VmuxPluginBuilder {
    #[cfg(feature = "desktop")]
    pub const fn desktop(mut self) -> Self {
        self.options = VmuxPluginOptions::desktop();
        self
    }

    #[cfg(feature = "mobile")]
    pub const fn mobile(mut self) -> Self {
        self.options = VmuxPluginOptions::mobile();
        self
    }

    pub const fn options(mut self, options: VmuxPluginOptions) -> Self {
        self.options = options;
        self
    }

    #[cfg(feature = "core")]
    pub const fn core(mut self, enabled: bool) -> Self {
        self.options = self.options.core(enabled);
        self
    }

    #[cfg(feature = "layout")]
    pub const fn layout(mut self, enabled: bool) -> Self {
        self.options = self.options.layout(enabled);
        self
    }

    #[cfg(feature = "terminal")]
    pub const fn terminal(mut self, enabled: bool) -> Self {
        self.options = self.options.terminal(enabled);
        self
    }

    #[cfg(feature = "editor")]
    pub const fn editor(mut self, enabled: bool) -> Self {
        self.options = self.options.editor(enabled);
        self
    }

    #[cfg(feature = "git")]
    pub const fn git(mut self, enabled: bool) -> Self {
        self.options = self.options.git(enabled);
        self
    }

    #[cfg(feature = "agent")]
    pub const fn agent(mut self, enabled: bool) -> Self {
        self.options = self.options.agent(enabled);
        self
    }

    #[cfg(feature = "knowledge")]
    pub const fn knowledge(mut self, enabled: bool) -> Self {
        self.options = self.options.knowledge(enabled);
        self
    }

    #[cfg(feature = "history")]
    pub const fn history(mut self, enabled: bool) -> Self {
        self.options = self.options.history(enabled);
        self
    }

    #[cfg(feature = "simulator")]
    pub const fn simulator(mut self, enabled: bool) -> Self {
        self.options = self.options.simulator(enabled);
        self
    }

    #[cfg(feature = "shortcut")]
    pub const fn shortcut(mut self, enabled: bool) -> Self {
        self.options = self.options.shortcut(enabled);
        self
    }

    #[cfg(feature = "team")]
    pub const fn team(mut self, enabled: bool) -> Self {
        self.options = self.options.team(enabled);
        self
    }

    #[cfg(feature = "space")]
    pub const fn space(mut self, enabled: bool) -> Self {
        self.options = self.options.space(enabled);
        self
    }

    #[cfg(feature = "service")]
    pub const fn service(mut self, enabled: bool) -> Self {
        self.options = self.options.service(enabled);
        self
    }

    #[cfg(feature = "start")]
    pub const fn start(mut self, enabled: bool) -> Self {
        self.options = self.options.start(enabled);
        self
    }

    #[cfg(feature = "browser")]
    pub const fn browser(mut self, enabled: bool) -> Self {
        self.options = self.options.browser(enabled);
        self
    }

    #[cfg(feature = "mobile")]
    pub const fn mobile_pages(mut self, enabled: bool) -> Self {
        self.options = self.options.mobile_pages(enabled);
        self
    }

    pub const fn build(self) -> VmuxPlugin {
        VmuxPlugin {
            options: self.options,
        }
    }
}

impl Default for VmuxPluginBuilder {
    fn default() -> Self {
        VmuxPlugin::builder()
    }
}

pub struct VmuxPlugin {
    options: VmuxPluginOptions,
}

impl VmuxPlugin {
    pub const fn builder() -> VmuxPluginBuilder {
        VmuxPluginBuilder {
            options: VmuxPluginOptions::none(),
        }
    }
}

impl Plugin for VmuxPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(any(
            feature = "core",
            feature = "layout",
            feature = "terminal",
            feature = "editor",
            feature = "git",
            feature = "agent",
            feature = "knowledge",
            feature = "history",
            feature = "simulator",
            feature = "shortcut",
            feature = "team",
            feature = "space",
            feature = "service",
            feature = "start",
            feature = "browser",
            feature = "mobile",
        )))]
        let _ = (&self.options, app);

        #[cfg(feature = "core")]
        if self.options.core {
            app.add_plugins(VmuxCorePlugin);
        }

        #[cfg(feature = "layout")]
        if self.options.layout {
            app.add_plugins(vmux_layout::LayoutPlugin);
        }

        #[cfg(feature = "core")]
        if self.options.core {
            app.add_plugins(vmux_core::input::KeyStrokePlugin);
        }

        #[cfg(feature = "terminal")]
        if self.options.terminal {
            app.add_plugins(vmux_terminal::TerminalPlugin);
        }

        #[cfg(feature = "editor")]
        if self.options.editor {
            app.add_plugins(vmux_editor::EditorPlugin);
        }

        #[cfg(feature = "git")]
        if self.options.git {
            app.add_plugins(vmux_git::GitPlugin);
        }

        #[cfg(feature = "agent")]
        if self.options.agent {
            app.add_plugins(vmux_agent::AgentPlugin);
        }

        #[cfg(feature = "knowledge")]
        if self.options.knowledge {
            app.add_plugins(vmux_knowledge::KnowledgePlugin);
        }

        #[cfg(feature = "history")]
        if self.options.history {
            app.add_plugins(vmux_history::HistoryPlugin);
        }

        #[cfg(feature = "simulator")]
        if self.options.simulator {
            app.add_plugins(vmux_simulator::SimulatorPlugin);
        }

        #[cfg(feature = "shortcut")]
        if self.options.shortcut {
            app.add_plugins(vmux_shortcut::ShortcutPlugin);
        }

        #[cfg(feature = "team")]
        if self.options.team {
            app.add_plugins(vmux_team::TeamPlugin);
        }

        #[cfg(feature = "space")]
        if self.options.space {
            app.add_plugins(vmux_space::SpacePlugin);
        }

        #[cfg(feature = "service")]
        if self.options.service {
            app.add_plugins(vmux_service::plugin::ServicePlugin);
        }

        #[cfg(feature = "start")]
        if self.options.start {
            app.add_plugins(vmux_start::StartPlugin);
        }

        #[cfg(feature = "browser")]
        if self.options.browser {
            app.add_plugins(vmux_browser::BrowserPlugin);
        }

        #[cfg(feature = "mobile")]
        if self.options.mobile {
            app.add_plugins(crate::VmuxMobilePlugin);
        }
    }
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
