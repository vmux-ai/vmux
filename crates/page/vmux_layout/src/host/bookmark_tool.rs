use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vmux_api::protocol::{AgentBookmarkCommand, AgentBookmarkPage, AgentCommand, AgentQuery};
use vmux_core::JsonArguments;
use vmux_mcp::tool::{
    AddedTool, McpToolPlugin, ToolCommand, ToolDispatchError, ToolDispatchSet, ToolQuery,
    ToolRequestSet,
};

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

fn list(mut commands: Commands, calls: Query<(Entity, &BookmarkTool), AddedTool<BookmarkTool>>) {
    for (request, tool) in &calls {
        if *tool != BookmarkTool::BookmarkList {
            continue;
        }
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::BookmarkList)));
    }
}

fn parse(
    mut commands: Commands,
    calls: Query<(Entity, &Name, &JsonArguments, &BookmarkTool), AddedTool<BookmarkTool>>,
) {
    for (request, name, arguments, tool) in &calls {
        let parsed =
            match tool {
                BookmarkTool::BookmarkList => continue,
                BookmarkTool::BookmarkAdd => {
                    arguments
                        .parse::<BookmarkAddArgs>(name.as_str())
                        .map(|args| {
                            commands.entity(request).insert(args);
                        })
                }
                BookmarkTool::BookmarkRemove => arguments
                    .parse::<BookmarkRemoveArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    }),
                BookmarkTool::BookmarkPin => {
                    arguments
                        .parse::<BookmarkPinArgs>(name.as_str())
                        .map(|args| {
                            commands.entity(request).insert(args);
                        })
                }
                BookmarkTool::BookmarkUnpin => arguments
                    .parse::<BookmarkUnpinArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    }),
                BookmarkTool::BookmarkFolderCreate => arguments
                    .parse::<BookmarkFolderCreateArgs>(name.as_str())
                    .map(|args| {
                        commands.entity(request).insert(args);
                    }),
            };
        if let Err(message) = parsed {
            commands
                .entity(request)
                .insert(ToolDispatchError::new(message));
        }
    }
}

fn add(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkAddArgs), AddedTool<BookmarkAddArgs>>,
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
    requests: Query<(Entity, &BookmarkRemoveArgs), AddedTool<BookmarkRemoveArgs>>,
) {
    for (entity, args) in &requests {
        let command = RequiredText::get(args.uuid.clone(), "bookmark_remove.uuid is required")
            .map(|uuid| AgentCommand::BookmarkCommand(AgentBookmarkCommand::Remove { uuid }));
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn pin(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkPinArgs), AddedTool<BookmarkPinArgs>>,
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
    requests: Query<(Entity, &BookmarkUnpinArgs), AddedTool<BookmarkUnpinArgs>>,
) {
    for (entity, args) in &requests {
        let command = RequiredText::get(args.uuid.clone(), "bookmark_unpin.uuid is required")
            .map(|uuid| AgentCommand::BookmarkCommand(AgentBookmarkCommand::Unpin { uuid }));
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn create_folder(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkFolderCreateArgs), AddedTool<BookmarkFolderCreateArgs>>,
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
