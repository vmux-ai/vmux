use super::{
    DispatchTarget, ToolCall, ToolCalls, ToolDispatchSet, ToolManifest, ToolRegistrationSet,
    ToolSpawner,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::{Commands, Component, IntoScheduleConfigs};
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};

pub(super) struct BookmarkToolPlugin;

impl Plugin for BookmarkToolPlugin {
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
#[serde(deny_unknown_fields)]
struct BookmarkAddArgs {
    url: String,
    title: Option<String>,
    favicon_url: Option<String>,
    folder: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkRemoveArgs {
    uuid: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkUnpinArgs {
    uuid: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExistingPinArgs {
    uuid: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PagePinArgs {
    url: String,
    title: Option<String>,
    favicon_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BookmarkPinArgs {
    Existing(ExistingPinArgs),
    Page(PagePinArgs),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkFolderCreateArgs {
    name: String,
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
        let args: BookmarkAddArgs = call.parse("bookmark_add")?;
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
        let args: BookmarkRemoveArgs = call.parse("bookmark_remove")?;
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
        let args: BookmarkPinArgs = call.parse("bookmark_pin")?;
        let command = match args {
            BookmarkPinArgs::Existing(args) => AgentBookmarkCommand::Pin {
                uuid: RequiredText::get(args.uuid, "bookmark_pin.uuid is required")?,
            },
            BookmarkPinArgs::Page(args) => AgentBookmarkCommand::PinUrl {
                page: AgentBookmarkPage {
                    url: RequiredText::get(args.url, "bookmark_pin.url is required")?,
                    title: args.title,
                    favicon_url: args.favicon_url,
                },
            },
        };
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            command,
        )))
    }

    for (request, call, _) in calls.matching(BookmarkTool::BookmarkPin) {
        call.finish_dispatch(request, &mut commands, target(call));
    }
}

fn unpin(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: BookmarkUnpinArgs = call.parse("bookmark_unpin")?;
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
        let args: BookmarkFolderCreateArgs = call.parse("bookmark_folder_create")?;
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
    fn get(value: String, error: &str) -> Result<String, String> {
        (!value.trim().is_empty())
            .then_some(value)
            .ok_or_else(|| error.to_string())
    }
}
