use bevy::prelude::*;
use serde::Deserialize;
use vmux_api::BinEvent;
use vmux_api::protocol::{
    AgentBookmark, AgentBookmarkNode, AgentBookmarks, AgentQueryResult, AgentRequest,
    AgentRequestId, ClientMessage,
};
use vmux_core::service::ServiceRequest;
use vmux_core::{Bookmark, BookmarkOrder, Collapsed, Folder, PageIcon, PageMetadata, Pin, Uuid};
use vmux_layout::bookmark::{
    AddRequest, CreateFolderRequest, PinRequest, PinUrlRequest, RemoveRequest, UnpinRequest,
};
use vmux_tool::{
    AddedTool, ToolAppExt, ToolCommand, ToolDispatchSet, ToolManifestPlugin, ToolQuery,
    ToolQueryHandled, ToolQueryRequest, ToolQueryRouteSet,
};

pub struct BookmarkToolPlugin;

impl Plugin for BookmarkToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ToolManifestPlugin::from_feature(
            include_str!("feature.ron"),
            "default",
        ))
        .add_message::<ToolQueryRequest>()
        .add_message::<ToolQueryHandled>()
        .add_message::<ServiceRequest>()
        .register_tool::<BookmarkListArgs>("bookmark_list")
        .register_tool::<BookmarkAddArgs>("bookmark_add")
        .register_tool::<BookmarkRemoveArgs>("bookmark_remove")
        .register_tool::<BookmarkPinArgs>("bookmark_pin")
        .register_tool::<BookmarkUnpinArgs>("bookmark_unpin")
        .register_tool::<BookmarkFolderCreateArgs>("bookmark_folder_create")
        .add_message::<BookmarkListRequest>()
        .add_systems(
            Update,
            (list, add, remove, pin, unpin, create_folder).in_set(ToolDispatchSet),
        )
        .add_systems(Update, route_bookmark_queries.in_set(ToolQueryRouteSet))
        .add_systems(Update, answer_bookmark_queries.after(ToolQueryRouteSet));
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

#[derive(Message)]
struct BookmarkListRequest {
    request_id: AgentRequestId,
}

#[vmux_api::agent]
struct AgentBookmarkList;

fn list(mut commands: Commands, calls: Query<Entity, AddedTool<BookmarkListArgs>>) {
    for request in &calls {
        commands
            .entity(request)
            .insert(ToolQuery(AgentRequest::encode(&AgentBookmarkList)));
    }
}

fn add(
    mut commands: Commands,
    requests: Query<(Entity, &BookmarkAddArgs), AddedTool<BookmarkAddArgs>>,
) {
    for (entity, args) in &requests {
        let command =
            RequiredText::get(args.url.clone(), "bookmark_add.url is required").and_then(|url| {
                AgentRequest::encode(&AddRequest {
                    metadata: PageMetadata {
                        url,
                        title: args.title.clone().unwrap_or_default(),
                        icon: PageIcon::favicon(args.favicon_url.clone().unwrap_or_default()),
                        bg_color: None,
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
            .and_then(|uuid| AgentRequest::encode(&RemoveRequest { uuid }));
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
                    .and_then(|uuid| AgentRequest::encode(&PinRequest { uuid }))
            }
            BookmarkPinArgs::Page(args) => RequiredText::get(
                args.url.clone(),
                "bookmark_pin.url is required",
            )
            .and_then(|url| {
                AgentRequest::encode(&PinUrlRequest {
                    metadata: PageMetadata {
                        url,
                        title: args.title.clone().unwrap_or_default(),
                        icon: PageIcon::favicon(args.favicon_url.clone().unwrap_or_default()),
                        bg_color: None,
                    },
                })
            }),
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
            .and_then(|uuid| AgentRequest::encode(&UnpinRequest { uuid }));
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
                .and_then(|name| AgentRequest::encode(&CreateFolderRequest::root(name)));
        commands.entity(entity).insert(ToolCommand(command));
    }
}

fn route_bookmark_queries(
    mut queries: MessageReader<ToolQueryRequest>,
    mut handled: MessageWriter<ToolQueryHandled>,
    mut bookmarks: MessageWriter<BookmarkListRequest>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in queries.read() {
        if request.query.id != AgentBookmarkList::id() {
            continue;
        }
        handled.write(ToolQueryHandled(request.request_id));
        match request.query.decode::<AgentBookmarkList>() {
            Ok(Some(_)) => {
                bookmarks.write(BookmarkListRequest {
                    request_id: request.request_id,
                });
            }
            Err(message) => {
                service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
                    AgentQueryResult::text(request.request_id, Err(message)),
                )));
            }
            Ok(None) => {}
        }
    }
}

fn answer_bookmark_queries(
    mut requests: MessageReader<BookmarkListRequest>,
    pins: Query<(&Uuid, &PageMetadata, &BookmarkOrder), With<Pin>>,
    folders: Query<
        (
            &Uuid,
            &Name,
            Option<&Children>,
            Has<Collapsed>,
            &BookmarkOrder,
        ),
        With<Folder>,
    >,
    top_level: Query<(&Uuid, &PageMetadata, &BookmarkOrder), (With<Bookmark>, Without<ChildOf>)>,
    bookmarks: Query<(&Uuid, &PageMetadata, &BookmarkOrder), With<Bookmark>>,
    mut service_requests: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let mut pin_rows: Vec<(u32, AgentBookmark)> = pins
            .iter()
            .map(|(uuid, metadata, order)| {
                (
                    order.0,
                    AgentBookmark::new(
                        uuid.0.clone(),
                        metadata.url.clone(),
                        metadata.title.clone(),
                        metadata.icon.favicon_url(),
                    ),
                )
            })
            .collect();
        pin_rows.sort_by_key(|(order, _)| *order);
        let pins = pin_rows.into_iter().map(|(_, bookmark)| bookmark).collect();
        let mut roots: Vec<(u32, AgentBookmarkNode)> = Vec::new();
        for (uuid, name, children, collapsed, order) in &folders {
            let mut child_rows: Vec<(u32, AgentBookmark)> = Vec::new();
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok((child_uuid, metadata, child_order)) = bookmarks.get(child) {
                        child_rows.push((
                            child_order.0,
                            AgentBookmark::new(
                                child_uuid.0.clone(),
                                metadata.url.clone(),
                                metadata.title.clone(),
                                metadata.icon.favicon_url(),
                            ),
                        ));
                    }
                }
            }
            child_rows.sort_by_key(|(order, _)| *order);
            let children = child_rows
                .into_iter()
                .map(|(_, bookmark)| bookmark)
                .collect();
            roots.push((
                order.0,
                AgentBookmarkNode::Folder {
                    uuid: uuid.0.clone(),
                    name: name.to_string(),
                    collapsed,
                    children,
                },
            ));
        }
        for (uuid, metadata, order) in &top_level {
            roots.push((
                order.0,
                AgentBookmarkNode::Entry {
                    bookmark: AgentBookmark::new(
                        uuid.0.clone(),
                        metadata.url.clone(),
                        metadata.title.clone(),
                        metadata.icon.favicon_url(),
                    ),
                },
            ));
        }
        roots.sort_by_key(|(order, _)| *order);
        let roots = roots.into_iter().map(|(_, node)| node).collect();
        let result = serde_json::to_string(&AgentBookmarks { pins, roots })
            .map_err(|error| error.to_string());
        service_requests.write(ServiceRequest(ClientMessage::AgentQueryResult(
            AgentQueryResult::text(request.request_id, result),
        )));
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
