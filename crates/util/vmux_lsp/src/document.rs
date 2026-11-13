use std::collections::HashMap;
use std::path::PathBuf;

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;

use crate::{
    Document, DocumentBinding, DocumentConnection, DocumentPath, DocumentViews, Server,
    ServerFailure, ServerKey, Set, Stopped, Transport,
};

pub(crate) struct DocumentsPlugin;

impl Plugin for DocumentsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ChangeDocument>()
            .add_message::<CloseDocument>()
            .add_systems(
                Update,
                (close, cleanup, open, change, cleanup)
                    .chain()
                    .in_set(Set::Documents),
            );
    }
}

#[derive(Component)]
pub struct OpenDocument {
    pub server: Entity,
    pub path: PathBuf,
    pub language: String,
    pub text: String,
    pub revision: Option<uuid::Uuid>,
}

#[derive(Message)]
pub struct ChangeDocument {
    pub owner: Entity,
    pub path: PathBuf,
    pub text: String,
    pub revision: Option<uuid::Uuid>,
}

#[derive(Message)]
pub struct CloseDocument {
    pub owner: Entity,
    pub path: PathBuf,
}

#[derive(Component, Clone, Copy)]
pub struct DocumentApplied {
    pub document: Entity,
    pub version: i32,
    pub revision: Option<uuid::Uuid>,
}

#[derive(Component)]
pub struct DocumentFailure(pub String);

#[derive(Component)]
pub(crate) struct Closed;

#[derive(Component)]
struct Contents {
    language: String,
    text: String,
}

fn close(
    mut requests: MessageReader<CloseDocument>,
    bindings: Query<&DocumentBinding>,
    openings: Query<&OpenDocument>,
    documents: Query<(&DocumentPath, &Document)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(binding) = bindings.get(request.owner) else {
            if openings
                .get(request.owner)
                .is_ok_and(|opening| opening.path == request.path)
            {
                commands
                    .entity(request.owner)
                    .remove::<(OpenDocument, DocumentFailure)>();
            }
            continue;
        };
        let Ok((path, document)) = documents.get(binding.document()) else {
            continue;
        };
        if path.0 != request.path {
            continue;
        }
        commands
            .entity(request.owner)
            .remove::<(DocumentBinding, DocumentFailure)>()
            .insert(DocumentApplied {
                document: binding.document(),
                version: document.version,
                revision: None,
            });
    }
}

type Unowned<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static DocumentPath,
        &'static ChildOf,
        &'static DocumentConnection,
        Option<&'static DocumentViews>,
    ),
    (With<Document>, Without<Closed>),
>;

fn cleanup(documents: Unowned, transports: Query<&Transport>, mut commands: Commands) {
    for (entity, path, parent, connection, views) in &documents {
        if views.is_some_and(|views| !views.0.is_empty()) {
            continue;
        }
        if let Ok(transport) = transports.get(parent.parent())
            && connection.0 == Some(transport.identity())
            && let Some(uri) = path.uri()
        {
            transport.did_close(&uri);
        }
        commands.entity(entity).remove::<Contents>().insert(Closed);
    }
}

type Servers<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Transport>,
        &'static ServerKey,
        Has<Stopped>,
        Has<ServerFailure>,
    ),
    With<Server>,
>;

type Existing<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static DocumentPath,
        &'static mut Document,
        &'static ChildOf,
        &'static DocumentConnection,
        Has<Closed>,
        Option<&'static Contents>,
    ),
>;

fn open(
    requests: Query<(Entity, &OpenDocument, Option<&DocumentBinding>)>,
    servers: Servers,
    mut documents: Existing,
    mut commands: Commands,
) {
    let mut pending = Vec::<(Entity, PathBuf, Entity, i32, String, String)>::new();
    for (owner, request, binding) in &requests {
        let Ok((transport, _, stopped, failed)) = servers.get(request.server) else {
            commands
                .entity(owner)
                .remove::<OpenDocument>()
                .insert(DocumentFailure("language server no longer exists".into()));
            continue;
        };
        if stopped || failed {
            commands
                .entity(owner)
                .remove::<OpenDocument>()
                .insert(DocumentFailure("language server is unavailable".into()));
            continue;
        }
        let Some(transport) = transport else { continue };
        let path = DocumentPath(request.path.clone());
        let Some(uri) = path.uri() else {
            commands
                .entity(owner)
                .remove::<OpenDocument>()
                .insert(DocumentFailure(
                    "document path must identify an absolute file URI".into(),
                ));
            continue;
        };
        let mut target = None;
        let mut error = None;
        for (server, candidate, entity, version, language, text) in &mut pending {
            if *server != request.server || *candidate != request.path {
                continue;
            }
            if *language != request.language {
                error = Some("document already has another language");
                break;
            }
            if *text != request.text {
                let Some(next) = version.checked_add(1) else {
                    error = Some("document version exhausted");
                    break;
                };
                transport.did_change(&uri, next, &request.text);
                *version = next;
                text.clone_from(&request.text);
                commands.entity(*entity).insert((
                    Document { version: next },
                    Contents {
                        language: language.clone(),
                        text: text.clone(),
                    },
                ));
            }
            target = Some((*entity, *version));
            break;
        }
        if target.is_none() && error.is_none() {
            for (entity, candidate, mut document, parent, connection, closed, contents) in
                &mut documents
            {
                if candidate.0 != request.path
                    || parent.parent() != request.server
                    || connection.0 != Some(transport.identity())
                {
                    continue;
                }
                if !closed && contents.is_some_and(|contents| contents.language != request.language)
                {
                    error = Some("document already has another language");
                    break;
                }
                if closed || contents.is_none_or(|contents| contents.text != request.text) {
                    let Some(version) = document.version.checked_add(1) else {
                        error = Some("document version exhausted");
                        break;
                    };
                    if closed {
                        transport.did_open(&uri, &request.language, version, &request.text);
                    } else {
                        transport.did_change(&uri, version, &request.text);
                    }
                    document.version = version;
                    commands.entity(entity).remove::<Closed>().insert(Contents {
                        language: request.language.clone(),
                        text: request.text.clone(),
                    });
                }
                pending.push((
                    request.server,
                    request.path.clone(),
                    entity,
                    document.version,
                    request.language.clone(),
                    request.text.clone(),
                ));
                target = Some((entity, document.version));
                break;
            }
        }
        if let Some(error) = error {
            commands
                .entity(owner)
                .remove::<OpenDocument>()
                .insert(DocumentFailure(error.into()));
            continue;
        }
        let (document, version) = match target {
            Some(target) => target,
            None => {
                transport.did_open(&uri, &request.language, 1, &request.text);
                let entity = commands
                    .spawn((
                        Name::new(request.path.to_string_lossy().into_owned()),
                        path,
                        Document { version: 1 },
                        Contents {
                            language: request.language.clone(),
                            text: request.text.clone(),
                        },
                        ChildOf(request.server),
                    ))
                    .id();
                pending.push((
                    request.server,
                    request.path.clone(),
                    entity,
                    1,
                    request.language.clone(),
                    request.text.clone(),
                ));
                (entity, 1)
            }
        };
        let mut owner = commands.entity(owner);
        if binding.is_none_or(|binding| binding.document() != document) {
            owner.insert(DocumentBinding::new(document));
        }
        owner
            .remove::<(OpenDocument, DocumentFailure)>()
            .insert(DocumentApplied {
                document,
                version,
                revision: request.revision,
            });
    }
}

type Editable<'w, 's> = Query<
    'w,
    's,
    (
        &'static DocumentPath,
        &'static mut Document,
        &'static ChildOf,
        &'static DocumentConnection,
        Option<&'static mut Contents>,
    ),
    Without<Closed>,
>;

fn change(
    mut requests: MessageReader<ChangeDocument>,
    bindings: Query<&DocumentBinding>,
    mut documents: Editable,
    transports: Query<&Transport>,
    mut commands: Commands,
) {
    let mut ordered = Vec::new();
    let mut newest = HashMap::new();
    for request in requests.read() {
        newest.insert(request.owner, ordered.len());
        ordered.push(request);
    }
    for (index, request) in ordered.into_iter().enumerate() {
        if newest.get(&request.owner) != Some(&index) {
            continue;
        }
        let Ok(binding) = bindings.get(request.owner) else {
            continue;
        };
        let Ok((path, mut document, parent, connection, contents)) =
            documents.get_mut(binding.document())
        else {
            continue;
        };
        if path.0 != request.path {
            continue;
        }
        let Ok(transport) = transports.get(parent.parent()) else {
            continue;
        };
        if connection.0 != Some(transport.identity()) {
            continue;
        }
        let Some(uri) = path.uri() else { continue };
        let Some(version) = document.version.checked_add(1) else {
            commands
                .entity(request.owner)
                .insert(DocumentFailure("document version exhausted".into()));
            continue;
        };
        transport.did_change(&uri, version, &request.text);
        document.version = version;
        if let Some(mut contents) = contents {
            contents.text.clone_from(&request.text);
        }
        commands
            .entity(request.owner)
            .remove::<DocumentFailure>()
            .insert(DocumentApplied {
                document: binding.document(),
                version,
                revision: request.revision,
            });
    }
}
