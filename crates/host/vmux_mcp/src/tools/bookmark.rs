use super::{DispatchTarget, ToolCall, ToolManifest};
use bevy_app::{App, Plugin};
use serde::Deserialize;
use vmux_client::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};

pub(super) struct BookmarkToolsPlugin;

impl Plugin for BookmarkToolsPlugin {
    fn build(&self, app: &mut App) {
        let mut tools = ToolManifest::from_ron(include_str!("bookmark.ron"));
        tools.local(app, "bookmark_list", list);
        tools.local(app, "bookmark_add", add);
        tools.local(app, "bookmark_remove", remove);
        tools.local(app, "bookmark_pin", pin);
        tools.local(app, "bookmark_unpin", unpin);
        tools.local(app, "bookmark_folder_create", create_folder);
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

pub(super) fn list(_call: &ToolCall) -> Result<DispatchTarget, String> {
    Ok(DispatchTarget::Query(AgentQuery::BookmarkList))
}

pub(super) fn add(call: &ToolCall) -> Result<DispatchTarget, String> {
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

pub(super) fn remove(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: UuidArgs = call.parse("bookmark_remove")?;
    let uuid = RequiredText::get(args.uuid, "bookmark_remove.uuid is required")?;
    Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
        AgentBookmarkCommand::Remove { uuid },
    )))
}

pub(super) fn pin(call: &ToolCall) -> Result<DispatchTarget, String> {
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

pub(super) fn unpin(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: UuidArgs = call.parse("bookmark_unpin")?;
    let uuid = RequiredText::get(args.uuid, "bookmark_unpin.uuid is required")?;
    Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
        AgentBookmarkCommand::Unpin { uuid },
    )))
}

pub(super) fn create_folder(call: &ToolCall) -> Result<DispatchTarget, String> {
    let args: FolderArgs = call.parse("bookmark_folder_create")?;
    let name = RequiredText::get(args.name, "bookmark_folder_create.name is required")?;
    Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
        AgentBookmarkCommand::CreateFolder { name },
    )))
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
