use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::protocol::{
    AgentBookmarkAdd, AgentBookmarkFolderCreate, AgentBookmarkId, AgentBookmarkPage,
    AgentBookmarkPinUrl, AgentCommand, AgentQuery,
};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
};

pub struct BookmarkToolPlugin;

impl Plugin for BookmarkToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::new(include_str!("bookmark_tool.ron")))
            .register_tool::<BookmarkListArgs>("bookmark_list")
            .register_tool::<BookmarkAddArgs>("bookmark_add")
            .register_tool::<BookmarkRemoveArgs>("bookmark_remove")
            .register_tool::<BookmarkPinArgs>("bookmark_pin")
            .register_tool::<BookmarkUnpinArgs>("bookmark_unpin")
            .register_tool::<BookmarkFolderCreateArgs>("bookmark_folder_create")
            .add_systems(
                Update,
                (list, add, remove, pin, unpin, create_folder).in_set(ToolDispatchSet),
            );
    }
}

#[derive(Component, Deserialize)]
#[serde(deny_unknown_fields)]
struct BookmarkListArgs {}

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

fn list(mut commands: Commands, calls: Query<Entity, AddedTool<BookmarkListArgs>>) {
    for request in &calls {
        commands
            .entity(request)
            .insert(ToolQuery(Ok(AgentQuery::BookmarkList)));
    }
}

fn add(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkAddArgs), AddedTool<BookmarkAddArgs>>,
) {
    for (entity, args) in &requests {
        let command =
            RequiredText::get(args.url.clone(), "bookmark_add.url is required").map(|url| {
                AgentCommand::BookmarkAdd(AgentBookmarkAdd {
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
            .map(|uuid| AgentCommand::BookmarkRemove(AgentBookmarkId { uuid }));
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
                    .map(|uuid| AgentCommand::BookmarkPin(AgentBookmarkId { uuid }))
            }
            BookmarkPinArgs::Page(args) => {
                RequiredText::get(args.url.clone(), "bookmark_pin.url is required").map(|url| {
                    AgentCommand::BookmarkPinUrl(AgentBookmarkPinUrl {
                        page: AgentBookmarkPage {
                            url,
                            title: args.title.clone(),
                            favicon_url: args.favicon_url.clone(),
                        },
                    })
                })
            }
        };
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn unpin(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkUnpinArgs), AddedTool<BookmarkUnpinArgs>>,
) {
    for (entity, args) in &requests {
        let command = RequiredText::get(args.uuid.clone(), "bookmark_unpin.uuid is required")
            .map(|uuid| AgentCommand::BookmarkUnpin(AgentBookmarkId { uuid }));
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn create_folder(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkFolderCreateArgs), AddedTool<BookmarkFolderCreateArgs>>,
) {
    for (entity, args) in &requests {
        let command =
            RequiredText::get(args.name.clone(), "bookmark_folder_create.name is required")
                .map(|name| AgentCommand::BookmarkFolderCreate(AgentBookmarkFolderCreate { name }));
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
