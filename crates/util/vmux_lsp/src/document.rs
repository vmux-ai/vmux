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
            if openings
                .get(request.owner)
                .is_ok_and(|opening| opening.path == request.path)
            {
                commands
                    .entity(request.owner)
                    .remove::<(OpenDocument, DocumentFailure)>();
            }
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
        commands
            .entity(entity)
            .remove::<(Contents, crate::DiagnosticPath)>()
            .insert(Closed);
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

impl OpenDocument {
    fn fail(&self, owner: Entity, message: impl Into<String>, commands: &mut Commands) {
        commands
            .entity(owner)
            .remove::<OpenDocument>()
            .insert(DocumentFailure(message.into()));
    }

    fn apply_pending(
        &self,
        transport: &Transport,
        uri: &str,
        pending: &mut [(Entity, PathBuf, Entity, i32, String, String)],
        commands: &mut Commands,
    ) -> Result<Option<(Entity, i32)>, &'static str> {
        for (server, candidate, entity, version, language, text) in pending {
            if *server != self.server || *candidate != self.path {
                continue;
            }
            if *language != self.language {
                return Err("document already has another language");
            }
            if *text != self.text {
                let Some(next) = version.checked_add(1) else {
                    return Err("document version exhausted");
                };
                transport.did_change(uri, next, &self.text);
                *version = next;
                text.clone_from(&self.text);
                commands.entity(*entity).insert((
                    Document { version: next },
                    Contents {
                        language: language.clone(),
                        text: text.clone(),
                    },
                ));
            }
            return Ok(Some((*entity, *version)));
        }
        Ok(None)
    }

    fn apply_existing(
        &self,
        transport: &Transport,
        uri: &str,
        documents: &mut Existing,
        pending: &mut Vec<(Entity, PathBuf, Entity, i32, String, String)>,
        commands: &mut Commands,
    ) -> Result<Option<(Entity, i32)>, &'static str> {
        for (entity, candidate, mut document, parent, connection, closed, contents) in documents {
            if candidate.0 != self.path
                || parent.parent() != self.server
                || connection.0 != Some(transport.identity())
            {
                continue;
            }
            if !closed && contents.is_some_and(|contents| contents.language != self.language) {
                return Err("document already has another language");
            }
            if closed || contents.is_none_or(|contents| contents.text != self.text) {
                let Some(version) = document.version.checked_add(1) else {
                    return Err("document version exhausted");
                };
                if closed {
                    transport.did_open(uri, &self.language, version, &self.text);
                } else {
                    transport.did_change(uri, version, &self.text);
                }
                document.version = version;
                commands.entity(entity).remove::<Closed>().insert(Contents {
                    language: self.language.clone(),
                    text: self.text.clone(),
                });
            }
            pending.push((
                self.server,
                self.path.clone(),
                entity,
                document.version,
                self.language.clone(),
                self.text.clone(),
            ));
            return Ok(Some((entity, document.version)));
        }
        Ok(None)
    }

    fn spawn_new(
        &self,
        transport: &Transport,
        uri: &str,
        path: DocumentPath,
        pending: &mut Vec<(Entity, PathBuf, Entity, i32, String, String)>,
        commands: &mut Commands,
    ) -> (Entity, i32) {
        transport.did_open(uri, &self.language, 1, &self.text);
        let entity = commands
            .spawn((
                Name::new(self.path.to_string_lossy().into_owned()),
                path,
                Document { version: 1 },
                Contents {
                    language: self.language.clone(),
                    text: self.text.clone(),
                },
                ChildOf(self.server),
            ))
            .id();
        pending.push((
            self.server,
            self.path.clone(),
            entity,
            1,
            self.language.clone(),
            self.text.clone(),
        ));
        (entity, 1)
    }
}

fn open(
    requests: Query<(Entity, &OpenDocument, Option<&DocumentBinding>)>,
    servers: Servers,
    mut documents: Existing,
    mut commands: Commands,
) {
    let mut pending = Vec::<(Entity, PathBuf, Entity, i32, String, String)>::new();
    for (owner, request, binding) in &requests {
        let Ok((transport, _, stopped, failed)) = servers.get(request.server) else {
            request.fail(owner, "language server no longer exists", &mut commands);
            continue;
        };
        if stopped || failed {
            request.fail(owner, "language server is unavailable", &mut commands);
            continue;
        }
        let Some(transport) = transport else { continue };
        let path = DocumentPath(request.path.clone());
        let Some(uri) = path.uri() else {
            request.fail(
                owner,
                "document path must identify an absolute file URI",
                &mut commands,
            );
            continue;
        };
        let target = match request.apply_pending(transport, &uri, &mut pending, &mut commands) {
            Ok(Some(target)) => Some(target),
            Ok(None) => {
                match request.apply_existing(
                    transport,
                    &uri,
                    &mut documents,
                    &mut pending,
                    &mut commands,
                ) {
                    Ok(target) => target,
                    Err(error) => {
                        request.fail(owner, error, &mut commands);
                        continue;
                    }
                }
            }
            Err(error) => {
                request.fail(owner, error, &mut commands);
                continue;
            }
        };
        let (document, version) = match target {
            Some(target) => target,
            None => request.spawn_new(transport, &uri, path, &mut pending, &mut commands),
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
