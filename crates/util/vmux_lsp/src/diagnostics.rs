use std::path::{Path, PathBuf};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use uuid::Uuid;
use vmux_ecs::Order;

use crate::{Lookup, Set};

pub(crate) struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, receive.after(Set::Servers).before(Set::Documents))
            .add_systems(Update, (prune, apply).chain().in_set(Set::Diagnostics));
    }
}

pub struct ReceivedDiagnostics {
    pub path: PathBuf,
    pub diagnostics: Vec<lsp_types::Diagnostic>,
    pub version: Option<i32>,
    pub server: Option<Entity>,
    pub transport: Option<Uuid>,
}

#[derive(Clone)]
pub struct DiagnosticsSender {
    sender: crossbeam_channel::Sender<ReceivedDiagnostics>,
    server: Option<Entity>,
    transport: Option<Uuid>,
}

impl DiagnosticsSender {
    pub fn channel() -> (Self, DiagnosticsInbox) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                sender,
                server: None,
                transport: None,
            },
            DiagnosticsInbox(receiver),
        )
    }

    pub fn scoped(&self, server: Entity) -> Self {
        Self {
            sender: self.sender.clone(),
            server: Some(server),
            transport: self.transport,
        }
    }

    pub fn with_transport(mut self, transport: Uuid) -> Self {
        self.transport = Some(transport);
        self
    }

    pub fn send(&self, mut diagnostics: ReceivedDiagnostics) {
        diagnostics.server = self.server;
        diagnostics.transport = self.transport;
        let _ = self.sender.send(diagnostics);
    }
}

impl Default for DiagnosticsSender {
    fn default() -> Self {
        Self::channel().0
    }
}

#[derive(Component, Clone)]
pub struct DiagnosticsInbox(pub crossbeam_channel::Receiver<ReceivedDiagnostics>);

impl DiagnosticsInbox {
    pub fn drain(&self) -> impl Iterator<Item = ReceivedDiagnostics> + '_ {
        self.0.try_iter()
    }
}

#[derive(Component)]
#[component(immutable)]
pub struct DiagnosticPath(pub PathBuf);

#[derive(Component, Clone)]
pub(crate) struct Ticket {
    document: Entity,
    server: Entity,
    version: i32,
    transport: Uuid,
    alternate: Option<PathBuf>,
}

impl Lookup<'_, '_> {
    fn capture_ticket(&self, server: Entity, path: &Path, transport: Uuid) -> Option<Ticket> {
        if !self.transport_current(server, transport) {
            return None;
        }
        for exact in [true, false] {
            let mut found = None;
            for (entity, candidate, document, parent, connection) in &self.documents {
                if parent.parent() != server || connection.0 != Some(transport) {
                    continue;
                }
                let matches = if exact {
                    candidate.0 == path
                } else {
                    self.diagnostic_paths
                        .get(entity)
                        .is_ok_and(|alternate| alternate.0 == path)
                };
                if !matches {
                    continue;
                }
                if found.is_some() {
                    return None;
                }
                found = Some(Ticket {
                    document: entity,
                    server,
                    version: document.version,
                    transport,
                    alternate: (!exact).then(|| path.to_path_buf()),
                });
            }
            if found.is_some() {
                return found;
            }
        }
        None
    }

    pub(crate) fn ticket_current(&self, ticket: &Ticket) -> bool {
        let Ok((_, _, document, parent, connection)) = self.documents.get(ticket.document) else {
            return false;
        };
        if document.version != ticket.version
            || parent.parent() != ticket.server
            || connection.0 != Some(ticket.transport)
            || !self.transport_current(ticket.server, ticket.transport)
        {
            return false;
        }
        let Some(path) = &ticket.alternate else {
            return true;
        };
        let Some(current) = self.capture_ticket(ticket.server, path, ticket.transport) else {
            return false;
        };
        current.document == ticket.document
    }
}

#[derive(Component)]
pub struct Diagnostics(Vec<lsp_types::Diagnostic>);

impl Diagnostics {
    pub fn items(&self) -> &[lsp_types::Diagnostic] {
        &self.0
    }
}

#[derive(Component)]
struct Pending {
    #[allow(unknown_lints, vmux_ecs_ownership)]
    ticket: Ticket,
    version: Option<i32>,
    diagnostics: Vec<lsp_types::Diagnostic>,
}

fn receive(inboxes: Query<&DiagnosticsInbox>, lookup: Lookup, mut commands: Commands) {
    let mut sequence = 0_u32;
    for inbox in &inboxes {
        for input in inbox.drain() {
            let (Some(server), Some(transport)) = (input.server, input.transport) else {
                continue;
            };
            let Some(ticket) = lookup.capture_ticket(server, &input.path, transport) else {
                continue;
            };
            commands.spawn((
                Name::new("Language diagnostics"),
                Pending {
                    ticket,
                    version: input.version,
                    diagnostics: input.diagnostics,
                },
                Order(sequence),
                ChildOf(server),
            ));
            sequence = sequence.saturating_add(1);
        }
    }
}

fn prune(tickets: Query<(Entity, &Ticket)>, lookup: Lookup, mut commands: Commands) {
    for (entity, ticket) in &tickets {
        if !lookup.ticket_current(ticket) {
            commands.entity(entity).remove::<(Diagnostics, Ticket)>();
        }
    }
}

fn apply(
    mut pending: Query<(Entity, &mut Pending, &Order)>,
    lookup: Lookup,
    mut commands: Commands,
) {
    let mut ordered = Vec::new();
    for (entity, _, order) in &pending {
        ordered.push((order.0, entity));
    }
    ordered.sort_by_key(|(order, _)| *order);
    for (_, entity) in ordered {
        let Ok((_, mut pending, _)) = pending.get_mut(entity) else {
            continue;
        };
        if lookup.ticket_current(&pending.ticket)
            && pending
                .version
                .is_none_or(|version| version == pending.ticket.version)
        {
            commands.entity(pending.ticket.document).insert((
                pending.ticket.clone(),
                Diagnostics(std::mem::take(&mut pending.diagnostics)),
            ));
        }
        commands.entity(entity).despawn();
    }
}
