use super::{
    DispatchTarget, NextToolOrder, ParsedToolCall, ToolCalls, ToolDispatchSet, ToolManifest,
    ToolRegistrationSet, ToolRequestSet,
};
use bevy_app::{App, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_client::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};

pub(super) struct BookmarkToolPlugin;

impl Plugin for BookmarkToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register.in_set(ToolRegistrationSet::Bookmark))
            .add_systems(Update, parse.in_set(ToolRequestSet))
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

fn register(mut commands: Commands, mut next_order: ResMut<NextToolOrder>) {
    ToolManifest::<BookmarkTool>::from_ron(include_str!("bookmark.ron"))
        .spawn(&mut commands, &mut next_order);
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkAddArgs {
    url: String,
    title: Option<String>,
    favicon_url: Option<String>,
    folder: Option<String>,
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkRemoveArgs {
    uuid: String,
}

#[derive(Component, Deserialize)]
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

#[derive(Component, Deserialize)]
#[serde(untagged)]
enum BookmarkPinArgs {
    Existing(ExistingPinArgs),
    Page(PagePinArgs),
}

#[derive(Component, Deserialize)]
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

fn parse(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    for (request, call, tool) in calls.iter() {
        match tool {
            BookmarkTool::BookmarkList => {}
            BookmarkTool::BookmarkAdd => call.parse_into::<BookmarkAddArgs>(request, &mut commands),
            BookmarkTool::BookmarkRemove => {
                call.parse_into::<BookmarkRemoveArgs>(request, &mut commands)
            }
            BookmarkTool::BookmarkPin => call.parse_into::<BookmarkPinArgs>(request, &mut commands),
            BookmarkTool::BookmarkUnpin => {
                call.parse_into::<BookmarkUnpinArgs>(request, &mut commands)
            }
            BookmarkTool::BookmarkFolderCreate => {
                call.parse_into::<BookmarkFolderCreateArgs>(request, &mut commands)
            }
        }
    }
}

fn add(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BookmarkAddArgs>),
        Added<ParsedToolCall<BookmarkAddArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let args = request.args();
        let target =
            RequiredText::get(args.url.clone(), "bookmark_add.url is required").map(|url| {
                DispatchTarget::Command(AgentCommand::BookmarkCommand(AgentBookmarkCommand::Add {
                    page: AgentBookmarkPage {
                        url,
                        title: args.title.clone(),
                        favicon_url: args.favicon_url.clone(),
                    },
                    folder: args.folder.clone(),
                }))
            });
        request.finish(entity, &mut commands, target);
    }
}

fn remove(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BookmarkRemoveArgs>),
        Added<ParsedToolCall<BookmarkRemoveArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let target = RequiredText::get(
            request.args().uuid.clone(),
            "bookmark_remove.uuid is required",
        )
        .map(|uuid| {
            DispatchTarget::Command(AgentCommand::BookmarkCommand(
                AgentBookmarkCommand::Remove { uuid },
            ))
        });
        request.finish(entity, &mut commands, target);
    }
}

fn pin(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BookmarkPinArgs>),
        Added<ParsedToolCall<BookmarkPinArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let target = match request.args() {
            BookmarkPinArgs::Existing(args) => {
                RequiredText::get(args.uuid.clone(), "bookmark_pin.uuid is required")
                    .map(|uuid| AgentBookmarkCommand::Pin { uuid })
            }
            BookmarkPinArgs::Page(args) => {
                RequiredText::get(args.url.clone(), "bookmark_pin.url is required").map(|url| {
                    AgentBookmarkCommand::PinUrl {
                        page: AgentBookmarkPage {
                            url,
                            title: args.title.clone(),
                            favicon_url: args.favicon_url.clone(),
                        },
                    }
                })
            }
        }
        .map(|command| DispatchTarget::Command(AgentCommand::BookmarkCommand(command)));
        request.finish(entity, &mut commands, target);
    }
}

fn unpin(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BookmarkUnpinArgs>),
        Added<ParsedToolCall<BookmarkUnpinArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let target = RequiredText::get(
            request.args().uuid.clone(),
            "bookmark_unpin.uuid is required",
        )
        .map(|uuid| {
            DispatchTarget::Command(AgentCommand::BookmarkCommand(AgentBookmarkCommand::Unpin {
                uuid,
            }))
        });
        request.finish(entity, &mut commands, target);
    }
}

fn create_folder(
    mut commands: Commands,
    requests: Query<
        (Entity, &ParsedToolCall<BookmarkFolderCreateArgs>),
        Added<ParsedToolCall<BookmarkFolderCreateArgs>>,
    >,
) {
    for (entity, request) in &requests {
        let target = RequiredText::get(
            request.args().name.clone(),
            "bookmark_folder_create.name is required",
        )
        .map(|name| {
            DispatchTarget::Command(AgentCommand::BookmarkCommand(
                AgentBookmarkCommand::CreateFolder { name },
            ))
        });
        request.finish(entity, &mut commands, target);
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
