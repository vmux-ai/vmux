use super::{DispatchTarget, ToolCall, ToolManifest};
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::{Commands, On};
use serde::Deserialize;
use vmux_client::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};

pub(super) struct BookmarkToolsPlugin;

impl Plugin for BookmarkToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("bookmark.ron"));
        tools.observe(app, "bookmark_list", list);
        tools.observe(app, "bookmark_add", add);
        tools.observe(app, "bookmark_remove", remove);
        tools.observe(app, "bookmark_pin", pin);
        tools.observe(app, "bookmark_unpin", unpin);
        tools.observe(app, "bookmark_folder_create", create_folder);
        tools.finish();
    }
}

#[derive(Deserialize)]
struct PageArgs {
    url: Option<String>,
    title: Option<String>,
    favicon_url: Option<String>,
    folder: Option<String>,
}

#[derive(Deserialize)]
struct UuidArgs {
    uuid: Option<String>,
}

#[derive(Deserialize)]
struct PinArgs {
    uuid: Option<String>,
    url: Option<String>,
    title: Option<String>,
    favicon_url: Option<String>,
}

#[derive(Deserialize)]
struct FolderArgs {
    name: Option<String>,
}

fn list(trigger: On<ToolCall>, mut commands: Commands) {
    trigger.finish_dispatch(
        &mut commands,
        Ok(DispatchTarget::Query(AgentQuery::BookmarkList)),
    );
}

fn add(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: PageArgs = call.parse("bookmark_add")?;
        let url = RequiredText::get(args.url, "bookmark_add.url is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::Add {
                page: AgentBookmarkPage {
                    url,
                    title: args.title,
                    favicon_url: args.favicon_url,
                },
                folder: args.folder,
            },
        )))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn remove(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: UuidArgs = call.parse("bookmark_remove")?;
        let uuid = RequiredText::get(args.uuid, "bookmark_remove.uuid is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::Remove { uuid },
        )))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn pin(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: PinArgs = call.parse("bookmark_pin")?;
        if let Some(uuid) = RequiredText::optional(args.uuid) {
            return Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
                AgentBookmarkCommand::Pin { uuid },
            )));
        }
        let url = RequiredText::get(args.url, "bookmark_pin requires uuid or url")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::PinUrl {
                page: AgentBookmarkPage {
                    url,
                    title: args.title,
                    favicon_url: args.favicon_url,
                },
            },
        )))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn unpin(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: UuidArgs = call.parse("bookmark_unpin")?;
        let uuid = RequiredText::get(args.uuid, "bookmark_unpin.uuid is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::Unpin { uuid },
        )))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

fn create_folder(trigger: On<ToolCall>, mut commands: Commands) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: FolderArgs = call.parse("bookmark_folder_create")?;
        let name = RequiredText::get(args.name, "bookmark_folder_create.name is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::CreateFolder { name },
        )))
    }

    trigger.finish_dispatch(&mut commands, target(&trigger));
}

struct RequiredText;

impl RequiredText {
    fn optional(value: Option<String>) -> Option<String> {
        value.filter(|value| !value.trim().is_empty())
    }

    fn get(value: Option<String>, error: &str) -> Result<String, String> {
        Self::optional(value).ok_or_else(|| error.to_string())
    }
}
