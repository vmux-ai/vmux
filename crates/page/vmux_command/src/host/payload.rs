use crate::command::AppCommand;
use crate::event::{CommandBarOpenEvent, OpenId};
use crate::open_target::OpenTarget;
use crate::snapshot::{CommandBarPagesSnapshot, CommandBarSpacesSnapshot, Contributions};
use bevy::prelude::default;
use vmux_ui::i18n::{Locale, TranslationValue};
use vmux_wire::command_bar::{
    CommandBarCommandEntry, CommandBarPage, CommandBarSpace, CommandBarTab, SearchEngine,
};

pub struct CommandBarEntry {
    pub id: String,
    pub name: String,
    pub shortcut: String,
}

#[allow(clippy::too_many_arguments)]
pub fn build_command_bar_open_payload(
    open_id: OpenId,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces_snapshot: &CommandBarSpacesSnapshot,
    contributions: &Contributions,
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &crate::snapshot::CommandBarWorkSnapshot,
    locale: &Locale,
    active_stack_count: usize,
    tabs: Vec<CommandBarTab>,
    target: Option<OpenTarget>,
) -> CommandBarOpenEvent {
    let mut contributed = Vec::new();
    for command in contributions.commands() {
        let args: Vec<(&str, TranslationValue<'_>)> = command
            .args
            .iter()
            .map(|(name, value)| (name.as_str(), TranslationValue::String(value)))
            .collect();
        contributed.push(CommandBarEntry {
            id: command.id.clone(),
            name: locale.translate_with(&command.message_id, &args),
            shortcut: String::new(),
        });
    }
    let mut pages = Vec::with_capacity(pages_snapshot.pages.len());
    let mut superseded = Vec::new();
    for entry in &pages_snapshot.pages {
        let mut page = entry.page.clone();
        if let Some(message_id) = entry.title_message_id {
            page.title = locale.translate(message_id);
        }
        if let Some(command_id) = entry.replaces_command {
            page.shortcut = command_shortcut(command_id);
            superseded.push(command_id);
        }
        pages.push(page);
    }
    for entry in contributions.pages() {
        pages.push(entry.page.clone());
    }
    let commands: Vec<CommandBarCommandEntry> = command_list(locale, contributed, &superseded)
        .into_iter()
        .map(|e| CommandBarCommandEntry {
            id: e.id,
            name: e.name,
            shortcut: e.shortcut,
        })
        .collect();
    let spaces = spaces_snapshot
        .spaces
        .iter()
        .map(|s| {
            let is_active = s.id == spaces_snapshot.active_space_id;
            CommandBarSpace {
                id: s.id.clone(),
                name: s.name.clone(),
                profile: s.profile.clone(),
                is_active,
                tab_count: if is_active {
                    active_stack_count as u32
                } else {
                    0
                },
            }
        })
        .collect();
    command_bar_open_payload(
        open_id,
        native_windowed,
        space_name,
        url,
        spaces,
        tabs,
        commands,
        target,
        pages,
        work_snapshot.work_dirs.clone(),
        work_snapshot.recent_files.clone(),
        work_snapshot.search_engines.clone(),
    )
}

pub fn command_list(
    locale: &Locale,
    contributed: Vec<CommandBarEntry>,
    superseded: &[&str],
) -> Vec<CommandBarEntry> {
    let mut entries = Vec::new();
    for (id, name, shortcut) in AppCommand::command_bar_entries() {
        if superseded.contains(&id) {
            continue;
        }
        entries.push(CommandBarEntry {
            id: id.to_string(),
            name: localized_command_name(locale.as_str(), id, name),
            shortcut: shortcut.to_string(),
        });
    }
    entries.extend(contributed);
    entries
}

pub fn localized_command_name(locale: &str, id: &str, fallback: String) -> String {
    let locale = Locale::from(locale);
    let message_id = format!("command-{}", id.replace('_', "-"));
    let translated = locale.translate(&message_id);
    if translated == message_id {
        return fallback;
    }
    let Some((root_id, group_id)) = command_hierarchy_ids(id) else {
        return translated;
    };
    let mut segments = translated
        .split(" > ")
        .map(str::to_string)
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return translated;
    }
    segments[0] = locale.translate(root_id);
    if let Some(group_id) = group_id
        && segments.len() > 2
    {
        segments[1] = locale.translate(group_id);
    }
    segments.join(" > ")
}

pub(crate) fn command_shortcut(id: &str) -> String {
    AppCommand::command_bar_entries()
        .into_iter()
        .find(|(entry_id, _, _)| *entry_id == id)
        .map(|(_, _, shortcut)| shortcut.to_string())
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub fn command_bar_open_payload(
    open_id: OpenId,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces: Vec<CommandBarSpace>,
    tabs: Vec<CommandBarTab>,
    commands: Vec<CommandBarCommandEntry>,
    target: Option<crate::open_target::OpenTarget>,
    pages: Vec<CommandBarPage>,
    work_dirs: Vec<crate::event::CommandBarWorkDir>,
    recent_files: Vec<crate::event::CommandBarRecentFile>,
    search_engines: Vec<SearchEngine>,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        native_windowed,
        url,
        space_name,
        spaces,
        tabs,
        commands,
        pages,
        work_dirs,
        recent_files,
        search_engines,
        prompt_context: default(),
        agent_models: Vec::new(),
        target,
        space_switch: false,
    }
}

pub(crate) fn command_hierarchy_ids(id: &str) -> Option<(&'static str, Option<&'static str>)> {
    if id == "minimize_window" {
        Some(("menu-layout", Some("command-group-window")))
    } else if id == "toggle_layout" {
        Some(("menu-layout", Some("menu-layout")))
    } else if matches!(
        id,
        "close_tab" | "new_task" | "next_tab" | "prev_tab" | "rename_tab"
    ) || id.starts_with("tab_select_")
    {
        Some(("menu-layout", Some("command-group-tab")))
    } else if id.starts_with("open_in_") {
        Some(("menu-browser", Some("command-group-open")))
    } else if id.contains("pane") {
        Some(("menu-layout", Some("command-group-pane")))
    } else if id.starts_with("stack_") {
        Some(("menu-layout", Some("command-group-stack")))
    } else if id == "space_open" {
        Some(("menu-layout", Some("command-group-space")))
    } else if id.starts_with("terminal_") {
        Some(("menu-terminal", None))
    } else if matches!(
        id,
        "browser_prev_page" | "browser_next_page" | "browser_reload" | "browser_hard_reload"
    ) {
        Some(("menu-browser", Some("command-group-navigation")))
    } else if matches!(
        id,
        "browser_zoom_in" | "browser_zoom_out" | "browser_zoom_reset" | "browser_dev_tools"
    ) {
        Some(("menu-browser", Some("command-group-view")))
    } else if id.starts_with("browser_open_") {
        Some(("menu-browser", Some("command-group-bar")))
    } else if id == "service_open" {
        Some(("menu-service", None))
    } else if id.starts_with("bookmark_") {
        Some(("menu-bookmark", None))
    } else {
        None
    }
}
