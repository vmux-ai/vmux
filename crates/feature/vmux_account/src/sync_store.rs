use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use uuid::Uuid;
use vmux_ecs::record::{Receipt, Record};

use crate::sync::{Checkpoint, CheckpointChange, ExpectedScope, Origin, Pending};

pub(crate) struct Store;

#[derive(bevy_ecs::prelude::Component)]
pub(crate) struct Loaded {
    pub pending: Vec<(u32, Pending)>,
    #[allow(unknown_lints, vmux_ecs_ownership)]
    pub checkpoints: Vec<Checkpoint>,
    pub next: Option<u32>,
    pub through: u32,
}

impl Store {
    pub(crate) fn path(data: &Path) -> PathBuf {
        data.join("accounts.sqlite")
    }

    fn connect(data: &Path) -> Result<Connection, String> {
        let connection = Connection::open(Self::path(data)).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|error| error.to_string())?;
        Self::schema(&connection)?;
        Ok(connection)
    }

    pub(crate) fn schema(connection: &Connection) -> Result<(), String> {
        connection
            .execute_batch(
                "create table if not exists profile_outbox (
                sequence integer primary key autoincrement,
                id text unique not null,
                profile_id text not null,
                server text not null,
                account_id text not null,
                value text not null
            );
            create table if not exists profile_checkpoint (
                profile_id text not null, server text not null, account_id text not null,
                remote_revision integer not null, local_revision integer not null,
                deleted integer not null,
                primary key (profile_id, server, account_id)
            );",
            )
            .map_err(|error| error.to_string())
    }

    pub(crate) fn enqueue(transaction: &Transaction<'_>, pending: &Pending) -> Result<(), String> {
        let value = serde_json::to_string(pending).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "insert into profile_outbox (id, profile_id, server, account_id, value)
             values (?1, ?2, ?3, ?4, ?5) on conflict(id) do nothing",
                params![
                    pending.operation_id.to_string(),
                    pending.profile_id.to_string(),
                    pending.origin.server,
                    pending.origin.account.to_string(),
                    value
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn load(
        data: &Path,
        origin: &Origin,
        after: u32,
        through: Option<u32>,
    ) -> Result<Loaded, String> {
        let mut connection = Self::connect(data)?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let through = match through {
            Some(value) => value,
            None => transaction
                .query_row(
                    "select coalesce(max(sequence), 0) from profile_outbox where server=?1 and account_id=?2",
                    params![origin.server, origin.account.to_string()],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?,
        };
        let mut outbox = transaction.prepare(
            "select sequence, value from profile_outbox where server=?1 and account_id=?2 and sequence>?3 and sequence<=?4 order by sequence limit 17",
        ).map_err(|error| error.to_string())?;
        let mut rows = outbox
            .query(params![
                origin.server,
                origin.account.to_string(),
                after,
                through
            ])
            .map_err(|error| error.to_string())?;
        let mut pending = Vec::new();
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            let sequence = row.get(0).map_err(|error| error.to_string())?;
            let value: String = row.get(1).map_err(|error| error.to_string())?;
            pending.push((
                sequence,
                serde_json::from_str(&value).map_err(|error| error.to_string())?,
            ));
        }
        let next = if pending.len() > 16 {
            pending.pop();
            pending.last().map(|entry| entry.0)
        } else {
            None
        };
        let checkpoints = if next.is_none() {
            Self::checkpoints(&transaction, origin)?
        } else {
            Vec::new()
        };
        Ok(Loaded {
            pending,
            checkpoints,
            next,
            through,
        })
    }

    fn checkpoints(connection: &Connection, origin: &Origin) -> Result<Vec<Checkpoint>, String> {
        let mut state = connection.prepare(
            "select profile_id, remote_revision, local_revision, deleted from profile_checkpoint where server=?1 and account_id=?2 limit 4096",
        ).map_err(|error| error.to_string())?;
        let mut rows = state
            .query(params![origin.server, origin.account.to_string()])
            .map_err(|error| error.to_string())?;
        let mut checkpoints = Vec::new();
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            let id: String = row.get(0).map_err(|error| error.to_string())?;
            checkpoints.push(Checkpoint {
                profile_id: id.parse().map_err(|error: uuid::Error| error.to_string())?,
                remote_revision: row.get(1).map_err(|error| error.to_string())?,
                local_revision: row.get(2).map_err(|error| error.to_string())?,
                deleted: row.get(3).map_err(|error| error.to_string())?,
            });
        }
        Ok(checkpoints)
    }

    pub(crate) fn seal(
        data: &Path,
        origin: &Origin,
        operation: Uuid,
        revision: u64,
    ) -> Result<Pending, String> {
        let mut connection = Self::connect(data)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let mut pending = Self::entry(&transaction, origin, operation)?
            .ok_or("outbound operation disappeared")?;
        if pending.base_revision.is_none() {
            if Self::state(&transaction, origin, pending.profile_id)?
                .map_or(0, |state| state.remote_revision)
                != revision
            {
                return Err("Profile checkpoint changed before dispatch".into());
            }
            pending.base_revision = Some(revision);
            Self::replace(&transaction, &pending)?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(pending)
    }

    pub(crate) fn acknowledge(
        data: &Path,
        origin: &Origin,
        receipt: &Receipt,
    ) -> Result<Checkpoint, String> {
        let mut connection = Self::connect(data)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let pending = Self::entry(&transaction, origin, receipt.operation_id)?
            .ok_or("outbound operation disappeared")?;
        if pending.profile_id != receipt.entity_id
            || pending
                .base_revision
                .and_then(|revision| revision.checked_add(1))
                != Some(receipt.revision)
        {
            return Err("receipt does not match the sealed operation".into());
        }
        Self::checkpoint(
            &transaction,
            origin,
            pending.profile_id,
            receipt.revision,
            pending.local_revision,
            pending.name.is_none(),
        )?;
        Self::remove(&transaction, origin, pending.operation_id)?;
        let checkpoint = Self::state(&transaction, origin, pending.profile_id)?
            .ok_or("Profile checkpoint disappeared")?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(checkpoint)
    }

    pub(crate) fn conflicted(
        data: &Path,
        origin: &Origin,
        operation: Uuid,
        current: Option<Record>,
    ) -> Result<(), String> {
        let mut connection = Self::connect(data)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let mut pending = Self::entry(&transaction, origin, operation)?
            .ok_or("outbound operation disappeared")?;
        pending.conflict = current;
        pending.conflicted = true;
        Self::replace(&transaction, &pending)?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub(crate) fn replace_pending(
        data: &Path,
        pending: &Pending,
        scope: &ExpectedScope,
        conflict_id: Uuid,
    ) -> Result<(), String> {
        let mut connection = Self::connect(data)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let account = Self::record(&transaction, scope.account_id)?;
        let binding = account
            .components
            .iter()
            .find(|value| value.key == "account_binding" && value.version == 1)
            .and_then(|value| serde_json::from_value::<crate::Binding>(value.value.clone()).ok());
        let conflict = Self::entry(&transaction, &pending.origin, conflict_id)?
            .ok_or("Profile conflict changed")?;
        let profile = Self::record(&transaction, pending.profile_id)?;
        if !scope.matches(
            &vmux_ecs::Uuid(account.id.to_string()),
            binding.as_ref(),
            account.revision,
        ) || profile.revision != pending.local_revision
            || profile.scope_id != scope.account_id
            || !conflict.conflicted
            || conflict.profile_id != pending.profile_id
            || conflict
                .conflict
                .as_ref()
                .map_or(0, |record| record.revision)
                != pending.base_revision.unwrap_or(0)
        {
            return Err("Profile or Account changed before conflict resolution".into());
        }
        transaction
            .execute(
                "delete from profile_outbox where profile_id=?1 and server=?2 and account_id=?3 and json_extract(value,'$.local_revision') <= ?4",
                params![
                    pending.profile_id.to_string(),
                    pending.origin.server,
                    pending.origin.account.to_string(),
                    pending.local_revision
                ],
            )
            .map_err(|error| error.to_string())?;
        Self::enqueue(&transaction, pending)?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub(crate) fn apply_checkpoint(
        transaction: &Transaction<'_>,
        origin: &Origin,
        profile: Uuid,
        local: u64,
        change: &CheckpointChange,
    ) -> Result<(), String> {
        if let Some(operation) = change.resolves {
            let pending =
                Self::entry(transaction, origin, operation)?.ok_or("Profile conflict changed")?;
            if !pending.conflicted
                || pending.profile_id != profile
                || pending
                    .conflict
                    .as_ref()
                    .map_or(0, |record| record.revision)
                    != change.remote_revision
            {
                return Err("Profile conflict changed".into());
            }
        }
        transaction.execute(
            "delete from profile_outbox where profile_id=?1 and server=?2 and account_id=?3 and json_extract(value,'$.local_revision') <= ?4",
            params![profile.to_string(), origin.server, origin.account.to_string(), change.through_local_revision],
        ).map_err(|error| error.to_string())?;
        Self::checkpoint(
            transaction,
            origin,
            profile,
            change.remote_revision,
            local,
            change.deleted,
        )
    }

    fn record(transaction: &Transaction<'_>, id: Uuid) -> Result<Record, String> {
        let value: String = transaction
            .query_row(
                "select value from records where id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        serde_json::from_str(&value).map_err(|error| error.to_string())
    }

    fn checkpoint(
        transaction: &Transaction<'_>,
        origin: &Origin,
        profile: Uuid,
        remote: u64,
        local: u64,
        deleted: bool,
    ) -> Result<(), String> {
        transaction.execute(
            "insert into profile_checkpoint (profile_id,server,account_id,remote_revision,local_revision,deleted)
             values (?1,?2,?3,?4,?5,?6)
             on conflict(profile_id,server,account_id) do update set
             remote_revision=max(remote_revision,excluded.remote_revision),
             local_revision=max(local_revision,excluded.local_revision),
             deleted=case when excluded.remote_revision >= remote_revision then excluded.deleted else deleted end",
            params![profile.to_string(),origin.server,origin.account.to_string(),remote,local,deleted],
        ).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn state(
        transaction: &Transaction<'_>,
        origin: &Origin,
        profile: Uuid,
    ) -> Result<Option<Checkpoint>, String> {
        transaction.query_row(
            "select remote_revision,local_revision,deleted from profile_checkpoint where profile_id=?1 and server=?2 and account_id=?3",
            params![profile.to_string(), origin.server, origin.account.to_string()],
            |row| Ok(Checkpoint { profile_id: profile, remote_revision: row.get(0)?, local_revision: row.get(1)?, deleted: row.get(2)? }),
        ).optional().map_err(|error| error.to_string())
    }

    fn entry(
        transaction: &Transaction<'_>,
        origin: &Origin,
        operation: Uuid,
    ) -> Result<Option<Pending>, String> {
        let value: Option<String> = transaction
            .query_row(
                "select value from profile_outbox where id=?1 and server=?2 and account_id=?3",
                params![
                    operation.to_string(),
                    origin.server,
                    origin.account.to_string()
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        match value {
            Some(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|error| error.to_string()),
            None => Ok(None),
        }
    }

    fn replace(transaction: &Transaction<'_>, pending: &Pending) -> Result<(), String> {
        let value = serde_json::to_string(pending).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "update profile_outbox set value=?2 where id=?1",
                params![pending.operation_id.to_string(), value],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn remove(
        transaction: &Transaction<'_>,
        origin: &Origin,
        operation: Uuid,
    ) -> Result<(), String> {
        transaction
            .execute(
                "delete from profile_outbox where id=?1 and server=?2 and account_id=?3",
                params![
                    operation.to_string(),
                    origin.server,
                    origin.account.to_string()
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        directory: tempfile::TempDir,
        origin: Origin,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                directory: tempfile::tempdir().unwrap(),
                origin: Origin {
                    server: "localhost:18788".into(),
                    account: Uuid::new_v4(),
                },
            }
        }

        fn enqueue(&self, profile: Uuid, revision: u64, name: &str) -> Uuid {
            let operation = Uuid::new_v4();
            let mut database = Store::connect(self.directory.path()).unwrap();
            let transaction = database.transaction().unwrap();
            Store::enqueue(
                &transaction,
                &Pending {
                    operation_id: operation,
                    profile_id: profile,
                    origin: self.origin.clone(),
                    local_revision: revision,
                    name: Some(name.into()),
                    base_revision: None,
                    conflict: None,
                    conflicted: false,
                },
            )
            .unwrap();
            transaction.commit().unwrap();
            operation
        }
    }

    #[test]
    fn acknowledged_writes_advance_the_next_base_without_rewriting_a_lost_ack_replay() {
        let fixture = Fixture::new();
        let profile = Uuid::new_v4();
        let first = fixture.enqueue(profile, 3, "First");
        Store::seal(fixture.directory.path(), &fixture.origin, first, 0).unwrap();
        let later = fixture.enqueue(profile, 4, "Later edit during send");
        let replay = Store::seal(fixture.directory.path(), &fixture.origin, first, 99).unwrap();
        assert_eq!(replay.base_revision, Some(0));
        assert_eq!(replay.name.as_deref(), Some("First"));
        let receipt = Receipt {
            operation_id: first,
            entity_id: profile,
            revision: 1,
        };
        Store::acknowledge(fixture.directory.path(), &fixture.origin, &receipt).unwrap();
        assert!(Store::seal(fixture.directory.path(), &fixture.origin, later, 0).is_err());
        let sent = Store::seal(fixture.directory.path(), &fixture.origin, later, 1).unwrap();
        let loaded = Store::load(fixture.directory.path(), &fixture.origin, 0, None).unwrap();
        let pending = loaded.pending;
        let checkpoints = loaded.checkpoints;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1.operation_id, later);
        assert_eq!(sent.name.as_deref(), Some("Later edit during send"));
        assert_eq!(checkpoints[0].local_revision, 3);
    }

    #[test]
    fn old_acknowledgements_cannot_resurrect_a_newer_durable_remote_tombstone() {
        let fixture = Fixture::new();
        let profile = Uuid::new_v4();
        let operation = fixture.enqueue(profile, 3, "Before deletion");
        Store::seal(fixture.directory.path(), &fixture.origin, operation, 0).unwrap();
        let mut database = Store::connect(fixture.directory.path()).unwrap();
        let transaction = database.transaction().unwrap();
        Store::apply_checkpoint(
            &transaction,
            &fixture.origin,
            profile,
            4,
            &CheckpointChange {
                remote_revision: 2,
                through_local_revision: 0,
                deleted: true,
                resolves: None,
            },
        )
        .unwrap();
        transaction.commit().unwrap();
        let receipt = Receipt {
            operation_id: operation,
            entity_id: profile,
            revision: 1,
        };
        Store::acknowledge(fixture.directory.path(), &fixture.origin, &receipt).unwrap();
        let loaded = Store::load(fixture.directory.path(), &fixture.origin, 0, None).unwrap();
        let pending = loaded.pending;
        let checkpoints = loaded.checkpoints;
        assert!(pending.is_empty());
        assert_eq!(checkpoints.len(), 1);
        assert!(checkpoints[0].deleted);
        assert_eq!(checkpoints[0].remote_revision, 2);
        assert_eq!(checkpoints[0].local_revision, 4);
    }

    #[test]
    fn bounded_inventory_reaches_later_profiles_and_keeps_its_captured_boundary() {
        let fixture = Fixture::new();
        let blocked = Uuid::new_v4();
        let mut expected = Vec::new();
        for revision in 1..=20 {
            expected.push(fixture.enqueue(blocked, revision, "Blocked profile"));
        }
        Store::conflicted(fixture.directory.path(), &fixture.origin, expected[0], None).unwrap();
        for _ in 0..13 {
            expected.push(fixture.enqueue(Uuid::new_v4(), 1, "Unrelated profile"));
        }
        let first = Store::load(fixture.directory.path(), &fixture.origin, 0, None).unwrap();
        assert!(first.pending[0].1.conflicted);
        assert!(first.pending.len() <= 16);
        let appended = fixture.enqueue(Uuid::new_v4(), 1, "Appended during inventory");
        let through = first.through;
        let mut next = first.next;
        let mut received: Vec<_> = first
            .pending
            .iter()
            .map(|(_, entry)| entry.operation_id)
            .collect();
        while let Some(after) = next {
            let page = Store::load(
                fixture.directory.path(),
                &fixture.origin,
                after,
                Some(through),
            )
            .unwrap();
            assert!(page.pending.len() <= 16);
            received.extend(page.pending.iter().map(|(_, entry)| entry.operation_id));
            next = page.next;
        }
        assert_eq!(received, expected);
        assert!(!received.contains(&appended));
        let later = Store::load(fixture.directory.path(), &fixture.origin, through, None).unwrap();
        assert_eq!(later.pending.len(), 1);
        assert_eq!(later.pending[0].1.operation_id, appended);
    }
}
