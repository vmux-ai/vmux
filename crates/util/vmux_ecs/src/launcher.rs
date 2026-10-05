use bevy::prelude::*;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandBarContribution {
    pub row: vmux_api::command_bar::CommandBarResultItem,
    pub title_message_id: String,
    pub subtitle_message_id: String,
    pub query_prefix: String,
    pub value: String,
    pub keywords: Vec<String>,
    pub rank: i32,
    pub close: bool,
    pub picker: Option<vmux_api::command_bar::CommandBarPicker>,
    pub numbered: bool,
    pub preferred: bool,
    pub pre_filtered: bool,
}

impl CommandBarContribution {
    pub fn matches(&self, query: &str) -> bool {
        if self.pre_filtered {
            return true;
        }
        let query = query.trim();
        let query = if self.query_prefix.is_empty() {
            query
        } else {
            let Some(query) = query.strip_prefix(&self.query_prefix) else {
                return false;
            };
            query.trim()
        };
        let query = query.to_ascii_lowercase();
        query.is_empty()
            || self.row.title.to_ascii_lowercase().contains(&query)
            || self.row.subtitle.to_ascii_lowercase().contains(&query)
            || self
                .keywords
                .iter()
                .any(|keyword| keyword.to_ascii_lowercase().contains(&query))
    }
}

#[derive(EntityEvent, Clone, Debug)]
pub struct CommandBarContributionActivated {
    #[event_target]
    pub target: Entity,
    pub webview: Entity,
    pub query: String,
    pub open: Option<vmux_api::open_target::OpenTarget>,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct CommandBarQueryChanged {
    #[event_target]
    pub target: Entity,
    pub open_id: vmux_api::command_bar::OpenId,
    pub query: String,
    pub start: bool,
    pub open: Option<vmux_api::open_target::OpenTarget>,
    pub picker: Option<vmux_api::command_bar::CommandBarPicker>,
    pub numbered: bool,
}

#[derive(Message, Clone, Debug)]
pub struct ContributedCommandChosen {
    pub id: String,
    pub stack: Option<Entity>,
    pub pane: Option<Entity>,
}

#[derive(Message, Clone, Copy, Debug, Default)]
pub struct LauncherDismissRequest;
#[derive(Component, Clone, Copy, Debug)]
pub struct HostsLauncher;

#[derive(Message, Clone, Copy, Debug)]
pub struct InlineTransitionRequested {
    pub stack: Entity,
    pub webview: Entity,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct RendersLauncherPanel;

#[derive(Message, Clone, Copy, Debug)]
pub struct RestoreKeyboardToStack {
    pub stack: Entity,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct StackInPaneChosen {
    pub pane_bits: u64,
    pub index: usize,
}
