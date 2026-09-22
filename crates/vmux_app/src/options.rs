#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VmuxPluginOptions {
    #[cfg(feature = "core")]
    pub(crate) core: bool,
    #[cfg(feature = "layout")]
    pub(crate) layout: bool,
    #[cfg(feature = "terminal")]
    pub(crate) terminal: bool,
    #[cfg(feature = "editor")]
    pub(crate) editor: bool,
    #[cfg(feature = "git")]
    pub(crate) git: bool,
    #[cfg(feature = "agent")]
    pub(crate) agent: bool,
    #[cfg(feature = "knowledge")]
    pub(crate) knowledge: bool,
    #[cfg(feature = "history")]
    pub(crate) history: bool,
    #[cfg(feature = "simulator")]
    pub(crate) simulator: bool,
    #[cfg(feature = "shortcut")]
    pub(crate) shortcut: bool,
    #[cfg(feature = "team")]
    pub(crate) team: bool,
    #[cfg(feature = "space")]
    pub(crate) space: bool,
    #[cfg(feature = "service")]
    pub(crate) service: bool,
    #[cfg(feature = "start")]
    pub(crate) start: bool,
    #[cfg(feature = "browser")]
    pub(crate) browser: bool,
    #[cfg(feature = "mobile")]
    pub(crate) mobile: bool,
}

impl VmuxPluginOptions {
    pub const fn none() -> Self {
        Self {
            #[cfg(feature = "core")]
            core: false,
            #[cfg(feature = "layout")]
            layout: false,
            #[cfg(feature = "terminal")]
            terminal: false,
            #[cfg(feature = "editor")]
            editor: false,
            #[cfg(feature = "git")]
            git: false,
            #[cfg(feature = "agent")]
            agent: false,
            #[cfg(feature = "knowledge")]
            knowledge: false,
            #[cfg(feature = "history")]
            history: false,
            #[cfg(feature = "simulator")]
            simulator: false,
            #[cfg(feature = "shortcut")]
            shortcut: false,
            #[cfg(feature = "team")]
            team: false,
            #[cfg(feature = "space")]
            space: false,
            #[cfg(feature = "service")]
            service: false,
            #[cfg(feature = "start")]
            start: false,
            #[cfg(feature = "browser")]
            browser: false,
            #[cfg(feature = "mobile")]
            mobile: false,
        }
    }

    #[cfg(feature = "desktop")]
    pub const fn desktop() -> Self {
        Self {
            core: true,
            layout: true,
            terminal: true,
            editor: true,
            git: true,
            agent: true,
            knowledge: true,
            history: true,
            simulator: true,
            shortcut: true,
            team: true,
            space: true,
            service: true,
            start: true,
            browser: true,
            #[cfg(feature = "mobile")]
            mobile: false,
        }
    }

    #[cfg(feature = "mobile")]
    pub const fn mobile() -> Self {
        Self {
            #[cfg(feature = "core")]
            core: false,
            #[cfg(feature = "layout")]
            layout: false,
            #[cfg(feature = "terminal")]
            terminal: false,
            #[cfg(feature = "editor")]
            editor: false,
            #[cfg(feature = "git")]
            git: false,
            #[cfg(feature = "agent")]
            agent: false,
            #[cfg(feature = "knowledge")]
            knowledge: false,
            #[cfg(feature = "history")]
            history: false,
            #[cfg(feature = "simulator")]
            simulator: false,
            #[cfg(feature = "shortcut")]
            shortcut: false,
            #[cfg(feature = "team")]
            team: false,
            #[cfg(feature = "space")]
            space: false,
            #[cfg(feature = "service")]
            service: false,
            #[cfg(feature = "start")]
            start: false,
            #[cfg(feature = "browser")]
            browser: false,
            mobile: true,
        }
    }

    #[cfg(feature = "core")]
    pub const fn core(mut self, enabled: bool) -> Self {
        self.core = enabled;
        self
    }

    #[cfg(feature = "layout")]
    pub const fn layout(mut self, enabled: bool) -> Self {
        self.layout = enabled;
        self
    }

    #[cfg(feature = "terminal")]
    pub const fn terminal(mut self, enabled: bool) -> Self {
        self.terminal = enabled;
        self
    }

    #[cfg(feature = "editor")]
    pub const fn editor(mut self, enabled: bool) -> Self {
        self.editor = enabled;
        self
    }

    #[cfg(feature = "git")]
    pub const fn git(mut self, enabled: bool) -> Self {
        self.git = enabled;
        self
    }

    #[cfg(feature = "agent")]
    pub const fn agent(mut self, enabled: bool) -> Self {
        self.agent = enabled;
        self
    }

    #[cfg(feature = "knowledge")]
    pub const fn knowledge(mut self, enabled: bool) -> Self {
        self.knowledge = enabled;
        self
    }

    #[cfg(feature = "history")]
    pub const fn history(mut self, enabled: bool) -> Self {
        self.history = enabled;
        self
    }

    #[cfg(feature = "simulator")]
    pub const fn simulator(mut self, enabled: bool) -> Self {
        self.simulator = enabled;
        self
    }

    #[cfg(feature = "shortcut")]
    pub const fn shortcut(mut self, enabled: bool) -> Self {
        self.shortcut = enabled;
        self
    }

    #[cfg(feature = "team")]
    pub const fn team(mut self, enabled: bool) -> Self {
        self.team = enabled;
        self
    }

    #[cfg(feature = "space")]
    pub const fn space(mut self, enabled: bool) -> Self {
        self.space = enabled;
        self
    }

    #[cfg(feature = "service")]
    pub const fn service(mut self, enabled: bool) -> Self {
        self.service = enabled;
        self
    }

    #[cfg(feature = "start")]
    pub const fn start(mut self, enabled: bool) -> Self {
        self.start = enabled;
        self
    }

    #[cfg(feature = "browser")]
    pub const fn browser(mut self, enabled: bool) -> Self {
        self.browser = enabled;
        self
    }

    #[cfg(feature = "mobile")]
    pub const fn mobile_pages(mut self, enabled: bool) -> Self {
        self.mobile = enabled;
        self
    }
}

impl Default for VmuxPluginOptions {
    fn default() -> Self {
        Self::none()
    }
}
