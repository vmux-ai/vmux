use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use vmux_ecs::record::{Receipt, Record};
use vmux_ecs::{
    BlockingCancellation, Job, JobOwner, JobPlugin, JobRequest, JobResult, JobSet, Operation,
};

use crate::{
    Commit, DirectoryObservation, Loaded, Location, ReadCompleted, ReadRequest, Root, Set,
    WriteCompleted, WriteFailure, WriteRequest,
};

pub struct Plugin;

impl bevy_app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            JobPlugin::<ReadJob>::default(),
            JobPlugin::<WriteJob>::default(),
        ))
        .add_systems(Update, (read, write).in_set(Set::Dispatch))
        .add_systems(Update, (receive_read, receive_write).in_set(JobSet::Apply));
    }
}

#[derive(Clone)]
struct ReadJob {
    data: PathBuf,
}

impl Job for ReadJob {
    type Output = Result<Loaded, String>;

    fn run(self) -> Self::Output {
        Database::read(&self.data)
    }
}

#[derive(Clone)]
struct WriteJob {
    data: PathBuf,
    request: WriteRequest,
}

impl Job for WriteJob {
    type Output = Result<Commit, WriteFailure>;

    fn run(self) -> Self::Output {
        Database::write(&self.data, &self.request)
    }
}

struct Database;

impl Database {
    fn connect(data: &Path) -> Result<Connection, String> {
        if BlockingCancellation::canceled() {
            return Err("account IO canceled".into());
        }
        std::fs::create_dir_all(data).map_err(|error| error.to_string())?;
        let connection =
            Connection::open(data.join("accounts.sqlite")).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(Duration::from_millis(500))
            .map_err(|error| error.to_string())?;
        connection.execute_batch(
            "pragma foreign_keys = on;
             create table if not exists store_meta (id integer primary key check (id = 1), generation integer not null check (generation >= 0));
             insert or ignore into store_meta values (1, 0);
             create table if not exists records (id text primary key, value text not null);
             create table if not exists operations (id text primary key, intent text not null, generation integer not null, receipt text not null);"
        ).map_err(|error| error.to_string())?;
        Ok(connection)
    }

    fn read(data: &Path) -> Result<Loaded, String> {
        let mut connection = Self::connect(data)?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let generation = transaction
            .query_row(
                "select generation from store_meta where id = 1",
                [],
                |row| row.get::<_, u64>(0),
            )
            .map_err(|error| error.to_string())?;
        let mut statement = transaction
            .prepare("select value from records order by id")
            .map_err(|error| error.to_string())?;
        let mut rows = statement.query([]).map_err(|error| error.to_string())?;
        let mut records = Vec::new();
        let mut bytes = 0usize;
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            if BlockingCancellation::canceled() {
                return Err("account IO canceled".into());
            }
            let value: String = row.get(0).map_err(|error| error.to_string())?;
            bytes = bytes
                .checked_add(value.len())
                .ok_or("account records exceed limit")?;
            if bytes > 16 * 1024 * 1024 || records.len() >= 4096 {
                return Err("account records exceed limit".into());
            }
            records
                .push(serde_json::from_str::<Record>(&value).map_err(|error| error.to_string())?);
        }
        drop(rows);
        drop(statement);
        transaction.commit().map_err(|error| error.to_string())?;
        let mut directories = Vec::new();
        let entries = match std::fs::read_dir(data.join("profiles")) {
            Ok(entries) => Some(entries),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        if let Some(entries) = entries {
            for entry in entries {
                if BlockingCancellation::canceled() {
                    return Err("account IO canceled".into());
                }
                let entry = entry.map_err(|error| error.to_string())?;
                if !entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_dir()
                {
                    continue;
                }
                if directories.len() >= 4096 {
                    return Err("profile directory count exceeds limit".into());
                }
                let id = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "profile directory identity is not UTF-8")?;
                let path = entry.path().join("display_name");
                let name = match path.symlink_metadata() {
                    Ok(metadata) => {
                        if !metadata.is_file()
                            || metadata.file_type().is_symlink()
                            || metadata.len() > 8192
                        {
                            return Err("invalid legacy profile name file".into());
                        }
                        Some(std::fs::read_to_string(path).map_err(|error| error.to_string())?)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.to_string()),
                };
                directories.push(DirectoryObservation { id, name });
            }
        }
        Ok(Loaded {
            generation,
            records,
            directories,
        })
    }

    fn write(data: &Path, request: &WriteRequest) -> Result<Commit, WriteFailure> {
        let mut connection = Self::connect(data).map_err(WriteFailure::Storage)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        let previous = transaction
            .query_row(
                "select intent, generation, receipt from operations where id = ?1",
                [request.operation_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        if let Some((intent, generation, value)) = previous {
            if intent != request.intent {
                return Err(WriteFailure::Conflict);
            }
            let receipt: Receipt = serde_json::from_str(&value)
                .map_err(|error| WriteFailure::Storage(error.to_string()))?;
            return Ok(Commit {
                generation,
                receipt,
                replayed: true,
            });
        }
        let current = transaction
            .query_row(
                "select generation from store_meta where id = 1",
                [],
                |row| row.get::<_, u64>(0),
            )
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        if current != request.generation {
            return Err(WriteFailure::Conflict);
        }
        let generation = current
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or_else(|| WriteFailure::Storage("account store generation exhausted".into()))?;
        let mut encoded = Vec::new();
        let mut receipt = None;
        let mut bytes = 0usize;
        if request.records.len() > 4096 || request.operation_id.is_nil() {
            return Err(WriteFailure::Storage("invalid account write".into()));
        }
        for record in &request.records {
            if !record.is_valid() {
                return Err(WriteFailure::Storage("invalid account record".into()));
            }
            let value = serde_json::to_string(record)
                .map_err(|error| WriteFailure::Storage(error.to_string()))?;
            bytes += value.len();
            if bytes > 16 * 1024 * 1024 {
                return Err(WriteFailure::Storage("account records exceed limit".into()));
            }
            if record.id == request.target_id {
                receipt = Some(Receipt {
                    operation_id: request.operation_id,
                    entity_id: record.id,
                    revision: record.revision,
                });
            }
            encoded.push((record.id.to_string(), value));
        }
        let receipt = receipt
            .ok_or_else(|| WriteFailure::Storage("account write target is missing".into()))?;
        let mut directory = match &request.directory {
            Some(id) => NewDirectory::create(data, id, request.exclusive)?,
            None => NewDirectory { created: None },
        };
        transaction
            .execute("delete from records", [])
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        for (id, value) in encoded {
            if BlockingCancellation::canceled() {
                return Err(WriteFailure::Storage("account IO canceled".into()));
            }
            transaction
                .execute(
                    "insert into records (id, value) values (?1, ?2)",
                    params![id, value],
                )
                .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        }
        let value = serde_json::to_string(&receipt)
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        transaction
            .execute(
                "insert into operations (id, intent, generation, receipt) values (?1, ?2, ?3, ?4)",
                params![
                    request.operation_id.to_string(),
                    request.intent,
                    generation,
                    value
                ],
            )
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        transaction
            .execute(
                "update store_meta set generation = ?1 where id = 1",
                [generation],
            )
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        if BlockingCancellation::canceled() {
            return Err(WriteFailure::Storage("account IO canceled".into()));
        }
        transaction
            .commit()
            .map_err(|error| WriteFailure::Storage(error.to_string()))?;
        directory.created = None;
        Ok(Commit {
            generation,
            receipt,
            replayed: false,
        })
    }
}

struct NewDirectory {
    created: Option<PathBuf>,
}

impl NewDirectory {
    fn create(data: &Path, id: &str, exclusive: bool) -> Result<Self, WriteFailure> {
        if id.is_empty()
            || !id.chars().all(|character| {
                character.is_ascii_alphanumeric() || character == '-' || character == '_'
            })
        {
            return Err(WriteFailure::Storage(
                "invalid profile directory identity".into(),
            ));
        }
        if BlockingCancellation::canceled() {
            return Err(WriteFailure::Storage("account IO canceled".into()));
        }
        let root = data.join("profiles");
        std::fs::create_dir_all(&root).map_err(|error| WriteFailure::Storage(error.to_string()))?;
        let path = root.join(id);
        match std::fs::create_dir(&path) {
            Ok(()) => Ok(Self {
                created: Some(path),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && !exclusive => {
                let metadata = path
                    .symlink_metadata()
                    .map_err(|error| WriteFailure::Storage(error.to_string()))?;
                if metadata.is_dir() && !metadata.file_type().is_symlink() {
                    Ok(Self { created: None })
                } else {
                    Err(WriteFailure::Storage("invalid profile directory".into()))
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(WriteFailure::Conflict)
            }
            Err(error) => Err(WriteFailure::Storage(error.to_string())),
        }
    }
}

impl Drop for NewDirectory {
    fn drop(&mut self) {
        if let Some(path) = &self.created {
            let _ = std::fs::remove_dir(path);
        }
    }
}

fn read(
    mut requests: MessageReader<ReadRequest>,
    locations: Query<&Location, With<Root>>,
    active: Query<&ChildOf, With<Operation>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(parent) = active.get(request.entity) else {
            continue;
        };
        if parent.parent() != request.root {
            continue;
        }
        let Ok(location) = locations.get(request.root) else {
            continue;
        };
        commands.entity(request.entity).insert(JobRequest(ReadJob {
            data: location.0.clone(),
        }));
    }
}

fn write(
    mut requests: MessageReader<WriteRequest>,
    locations: Query<&Location, With<Root>>,
    active: Query<&ChildOf, With<Operation>>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(parent) = active.get(request.entity) else {
            continue;
        };
        if parent.parent() != request.root {
            continue;
        }
        let Ok(location) = locations.get(request.root) else {
            continue;
        };
        commands.entity(request.entity).insert(JobRequest(WriteJob {
            data: location.0.clone(),
            request: request.clone(),
        }));
    }
}

fn receive_read(
    mut results: Results<ReadJob>,
    mut replies: MessageWriter<ReadCompleted>,
    mut commands: Commands,
) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("read result consumed".into()));
        let value = match value {
            Ok(value) => value,
            Err(error) => Err(error),
        };
        replies.write(ReadCompleted {
            entity,
            result: Some(value),
        });
        commands
            .entity(entity)
            .remove::<JobResult<ReadJob>>()
            .remove::<JobRequest<ReadJob>>();
    }
}

fn receive_write(
    mut results: Results<WriteJob>,
    mut replies: MessageWriter<WriteCompleted>,
    mut commands: Commands,
) {
    for (entity, mut result, owner, parent) in &mut results {
        if !owner.accepts(parent) {
            continue;
        }
        let value = std::mem::replace(&mut result.0, Err("write result consumed".into()));
        let value = match value {
            Ok(value) => value,
            Err(error) => Err(WriteFailure::Storage(error)),
        };
        replies.write(WriteCompleted {
            entity,
            result: Some(value),
        });
        commands
            .entity(entity)
            .remove::<JobResult<WriteJob>>()
            .remove::<JobRequest<WriteJob>>();
    }
}

type Results<'w, 's, J> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut JobResult<J>,
        &'static JobOwner<J>,
        Option<&'static ChildOf>,
    ),
>;
