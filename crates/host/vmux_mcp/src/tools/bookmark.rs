use super::{DispatchTarget, ToolCall, ToolDefinition};
use serde::Deserialize;
use vmux_client::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};

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

pub(super) fn list(_call: ToolCall<'_>) -> Result<DispatchTarget, String> {
    Ok(DispatchTarget::Query(AgentQuery::BookmarkList))
}

pub(super) fn add(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn remove(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
    let args: UuidArgs = call.parse("bookmark_remove")?;
    let uuid = RequiredText::get(args.uuid, "bookmark_remove.uuid is required")?;
    Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
        AgentBookmarkCommand::Remove { uuid },
    )))
}

pub(super) fn pin(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn unpin(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
    let args: UuidArgs = call.parse("bookmark_unpin")?;
    let uuid = RequiredText::get(args.uuid, "bookmark_unpin.uuid is required")?;
    Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
        AgentBookmarkCommand::Unpin { uuid },
    )))
}

pub(super) fn create_folder(call: ToolCall<'_>) -> Result<DispatchTarget, String> {
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

pub(super) fn bookmark_list_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_list".into(),
        description: "List all pins (favicon quick-access) and bookmarks (saved pages, \
optionally inside folders) for the current profile. Returns JSON: \
{pins:[{uuid,url,title,favicon_url}], roots:[ {kind:\"entry\",...} | \
{kind:\"folder\",uuid,name,collapsed,children:[...]} ]}."
            .into(),
        input_schema: serde_json::json!({"type":"object","properties":{},"additionalProperties":false}),
    }
}

pub(super) fn bookmark_add_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_add".into(),
        description: "Save a page as a bookmark. Optional folder (a folder uuid from \
bookmark_list) nests it; omit for top level."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["url"],
            "additionalProperties": false,
            "properties": {
                "url": {"type": "string"},
                "title": {"type": "string"},
                "favicon_url": {"type": "string"},
                "folder": {"type": "string"}
            }
        }),
    }
}

pub(super) fn bookmark_remove_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_remove".into(),
        description: "Remove a bookmark by its uuid (from bookmark_list).".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["uuid"],"additionalProperties":false,
            "properties":{"uuid":{"type":"string"}}
        }),
    }
}

pub(super) fn bookmark_pin_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_pin".into(),
        description: "Pin a page to the favicon grid. Provide a bookmark uuid to promote an \
existing bookmark, OR a url (+optional title/favicon_url) to pin a page directly."
            .into(),
        input_schema: serde_json::json!({
            "type":"object","additionalProperties":false,
            "properties":{
                "uuid":{"type":"string"},
                "url":{"type":"string"},
                "title":{"type":"string"},
                "favicon_url":{"type":"string"}
            }
        }),
    }
}

pub(super) fn bookmark_unpin_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_unpin".into(),
        description: "Unpin a pin by its uuid (from bookmark_list).".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["uuid"],"additionalProperties":false,
            "properties":{"uuid":{"type":"string"}}
        }),
    }
}

pub(super) fn bookmark_folder_create_definition() -> ToolDefinition {
    ToolDefinition {
        name: "bookmark_folder_create".into(),
        description: "Create a bookmark folder with the given name.".into(),
        input_schema: serde_json::json!({
            "type":"object","required":["name"],"additionalProperties":false,
            "properties":{"name":{"type":"string"}}
        }),
    }
}
