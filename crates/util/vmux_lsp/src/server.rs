use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_ecs::{Cwd, Operation, OperationCompleted, OperationState, OperationTarget};

use crate::{Document, DocumentConnection, Root, Server, ServerKey, Set, Transport};

pub(crate) struct ServersPlugin;

impl Plugin for ServersPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RestartServer>()
            .add_message::<StopServer>()
            .add_message::<LaunchServer>()
            .add_systems(
                Update,
                (
                    reset,
                    restart,
                    stop,
                    cancel_start,
                    accept,
                    finish,
                    poll,
                    retire,
                )
                    .chain()
                    .in_set(Set::Servers),
            );
    }
}

#[derive(Component)]
pub struct StartServer;

#[derive(Component)]
pub struct Stopped;

#[derive(Component)]
pub struct ServerFailure(pub String);

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct LaunchTicket {
    operation: Entity,
    server: Entity,
    root: Entity,
    key: ServerKey,
}

#[derive(Message, Clone)]
pub struct LaunchServer {
    pub operation: Entity,
    ticket: LaunchTicket,
}

impl LaunchServer {
    pub fn server(&self) -> Entity {
        self.ticket.server
    }

    pub fn key(&self) -> &ServerKey {
        &self.ticket.key
    }
}

#[derive(Component)]
pub struct ServerCompleted {
    ticket: LaunchTicket,
    result: Option<Result<Transport, String>>,
}

impl ServerCompleted {
    pub fn new(launch: LaunchServer, result: Result<Transport, String>) -> Self {
        Self {
            ticket: launch.ticket,
            result: Some(result),
        }
    }
}

#[derive(Message)]
pub struct RestartServer {
    pub server: Entity,
}

#[derive(Message)]
pub struct StopServer {
    pub server: Entity,
}

#[derive(Component)]
struct Starting(Entity);

type RootChanges = (With<Root>, Changed<Cwd>);

fn reset(
    roots: Query<(Entity, Ref<Cwd>), RootChanges>,
    servers: Query<(Entity, &ChildOf), With<Server>>,
    mut commands: Commands,
) {
    for (root, path) in &roots {
        if path.is_added() {
            continue;
        }
        for (server, parent) in &servers {
            if parent.parent() == root {
                commands.entity(server).despawn();
            }
        }
    }
}

fn restart(
    mut requests: MessageReader<RestartServer>,
    servers: Query<(&ServerKey, &ChildOf), With<Server>>,
    roots: Query<&Cwd, With<Root>>,
    mut commands: Commands,
) {
    let mut consumed = std::collections::HashSet::new();
    for request in requests.read() {
        if !consumed.insert(request.server) {
            continue;
        }
        let Ok((key, parent)) = servers.get(request.server) else {
            continue;
        };
        let root = parent.parent();
        let Ok(path) = roots.get(root) else { continue };
        if path.0 != key.root() {
            continue;
        }
        commands.entity(request.server).despawn();
        commands.spawn((
            Server,
            key.clone(),
            StartServer,
            ChildOf(root),
            Name::new(key.command().to_string()),
        ));
    }
}

fn stop(
    mut requests: MessageReader<StopServer>,
    servers: Query<(), With<Server>>,
    documents: Query<(Entity, &ChildOf), With<Document>>,
    starts: Query<(Entity, &LaunchTicket, &OperationState)>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if !servers.contains(request.server) {
            continue;
        }
        for (document, parent) in &documents {
            if parent.parent() == request.server {
                commands.entity(document).despawn();
            }
        }
        for (operation, ticket, state) in &starts {
            if ticket.server == request.server
                && matches!(state, OperationState::Pending | OperationState::Running)
            {
                commands.entity(operation).insert(OperationState::Canceled);
            }
        }
        commands
            .entity(request.server)
            .remove::<(Transport, StartServer, Starting, ServerFailure)>()
            .insert(Stopped);
    }
}

type PendingServers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ServerKey,
        &'static ChildOf,
        Option<&'static Transport>,
        Has<Starting>,
    ),
    (With<Server>, With<StartServer>),
>;

fn accept(
    servers: PendingServers,
    roots: Query<&Cwd, With<Root>>,
    mut launches: MessageWriter<LaunchServer>,
    mut commands: Commands,
) {
    for (server, key, parent, transport, starting) in &servers {
        commands.entity(server).remove::<StartServer>();
        if transport.is_some() || starting {
            continue;
        }
        let root = parent.parent();
        let valid = roots.get(root).is_ok_and(|path| path.0 == key.root());
        if !valid {
            commands
                .entity(server)
                .insert(ServerFailure("server root is unavailable".into()));
            continue;
        }
        commands.entity(server).remove::<(Stopped, ServerFailure)>();
        let operation = commands
            .spawn((
                Operation::bundle("lsp.start").unwrap(),
                OperationTarget::new(server),
                ChildOf(server),
            ))
            .id();
        commands.entity(server).insert(Starting(operation));
        let ticket = LaunchTicket {
            operation,
            server,
            root,
            key: key.clone(),
        };
        commands.entity(operation).insert(ticket.clone());
        launches.write(LaunchServer { operation, ticket });
    }
}

type StartingServers = (With<Server>, With<Starting>);

fn finish(
    mut completed: Query<(Entity, &LaunchTicket, &mut ServerCompleted, &OperationState)>,
    servers: Query<(&ServerKey, &ChildOf, &Starting), StartingServers>,
    roots: Query<&Cwd, With<Root>>,
    mut commands: Commands,
) {
    for (operation, accepted, mut completed, state) in &mut completed {
        let Some(result) = completed.result.take() else {
            continue;
        };
        if completed.ticket != *accepted {
            commands.entity(operation).remove::<ServerCompleted>();
            continue;
        }
        let current = accepted.operation == operation
            && servers
                .get(accepted.server)
                .is_ok_and(|(key, parent, starting)| {
                    key == &accepted.key
                        && parent.parent() == accepted.root
                        && starting.0 == operation
                })
            && roots
                .get(accepted.root)
                .is_ok_and(|path| path.0 == accepted.key.root());
        if !current || !matches!(state, OperationState::Pending | OperationState::Running) {
            if matches!(state, OperationState::Pending | OperationState::Running) {
                commands.entity(operation).insert(OperationState::Canceled);
            }
            commands.entity(operation).remove::<ServerCompleted>();
            continue;
        }
        commands.entity(accepted.server).remove::<Starting>();
        match result {
            Ok(transport) => {
                commands.entity(accepted.server).insert(transport);
                commands.trigger(OperationCompleted {
                    entity: operation,
                    result: Ok(()),
                });
            }
            Err(error) => {
                commands
                    .entity(accepted.server)
                    .insert(ServerFailure(error.clone()));
                commands.trigger(OperationCompleted {
                    entity: operation,
                    result: Err(error),
                });
            }
        }
        commands.entity(operation).remove::<ServerCompleted>();
    }
}

fn cancel_start(
    servers: Query<(Entity, &Starting), With<Server>>,
    operations: Query<&OperationState>,
    mut commands: Commands,
) {
    for (server, starting) in &servers {
        if operations
            .get(starting.0)
            .is_ok_and(|state| matches!(state, OperationState::Pending | OperationState::Running))
        {
            continue;
        }
        commands.entity(server).remove::<Starting>();
    }
}

fn poll(
    transports: Query<(Entity, &Transport), With<Server>>,
    documents: Query<(Entity, &ChildOf), With<Document>>,
    mut commands: Commands,
) {
    for (server, transport) in &transports {
        if !transport.exited() {
            continue;
        }
        for (document, parent) in &documents {
            if parent.parent() == server {
                commands.entity(document).despawn();
            }
        }
        commands
            .entity(server)
            .remove::<Transport>()
            .insert(ServerFailure("language server exited".into()));
    }
}

fn retire(
    documents: Query<(Entity, &DocumentConnection, &ChildOf), With<Document>>,
    transports: Query<&Transport, With<Server>>,
    mut commands: Commands,
) {
    for (entity, connection, parent) in &documents {
        let identity = transports
            .get(parent.parent())
            .ok()
            .map(Transport::identity);
        if connection.0 != identity {
            commands.entity(entity).despawn();
        }
    }
}
