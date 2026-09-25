use vmux_mcp::tool::{
    McpToolPlugin, ToolCall, ToolCalls, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolQuery,
    ToolRequestSet,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};

pub struct BookmarkToolPlugin;

impl Plugin for BookmarkToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(McpToolPlugin::<BookmarkTool>::new(include_str!(
            "bookmark_tool.ron"
        )))
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
    for (request, _, _) in calls.matching(BookmarkTool::BookmarkList) {
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::BookmarkList)));
    }
}

fn parse(mut commands: Commands, calls: ToolCalls<BookmarkTool>) {
    for (request, call, tool) in calls.iter() {
        let parsed = match tool {
            BookmarkTool::BookmarkList => continue,
            BookmarkTool::BookmarkAdd => call.parse::<BookmarkAddArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BookmarkTool::BookmarkRemove => call.parse::<BookmarkRemoveArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BookmarkTool::BookmarkPin => call.parse::<BookmarkPinArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BookmarkTool::BookmarkUnpin => call.parse::<BookmarkUnpinArgs>().map(|args| {
                commands.entity(request).insert(args);
            }),
            BookmarkTool::BookmarkFolderCreate => {
                call.parse::<BookmarkFolderCreateArgs>().map(|args| {
                    commands.entity(request).insert(args);
                })
            }
        };
        if let Err(message) = parsed {
            commands.entity(request).insert(ToolDispatchError::new(message));
        }
    }
}

fn add(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkAddArgs), (With<ToolCall>, Added<BookmarkAddArgs>)>,
) {
    for (entity, args) in &requests {
        let command =
            RequiredText::get(args.url.clone(), "bookmark_add.url is required").map(|url| {
                AgentCommand::BookmarkCommand(AgentBookmarkCommand::Add {
                    page: AgentBookmarkPage {
                        url,
                        title: args.title.clone(),
                        favicon_url: args.favicon_url.clone(),
                    },
                    folder: args.folder.clone(),
                })
            });
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn remove(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkRemoveArgs), (With<ToolCall>, Added<BookmarkRemoveArgs>)>,
) {
    for (entity, args) in &requests {
        let command = RequiredText::get(args.uuid.clone(), "bookmark_remove.uuid is required")
            .map(|uuid| AgentCommand::BookmarkCommand(AgentBookmarkCommand::Remove { uuid }));
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn pin(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkPinArgs), (With<ToolCall>, Added<BookmarkPinArgs>)>,
) {
    for (entity, args) in &requests {
        let command = match args {
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
        .map(AgentCommand::BookmarkCommand);
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn unpin(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkUnpinArgs), (With<ToolCall>, Added<BookmarkUnpinArgs>)>,
) {
    for (entity, args) in &requests {
        let command = RequiredText::get(args.uuid.clone(), "bookmark_unpin.uuid is required")
            .map(|uuid| AgentCommand::BookmarkCommand(AgentBookmarkCommand::Unpin { uuid }));
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn create_folder(
    mut commands: Commands,
    requests: Query<
        (Entity, &BookmarkFolderCreateArgs),
        (With<ToolCall>, Added<BookmarkFolderCreateArgs>),
    >,
) {
    for (entity, args) in &requests {
        let command =
            RequiredText::get(args.name.clone(), "bookmark_folder_create.name is required").map(
                |name| AgentCommand::BookmarkCommand(AgentBookmarkCommand::CreateFolder { name }),
            );
        commands.entity(entity).insert(ToolCommand(command));
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
