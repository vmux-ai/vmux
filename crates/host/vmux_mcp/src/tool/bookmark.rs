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

impl BookmarkAddArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("bookmark_add")?;
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkRemoveArgs {
    uuid: String,
}

impl BookmarkRemoveArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("bookmark_remove")?;
        let uuid = RequiredText::get(args.uuid, "bookmark_remove.uuid is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::Remove { uuid },
        )))
    }
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

impl BookmarkPinArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("bookmark_pin")?;
        let command = match args {
            Self::Existing(args) => AgentBookmarkCommand::Pin {
                uuid: RequiredText::get(args.uuid, "bookmark_pin.uuid is required")?,
            },
            Self::Page(args) => AgentBookmarkCommand::PinUrl {
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
}

impl BookmarkUnpinArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("bookmark_unpin")?;
        let uuid = RequiredText::get(args.uuid, "bookmark_unpin.uuid is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::Unpin { uuid },
        )))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkFolderCreateArgs {
    name: String,
}

impl BookmarkFolderCreateArgs {
    fn target(call: &ToolCall) -> Result<DispatchTarget, String> {
        let args: Self = call.parse("bookmark_folder_create")?;
        let name = RequiredText::get(args.name, "bookmark_folder_create.name is required")?;
        Ok(DispatchTarget::Command(AgentCommand::BookmarkCommand(
            AgentBookmarkCommand::CreateFolder { name },
        )))
    }
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
    for (request, call, _) in calls.matching(BookmarkTool::BookmarkAdd) {
        call.finish_dispatch(request, &mut commands, BookmarkAddArgs::target(call));
    }
}

fn remove(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    for (request, call, _) in calls.matching(BookmarkTool::BookmarkRemove) {
        call.finish_dispatch(request, &mut commands, BookmarkRemoveArgs::target(call));
    }
}

fn pin(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    for (request, call, _) in calls.matching(BookmarkTool::BookmarkPin) {
        call.finish_dispatch(request, &mut commands, BookmarkPinArgs::target(call));
    }
}

fn unpin(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    for (request, call, _) in calls.matching(BookmarkTool::BookmarkUnpin) {
        call.finish_dispatch(request, &mut commands, BookmarkUnpinArgs::target(call));
    }
}

fn create_folder(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    for (request, call, _) in calls.matching(BookmarkTool::BookmarkFolderCreate) {
        call.finish_dispatch(
            request,
            &mut commands,
            BookmarkFolderCreateArgs::target(call),
        );
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
