use std::io;
use std::time::{Duration, Instant};

use bevy_app::{App, Plugin, Update};
use bevy_ecs::name::Name;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryData;
use bevy_ecs::system::SystemParam;

use crate::docker::{Client, Condition, Container, Dependency, Operation, RunState};

struct BatchPlugin;

impl Plugin for BatchPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Container>().add_systems(
            Update,
            (
                ingest, link, select, expand, order, inspect, advance, apply, finish,
            )
                .chain(),
        );
    }
}

pub(crate) struct Runtime;

impl Runtime {
    pub(crate) fn run(client: Client, ids: &[String]) -> io::Result<()> {
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
struct ReadError(String);

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
                let mut target = None;
                let mut earliest = usize::MAX;
                for candidate in &rows.entries {
                    if candidate.parent.parent() == root
                        && candidate.project.0 == row.project.0
                        && candidate.service.0 == dependency.service
                        && candidate.source.0 < earliest
                    {
                        target = Some(candidate.entity);
                        earliest = candidate.source.0;
                    }
                }
                if let Some(target) = target {
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

fn inspect(roots: Roots, rows: Rows, mut commands: Commands) {
    let Ok((root, stage, endpoint, _, _, _)) = roots.single() else {
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
    for (target, _) in &links.0 {
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
                entity
                    .insert(ReadError(error.to_string()))
                    .remove::<Observation>();
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
            Err(format!("dependency {name}: {}", error.0))
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
                commands.entity(candidate).insert((
                    Finished,
                    Failure(format!(
                        "dependency {name}: container dependency did not become ready"
                    )),
                ));
                return;
            }
            Err(error) => {
                commands
                    .entity(candidate)
                    .insert((Finished, Failure(error)));
                return;
            }
        }
    }
    commands.entity(candidate).insert(Effect);
}

fn apply(
    roots: Roots,
    effects: Query<(Entity, &Identifier, &Operation, &ChildOf), With<Effect>>,
    mut commands: Commands,
) {
    let Ok((root, stage, endpoint, _, _, _)) = roots.single() else {
        return;
    };
    if *stage != Stage::Execute {
        return;
    }
    for (entity, id, operation, parent) in &effects {
        if parent.parent() != root {
            continue;
        }
        if let Err(error) = endpoint.0.change(&id.0, *operation) {
            commands.entity(entity).insert(Failure(error.to_string()));
        }
        commands.entity(entity).remove::<Effect>().insert(Finished);
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
