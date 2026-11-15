use std::io;
use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryData;
use bevy_ecs::system::SystemParam;

use crate::Owner;
use crate::docker::{Client, Condition, Container, Dependency, Operation, RunState};

struct BatchPlugin;

impl Plugin for BatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Container>().add_systems(
            Update,
            (
                cancel, ingest, link, select, expand, order, inspect, advance, apply, finish,
            )
                .chain(),
        );
    }
}

pub(crate) struct Runtime;

impl Runtime {
    pub(crate) fn run(client: Client, ids: &[String], owner: Option<&Owner>) -> io::Result<()> {
        if let Some(owner) = owner {
            owner.check()?;
        }
        let containers = client.list()?;
        let mut app = App::new();
        app.add_plugins(BatchPlugin);
        let root = app
            .world_mut()
            .spawn((
                Name::new("Docker startup"),
                Scope,
                Stage::Ingest,
                Endpoint(client),
                Requested(ids.to_vec()),
                Deadline(Instant::now() + Duration::from_secs(120)),
                Sequence::default(),
            ))
            .id();
        if let Some(owner) = owner {
            app.world_mut()
                .entity_mut(root)
                .insert(Cancellation(owner.clone()));
        }
        app.world_mut().write_message_batch(containers);
        let mut finished = app.world_mut().query_filtered::<Entity, With<Finished>>();
        let mut completed = 0;
        loop {
            app.update();
            if let Some(completion) = app.world().get::<Completion>(root) {
                return completion
                    .0
                    .clone()
                    .map_err(|(kind, error)| io::Error::new(kind, error));
            }
            let next = finished.iter(app.world()).count();
            if next == completed {
                std::thread::sleep(Duration::from_millis(50));
            }
            completed = next;
        }
    }
}

#[derive(Component)]
struct Scope;

#[derive(Component)]
struct Cancellation(Owner);

fn cancel(
    mut roots: Query<(Entity, &Cancellation, &mut Stage), With<Scope>>,
    effects: Query<(Entity, &ChildOf), With<Effect>>,
    mut commands: Commands,
) {
    for (root, cancellation, mut stage) in &mut roots {
        if *stage == Stage::Done {
            continue;
        }
        if let Err(error) = cancellation.0.check() {
            *stage = Stage::Done;
            commands
                .entity(root)
                .insert(Completion(Err((error.kind(), error.to_string()))));
            for (entity, parent) in &effects {
                if parent.parent() == root {
                    commands.entity(entity).remove::<(Effect, RetryAt)>();
                }
            }
        }
    }
}

#[derive(Component, PartialEq, Eq)]
enum Stage {
    Ingest,
    Link,
    Select,
    Expand,
    Order,
    Execute,
    Done,
}

#[derive(Component)]
struct Endpoint(Client);

#[derive(Component)]
struct Requested(Vec<String>);

#[derive(Component)]
struct Deadline(Instant);

#[derive(Component, Default)]
struct Sequence {
    position: usize,
    rank: usize,
}

#[derive(Component)]
struct Completion(std::result::Result<(), (io::ErrorKind, String)>);

#[derive(Component)]
struct Entry;

#[derive(Component)]
struct Identifier(String);

#[derive(Component)]
struct Project(String);

#[derive(Component)]
struct Service(String);

#[derive(Component)]
struct SourceOrder(usize);

#[derive(Component)]
struct Incoming(Vec<Dependency>);

#[derive(Component)]
struct Links(Vec<(Entity, Condition)>);

#[derive(Component)]
struct Position(usize);

#[derive(Component)]
struct Rank(usize);

#[derive(Component)]
struct Finished;

#[derive(Component)]
struct Effect;

#[derive(Component)]
struct Failure(String);

#[derive(Component)]
struct ReadError {
    kind: io::ErrorKind,
    message: String,
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl ReadError {
    fn retryable(&self) -> bool {
        matches!(
            self.kind,
            io::ErrorKind::WouldBlock
                | io::ErrorKind::TimedOut
                | io::ErrorKind::Interrupted
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionRefused
                | io::ErrorKind::UnexpectedEof
        )
    }
}

#[derive(Component)]
struct RetryAt(Instant);

#[derive(Component)]
struct Poll {
    candidate: Entity,
    next: Instant,
}

#[derive(Component)]
struct Observation(RunState);

impl Observation {
    fn assess(&self, condition: Condition) -> Result<bool, String> {
        let state = &self.0;
        let ready = match condition {
            Condition::Started => state.running,
            Condition::Healthy => {
                state.running
                    && state
                        .health
                        .as_ref()
                        .is_some_and(|health| health.status == "healthy")
            }
            Condition::Completed => {
                !state.running && state.status == "exited" && state.exit_code == 0
            }
        };
        if ready {
            return Ok(true);
        }
        if !state.running && matches!(state.status.as_str(), "exited" | "dead") {
            return Err(format!(
                "container stopped with exit code {}",
                state.exit_code
            ));
        }
        if condition == Condition::Healthy
            && state
                .health
                .as_ref()
                .is_none_or(|health| health.status == "unhealthy")
        {
            return Err("container healthcheck is missing or unhealthy".into());
        }
        Ok(false)
    }
}

type Roots<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Stage,
        &'static Endpoint,
        &'static Requested,
        &'static Deadline,
        &'static mut Sequence,
    ),
    With<Scope>,
>;

#[derive(QueryData)]
struct EntryView {
    entity: Entity,
    id: &'static Identifier,
    name: &'static Name,
    project: &'static Project,
    service: &'static Service,
    source: &'static SourceOrder,
    incoming: Option<&'static Incoming>,
    links: Option<&'static Links>,
    position: Option<&'static Position>,
    rank: Option<&'static Rank>,
    finished: Option<&'static Finished>,
    parent: &'static ChildOf,
}

type Entries<'w, 's> = Query<'w, 's, EntryView, With<Entry>>;

type Observations<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static Observation>,
        Option<&'static ReadError>,
        Option<&'static Poll>,
        Option<&'static Failure>,
    ),
    With<Entry>,
>;

type Effects<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Identifier,
        &'static Operation,
        &'static ChildOf,
        Option<&'static RetryAt>,
    ),
    With<Effect>,
>;

#[derive(SystemParam)]
struct Rows<'w, 's> {
    entries: Entries<'w, 's>,
    observations: Observations<'w, 's>,
}

impl Rows<'_, '_> {
    fn candidate(&self, root: Entity) -> Option<Entity> {
        let mut selected = None;
        let mut lowest = usize::MAX;
        for row in &self.entries {
            if row.parent.parent() != root || row.position.is_none() || row.finished.is_some() {
                continue;
            }
            let Some(rank) = row.rank else {
                continue;
            };
            if rank.0 < lowest {
                selected = Some(row.entity);
                lowest = rank.0;
            }
        }
        selected
    }
}

fn ingest(mut containers: MessageReader<Container>, mut roots: Roots, mut commands: Commands) {
    let Ok((root, mut stage, _, _, _, _)) = roots.single_mut() else {
        return;
    };
    if *stage != Stage::Ingest {
        return;
    }
    for (position, container) in containers.read().enumerate() {
        commands.spawn((
            Name::new(container.name.clone()),
            Entry,
            Identifier(container.id.clone()),
            Project(container.project.clone()),
            Service(container.service.clone()),
            SourceOrder(position),
            Incoming(container.dependencies.clone()),
            ChildOf(root),
        ));
    }
    *stage = Stage::Link;
}

fn link(mut roots: Roots, rows: Rows, mut commands: Commands) {
    let Ok((root, mut stage, _, _, _, _)) = roots.single_mut() else {
        return;
    };
    if *stage != Stage::Link {
        return;
    }
    for row in &rows.entries {
        if row.parent.parent() != root {
            continue;
        }
        let Some(incoming) = row.incoming else {
            continue;
        };
        let mut links = Vec::new();
        if !row.project.0.is_empty() {
            for dependency in &incoming.0 {
                let mut targets = Vec::new();
                for candidate in &rows.entries {
                    if candidate.parent.parent() == root
                        && candidate.project.0 == row.project.0
                        && candidate.service.0 == dependency.service
                    {
                        targets.push((candidate.source.0, candidate.entity));
                    }
                }
                if targets.is_empty() {
                    commands.entity(row.entity).insert(Failure(format!(
                        "{}: required dependency {} is missing from project {}",
                        row.name, dependency.service, row.project.0
                    )));
                }
                targets.sort_by_key(|(source, _)| *source);
                for (_, target) in targets {
                    links.push((target, dependency.condition));
                }
            }
        }
        commands
            .entity(row.entity)
            .remove::<Incoming>()
            .insert(Links(links));
    }
    *stage = Stage::Select;
}

fn select(mut roots: Roots, rows: Rows, mut commands: Commands) {
    let Ok((root, mut stage, _, requested, _, mut sequence)) = roots.single_mut() else {
        return;
    };
    if *stage != Stage::Select {
        return;
    }
    let mut selected = Vec::new();
    for id in &requested.0 {
        let mut found = None;
        for row in &rows.entries {
            if row.parent.parent() == root && row.id.0 == *id {
                found = Some(row.entity);
                break;
            }
        }
        let Some(entity) = found else {
            commands.entity(root).insert(Completion(Err((
                io::ErrorKind::NotFound,
                format!("container {id} is no longer present"),
            ))));
            *stage = Stage::Done;
            return;
        };
        if !selected.contains(&entity) {
            selected.push(entity);
        }
    }
    for entity in selected {
        commands
            .entity(entity)
            .insert((Operation::Start, Position(sequence.position)));
        sequence.position += 1;
    }
    *stage = Stage::Expand;
}

fn expand(mut roots: Roots, rows: Rows, mut commands: Commands) {
    let Ok((root, mut stage, _, _, _, mut sequence)) = roots.single_mut() else {
        return;
    };
    if *stage != Stage::Expand {
        return;
    }
    let mut selected = Vec::new();
    for row in &rows.entries {
        if row.parent.parent() == root
            && let Some(position) = row.position
        {
            selected.push((position.0, row.entity));
        }
    }
    selected.sort_by_key(|(position, _)| *position);
    let mut entities = Vec::new();
    for (_, entity) in selected {
        entities.push(entity);
    }
    let mut index = 0;
    while index < entities.len() {
        let entity = entities[index];
        index += 1;
        let Ok(row) = rows.entries.get(entity) else {
            continue;
        };
        let Some(links) = row.links else {
            continue;
        };
        for (target, _) in &links.0 {
            if !entities.contains(target) {
                entities.push(*target);
                commands
                    .entity(*target)
                    .insert((Operation::Start, Position(sequence.position)));
                sequence.position += 1;
            }
        }
    }
    *stage = Stage::Order;
}

fn order(mut roots: Roots, rows: Rows, mut commands: Commands) {
    let Ok((root, mut stage, _, _, _, mut sequence)) = roots.single_mut() else {
        return;
    };
    if *stage != Stage::Order {
        return;
    }
    let mut pending = Vec::new();
    for row in &rows.entries {
        if row.parent.parent() == root
            && row.rank.is_none()
            && let Some(position) = row.position
        {
            if let Ok((_, _, _, Some(failure))) = rows.observations.get(row.entity) {
                commands.entity(root).insert(Completion(Err((
                    io::ErrorKind::NotFound,
                    failure.0.clone(),
                ))));
                *stage = Stage::Done;
                return;
            }
            pending.push((position.0, row.entity));
        }
    }
    pending.sort_by_key(|(position, _)| *position);
    while !pending.is_empty() {
        let mut selected = None;
        for (index, (_, entity)) in pending.iter().enumerate() {
            let Ok(row) = rows.entries.get(*entity) else {
                continue;
            };
            let Some(links) = row.links else {
                continue;
            };
            let mut blocked = false;
            for (target, _) in &links.0 {
                if pending.iter().any(|(_, candidate)| candidate == target) {
                    blocked = true;
                    break;
                }
            }
            if !blocked {
                selected = Some(index);
                break;
            }
        }
        let Some(index) = selected else {
            commands.entity(root).insert(Completion(Err((
                io::ErrorKind::InvalidInput,
                "container group has a dependency cycle".into(),
            ))));
            *stage = Stage::Done;
            return;
        };
        let (_, entity) = pending.remove(index);
        commands.entity(entity).insert(Rank(sequence.rank));
        sequence.rank += 1;
    }
    *stage = Stage::Execute;
}

fn inspect(
    roots: Roots,
    rows: Rows,
    cancellations: Query<&Cancellation, With<Scope>>,
    mut commands: Commands,
) {
    let Ok((root, stage, endpoint, _, deadline, _)) = roots.single() else {
        return;
    };
    if *stage != Stage::Execute {
        return;
    }
    let Some(candidate) = rows.candidate(root) else {
        return;
    };
    let Ok(row) = rows.entries.get(candidate) else {
        return;
    };
    let Some(links) = row.links else {
        return;
    };
    let cancellation = cancellations.get(root).ok();
    for (target, _) in &links.0 {
        if Instant::now() >= deadline.0
            || cancellation.is_some_and(|cancellation| cancellation.0.check().is_err())
        {
            return;
        }
        let Ok((_, _, poll, failure)) = rows.observations.get(*target) else {
            continue;
        };
        if failure.is_some()
            || poll.is_some_and(|poll| poll.candidate == candidate && Instant::now() < poll.next)
        {
            continue;
        }
        let Ok(target_row) = rows.entries.get(*target) else {
            continue;
        };
        let entity = &mut commands.entity(*target);
        match endpoint.0.inspect(&target_row.id.0) {
            Ok(state) => {
                entity.insert(Observation(state)).remove::<ReadError>();
            }
            Err(error) => {
                let error = ReadError::from(error);
                entity.remove::<Observation>();
                if error.retryable() {
                    entity.remove::<ReadError>();
                } else {
                    entity.insert(error);
                }
            }
        }
        entity.insert(Poll {
            candidate,
            next: Instant::now() + Duration::from_millis(200),
        });
    }
}

fn advance(roots: Roots, rows: Rows, mut commands: Commands) {
    let Ok((root, stage, _, _, deadline, _)) = roots.single() else {
        return;
    };
    if *stage != Stage::Execute {
        return;
    }
    let Some(candidate) = rows.candidate(root) else {
        return;
    };
    let Ok(row) = rows.entries.get(candidate) else {
        return;
    };
    commands.entity(candidate).remove::<Effect>();
    let Some(links) = row.links else {
        return;
    };
    for (target, condition) in &links.0 {
        let Ok((observation, read_error, _, failure)) = rows.observations.get(*target) else {
            continue;
        };
        let Ok(target_row) = rows.entries.get(*target) else {
            continue;
        };
        let name = target_row.name;
        let result = if failure.is_some() {
            Err(format!("dependency {name} failed to start"))
        } else if let Some(error) = read_error {
            Err(format!("dependency {name}: {}", error.message))
        } else if let Some(observation) = observation {
            observation
                .assess(*condition)
                .map_err(|error| format!("dependency {name}: {error}"))
        } else {
            Ok(false)
        };
        match result {
            Ok(true) => {}
            Ok(false) if Instant::now() < deadline.0 => return,
            Ok(false) => {
                commands
                    .entity(candidate)
                    .insert((
                        Finished,
                        Failure(format!(
                            "dependency {name}: container dependency did not become ready"
                        )),
                    ))
                    .remove::<RetryAt>();
                return;
            }
            Err(error) => {
                commands
                    .entity(candidate)
                    .insert((Finished, Failure(error)))
                    .remove::<RetryAt>();
                return;
            }
        }
    }
    commands.entity(candidate).insert(Effect);
}

fn apply(
    roots: Roots,
    effects: Effects,
    cancellations: Query<&Cancellation, With<Scope>>,
    mut commands: Commands,
) {
    let Ok((root, stage, endpoint, _, deadline, _)) = roots.single() else {
        return;
    };
    if *stage != Stage::Execute {
        return;
    }
    let cancellation = cancellations.get(root).ok();
    for (entity, id, operation, parent, retry) in &effects {
        if parent.parent() != root {
            continue;
        }
        if cancellation.is_some_and(|cancellation| cancellation.0.check().is_err()) {
            return;
        }
        if Instant::now() >= deadline.0 {
            commands.entity(entity).remove::<Effect>().insert((
                Finished,
                Failure("container start did not complete before its deadline".into()),
            ));
            continue;
        }
        if retry.is_some_and(|retry| Instant::now() < retry.0) {
            continue;
        }
        if let Err(error) = endpoint.0.change(&id.0, *operation) {
            let error = ReadError::from(error);
            if matches!(operation, Operation::Start) && error.retryable() {
                commands
                    .entity(entity)
                    .insert(RetryAt(Instant::now() + Duration::from_millis(200)));
                continue;
            }
            commands.entity(entity).insert(Failure(error.message));
        }
        commands
            .entity(entity)
            .remove::<(Effect, RetryAt)>()
            .insert(Finished);
    }
}

fn finish(mut roots: Roots, rows: Rows, mut commands: Commands) {
    let Ok((root, mut stage, _, _, _, _)) = roots.single_mut() else {
        return;
    };
    if *stage != Stage::Execute || rows.candidate(root).is_some() {
        return;
    }
    let mut errors = Vec::new();
    for row in &rows.entries {
        if row.parent.parent() != root || row.position.is_none() {
            continue;
        }
        if let Ok((_, _, _, Some(failure))) = rows.observations.get(row.entity) {
            errors.push((
                row.rank.map_or(usize::MAX, |rank| rank.0),
                format!("{}: {}", row.name, failure.0),
            ));
        }
    }
    errors.sort_by_key(|(rank, _)| *rank);
    let result = if errors.is_empty() {
        Ok(())
    } else {
        let mut messages = Vec::new();
        for (_, error) in errors {
            messages.push(error);
        }
        Err((io::ErrorKind::Other, messages.join("\n")))
    };
    commands.entity(root).insert(Completion(result));
    *stage = Stage::Done;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_an_authorized_start_retires_its_effect_before_more_io() {
        let directory = tempfile::tempdir().unwrap();
        let owner = Owner::new(directory.path().join("profile"));
        let mut app = App::new();
        app.add_plugins(BatchPlugin);
        let root = app
            .world_mut()
            .spawn((
                Scope,
                Stage::Execute,
                Endpoint(Client::new(&directory.path().join("missing.sock"))),
                Requested(vec!["aaaaaaaaaaaa".into()]),
                Deadline(Instant::now() + Duration::from_secs(1)),
                Sequence::default(),
                Cancellation(owner.clone()),
            ))
            .id();
        let entry = app
            .world_mut()
            .spawn((
                Entry,
                Identifier("aaaaaaaaaaaa".into()),
                Name::new("account"),
                Project("vmux-cloud".into()),
                Service("account".into()),
                SourceOrder(0),
                Operation::Start,
                Position(0),
                Rank(0),
                Links(Vec::new()),
                ChildOf(root),
                Effect,
                RetryAt(Instant::now()),
            ))
            .id();
        owner.cancel();
        app.update();
        let completion = app.world().get::<Completion>(root).unwrap();
        let Err((kind, _)) = &completion.0 else {
            panic!("cancelled Start completed successfully");
        };
        assert_eq!(*kind, io::ErrorKind::Interrupted);
        assert!(app.world().get::<Effect>(entry).is_none());
        assert!(app.world().get::<RetryAt>(entry).is_none());
        app.update();
        assert_eq!(
            app.world()
                .get::<Completion>(root)
                .unwrap()
                .0
                .as_ref()
                .unwrap_err()
                .0,
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn a_cancelled_owner_refuses_inventory_io_before_creating_a_batch() {
        let directory = tempfile::tempdir().unwrap();
        let owner = Owner::new(directory.path().join("profile"));
        owner.cancel();
        let error = Client::new(&directory.path().join("missing.sock"))
            .start_many_owned(&["aaaaaaaaaaaa".into()], &owner)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn missing_required_dependency_is_refused_before_execution() {
        let mut app = App::new();
        app.add_plugins(BatchPlugin);
        let root = app
            .world_mut()
            .spawn((
                Scope,
                Stage::Ingest,
                Endpoint(Client::new(std::path::Path::new("/missing/docker.sock"))),
                Requested(vec!["aaaaaaaaaaaa".into()]),
                Deadline(Instant::now() + Duration::from_secs(1)),
                Sequence::default(),
            ))
            .id();
        app.world_mut().write_message(Container {
            id: "aaaaaaaaaaaa".into(),
            name: "app".into(),
            image: String::new(),
            state: "created".into(),
            status: String::new(),
            project: "stack".into(),
            service: "app".into(),
            dependencies: vec![Dependency {
                service: "db".into(),
                condition: Condition::Healthy,
            }],
            ports: Vec::new(),
        });
        app.update();
        let completion = app.world().get::<Completion>(root).unwrap();
        let Err((kind, message)) = &completion.0 else {
            panic!("missing required dependency was accepted");
        };
        assert_eq!(*kind, io::ErrorKind::NotFound);
        assert!(message.contains("required dependency db"));
        assert!(
            app.world_mut()
                .query::<&Effect>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
}
