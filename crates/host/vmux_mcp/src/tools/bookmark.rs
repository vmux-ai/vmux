use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};

pub(super) struct BookmarkToolsPlugin;

impl Plugin for BookmarkToolsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Bookmark))
            .add_systems(
                Update,
                (list, add, remove, pin, unpin, create_folder).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Clone, Copy, Component, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[expect(clippy::enum_variant_names)]
enum BookmarkTool {
    BookmarkList,
    BookmarkAdd,
    BookmarkRemove,
    BookmarkPin,
    BookmarkUnpin,
    BookmarkFolderCreate,
}

fn register(mut tools: ToolSpawner) {
    let manifest = ToolManifest::<BookmarkTool>::from_ron(include_str!("bookmark.ron"));
    tools.spawn_manifest(manifest);
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

fn list(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    for (request, call, _) in calls.matching(BookmarkTool::BookmarkList) {
        call.finish_dispatch(
            request,
            &mut commands,
            Ok(DispatchTarget::Query(AgentQuery::BookmarkList)),
        );
    }
}

fn add(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
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

    for (request, call, _) in calls.matching(BookmarkTool::BookmarkAdd) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn remove(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: UuidArgs = call.parse("bookmark_remove")?;
        let uuid = RequiredText::get(args.uuid, "bookmark_remove.uuid is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::Remove { uuid },
        )))
    }

    for (request, call, _) in calls.matching(BookmarkTool::BookmarkRemove) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn pin(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
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

    for (request, call, _) in calls.matching(BookmarkTool::BookmarkPin) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn unpin(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: UuidArgs = call.parse("bookmark_unpin")?;
        let uuid = RequiredText::get(args.uuid, "bookmark_unpin.uuid is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::Unpin { uuid },
        )))
    }

    for (request, call, _) in calls.matching(BookmarkTool::BookmarkUnpin) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn create_folder(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: FolderArgs = call.parse("bookmark_folder_create")?;
        let name = RequiredText::get(args.name, "bookmark_folder_create.name is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::CreateFolder { name },
        )))
    }

    for (request, call, _) in calls.matching(BookmarkTool::BookmarkFolderCreate) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
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
