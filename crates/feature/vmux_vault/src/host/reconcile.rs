use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{Task, futures_lite::future};
use vmux_ecs::{Operation, OperationRetained, OperationTarget};

use crate::storage::{
    EntryKind, FileAttributes, FileTimestamp, Hex, LocalEntry, LocalFingerprint, LocalPath, RonMap,
    SnapshotPath,
};

use super::key::KeyCanceled;
use super::vault_io::VaultWake;

pub(super) struct ReconcilePlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ReconcileSet;

impl Plugin for ReconcilePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (
                    cancel,
                    spawn_entries,
                    classify_entries,
                    start_ron_merge,
                    expand_ron_fields,
                    assemble_ron_fields,
                    finish_ron_merge,
                    start_toml_merge,
                    expand_toml_fields,
                    assemble_toml_fields,
                    finish_toml_merge,
                    start_json_merge,
                    expand_json_fields,
                    assemble_json_fields,
                    finish_json_merge,
                )
                    .chain(),
                (
                    submit_text_merge,
                    drain_text_merge,
                    request_conflict_label,
                    drain_conflict_label,
                    allocate_conflict_copies,
                    advance_to_validate,
                    validate_tree,
                    submit_verify_fingerprints,
                    drain_verify_fingerprints,
                    submit_apply_mutation,
                    drain_apply_mutation,
                    drain_canceled_session,
                    retire_completed,
                )
                    .chain(),
            )
                .chain()
                .in_set(ReconcileSet),
        );
    }
}

#[derive(Component)]
pub(super) struct ReconcileSession;

#[derive(Component)]
pub(super) struct ReconcileRoot(pub(super) PathBuf);

#[derive(Component)]
pub(super) struct ReconcileInputs {
    pub(super) baseline: BTreeMap<String, LocalEntry>,
    pub(super) local: BTreeMap<String, LocalEntry>,
    pub(super) remote: BTreeMap<String, LocalEntry>,
}

#[derive(Component, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct ReconcileCounts {
    pub(super) automatic_merges: usize,
    pub(super) conflict_copies: usize,
}

impl ReconcileCounts {
    pub(super) fn message(self) -> String {
        if self.conflict_copies > 0 {
            let noun = if self.conflict_copies == 1 {
                "copy"
            } else {
                "copies"
            };
            return format!(
                "Vault synced with {} conflicted {noun}",
                self.conflict_copies
            );
        }
        if self.automatic_merges > 0 {
            let noun = if self.automatic_merges == 1 {
                "merge"
            } else {
                "merges"
            };
            return format!(
                "Vault synced with {} automatic {noun}",
                self.automatic_merges
            );
        }
        "Vault synced".to_string()
    }
}

#[derive(Component)]
pub(super) struct ReconcileDone(pub(super) Result<ReconcileCounts, String>);

#[derive(Component)]
pub(super) struct ReconcileRetire;

#[derive(Component)]
pub(super) struct ReconcileSpawning;

#[derive(Component)]
struct ReconcileMerging;

#[derive(Component)]
struct ReconcileValidating;

#[derive(Component)]
struct ReconcileVerifying;

#[derive(Component)]
struct ReconcileApplying;

#[derive(Component)]
struct ReconcileCanceling;

#[derive(Component)]
struct ReconcileEntry;

#[derive(Component)]
struct RelativePath(String);

#[derive(Component)]
struct BaselineFile(Option<LocalEntry>);

#[derive(Component)]
struct CapturedLocal(Option<LocalEntry>);

#[derive(Component)]
struct RemoteFile(Option<LocalEntry>);

#[derive(Component)]
struct UnclassifiedEntry;

#[derive(Component)]
struct KeepLocalEntry;

#[derive(Component)]
struct ProposedMutation(Option<LocalEntry>);

#[derive(Component)]
struct AppliedMutation;

#[derive(Component)]
struct AutomaticMerge;

#[derive(Component)]
struct OpaqueConflict;

#[derive(Component)]
struct ConflictCopyEntry;

#[derive(Component)]
struct ConflictLabel(String);

#[derive(Component)]
struct HostnameTask(Task<String>);

#[derive(Component)]
struct UnionTextMergeRequest;

#[derive(Component)]
struct LocalTextMergeRequest;

#[derive(Component)]
struct RequireValidRon;

#[derive(Component)]
struct TextMergeTask(Task<Result<Vec<u8>, String>>);

#[derive(Component)]
struct RonMergeRequest;

#[derive(Component)]
struct TomlMergeRequest;

#[derive(Component)]
struct JsonMergeRequest;

#[derive(Component)]
struct FieldRoot;

#[derive(Component)]
struct ParentField(Entity);

#[derive(Component)]
struct AwaitingChildren;

#[derive(Component)]
struct RonField {
    baseline: Option<ron::Value>,
    local: Option<ron::Value>,
    remote: Option<ron::Value>,
}

impl RonField {
    fn options_equal(left: Option<&ron::Value>, right: Option<&ron::Value>) -> bool {
        let (Some(left), Some(right)) = (left, right) else {
            return left.is_none() && right.is_none();
        };
        let mut stack = vec![(left, right)];
        while let Some((left, right)) = stack.pop() {
            match (left, right) {
                (ron::Value::Map(left), ron::Value::Map(right)) => {
                    if left.len() != right.len() {
                        return false;
                    }
                    for (key, value) in left.iter() {
                        let Some(other) = RonMap(right).get(key) else {
                            return false;
                        };
                        stack.push((value, other));
                    }
                }
                (ron::Value::Seq(left), ron::Value::Seq(right)) => {
                    if left.len() != right.len() {
                        return false;
                    }
                    for (left, right) in left.iter().zip(right) {
                        stack.push((left, right));
                    }
                }
                (ron::Value::Option(left), ron::Value::Option(right)) => match (left, right) {
                    (Some(left), Some(right)) => stack.push((left, right)),
                    (None, None) => {}
                    _ => return false,
                },
                _ => {
                    if left != right {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[derive(Component)]
struct RonFieldKey(ron::Value);

#[derive(Component)]
struct ResolvedRon(Option<ron::Value>);

#[derive(Component)]
struct TomlField {
    baseline: Option<toml::Value>,
    local: Option<toml::Value>,
    remote: Option<toml::Value>,
}

#[derive(Component)]
struct TomlFieldKey(String);

#[derive(Component)]
struct ResolvedToml(Option<toml::Value>);

#[derive(Component)]
struct JsonField {
    baseline: Option<serde_json::Value>,
    local: Option<serde_json::Value>,
    remote: Option<serde_json::Value>,
}

#[derive(Component)]
struct JsonFieldKey(String);

#[derive(Component)]
struct ResolvedJson(Option<serde_json::Value>);

#[derive(Component)]
struct VerifyFingerprintsTask(Task<Result<(), String>>);

#[derive(Component)]
struct ApplyMutationTask(Task<Result<(), String>>);

#[derive(SystemParam)]
pub(super) struct ReconcileLauncher<'w, 's> {
    commands: Commands<'w, 's>,
}

impl ReconcileLauncher<'_, '_> {
    pub(super) fn spawn(
        &mut self,
        owner: Entity,
        root: PathBuf,
        wake: VaultWake,
        inputs: ReconcileInputs,
    ) -> Entity {
        wake.notify();
        self.commands
            .spawn((
                Operation::bundle("vault.reconcile").unwrap().running(),
                OperationRetained,
                OperationTarget::new(owner),
                ReconcileSession,
                ReconcileRoot(root),
                wake,
                inputs,
                ReconcileCounts::default(),
                ReconcileSpawning,
            ))
            .id()
    }
}

#[derive(SystemParam)]
struct MergeProgress<'w, 's> {
    pending_entries: Query<
        'w,
        's,
        &'static OperationTarget,
        (
            With<ReconcileEntry>,
            Or<(
                With<UnclassifiedEntry>,
                With<RonMergeRequest>,
                With<TomlMergeRequest>,
                With<JsonMergeRequest>,
                With<UnionTextMergeRequest>,
                With<LocalTextMergeRequest>,
                With<TextMergeTask>,
            )>,
        ),
    >,
    entries: Query<'w, 's, (Entity, &'static OperationTarget), With<ReconcileEntry>>,
    ron_fields: Query<'w, 's, &'static OperationTarget, With<RonField>>,
    toml_fields: Query<'w, 's, &'static OperationTarget, With<TomlField>>,
    json_fields: Query<'w, 's, &'static OperationTarget, With<JsonField>>,
}

impl MergeProgress<'_, '_> {
    fn has_pending(&self, session: Entity) -> bool {
        if self
            .pending_entries
            .iter()
            .any(|target| target.entity() == session)
        {
            return true;
        }
        let entries: BTreeSet<Entity> = self
            .entries
            .iter()
            .filter(|(_, target)| target.entity() == session)
            .map(|(entity, _)| entity)
            .collect();
        self.ron_fields
            .iter()
            .any(|target| entries.contains(&target.entity()))
            || self
                .toml_fields
                .iter()
                .any(|target| entries.contains(&target.entity()))
            || self
                .json_fields
                .iter()
                .any(|target| entries.contains(&target.entity()))
    }
}

fn cancel(
    sessions: Query<
        (Entity, &OperationTarget, &VaultWake),
        (
            With<ReconcileSession>,
            Without<ReconcileDone>,
            Without<ReconcileCanceling>,
        ),
    >,
    owners: Query<Option<&KeyCanceled>>,
    mut commands: Commands,
) {
    for (session, target, wake) in &sessions {
        let canceled = match owners.get(target.entity()) {
            Ok(Some(flag)) => flag.0.load(Ordering::Acquire),
            Ok(None) => false,
            Err(_) => true,
        };
        if canceled {
            commands
                .entity(session)
                .remove::<(
                    ReconcileSpawning,
                    ReconcileMerging,
                    ReconcileValidating,
                    ReconcileVerifying,
                    ReconcileApplying,
                )>()
                .insert(ReconcileCanceling);
            wake.notify();
        }
    }
}

fn spawn_entries(
    mut sessions: Query<
        (Entity, &VaultWake, &mut ReconcileInputs),
        (
            With<ReconcileSession>,
            With<ReconcileSpawning>,
            Without<ReconcileCanceling>,
        ),
    >,
    mut commands: Commands,
) {
    for (session, wake, mut inputs) in &mut sessions {
        let paths: BTreeSet<String> = inputs
            .baseline
            .keys()
            .chain(inputs.local.keys())
            .chain(inputs.remote.keys())
            .cloned()
            .collect();
        for path in paths {
            let baseline = inputs.baseline.remove(&path);
            let local = inputs.local.remove(&path);
            let remote = inputs.remote.remove(&path);
            commands.spawn((
                ReconcileEntry,
                OperationTarget::new(session),
                RelativePath(path),
                BaselineFile(baseline),
                CapturedLocal(local),
                RemoteFile(remote),
                UnclassifiedEntry,
            ));
        }
        wake.notify();
        commands
            .entity(session)
            .remove::<(ReconcileInputs, ReconcileSpawning)>()
            .insert(ReconcileMerging);
    }
}

fn classify_entries(
    entries: Query<
        (
            Entity,
            &OperationTarget,
            &RelativePath,
            &BaselineFile,
            &CapturedLocal,
            &RemoteFile,
        ),
        (With<ReconcileEntry>, With<UnclassifiedEntry>),
    >,
    sessions: Query<&VaultWake, (With<ReconcileSession>, With<ReconcileMerging>)>,
    mut commands: Commands,
) {
    for (entity, target, path, baseline, local, remote) in &entries {
        let Ok(wake) = sessions.get(target.entity()) else {
            continue;
        };
        commands.entity(entity).remove::<UnclassifiedEntry>();
        let local_changed = !LocalEntry::same(local.0.as_ref(), baseline.0.as_ref());
        let remote_changed = !LocalEntry::same(remote.0.as_ref(), baseline.0.as_ref());
        if local_changed && remote_changed && !LocalEntry::same(local.0.as_ref(), remote.0.as_ref())
        {
            match (local.0.as_ref(), remote.0.as_ref()) {
                (Some(local_entry), Some(remote_entry))
                    if local_entry.kind == EntryKind::File
                        && remote_entry.kind == EntryKind::File =>
                {
                    let baseline_bytes = baseline
                        .0
                        .as_ref()
                        .filter(|entry| entry.kind == EntryKind::File)
                        .map(|entry| entry.data.as_slice())
                        .unwrap_or_default();
                    let utf8 = std::str::from_utf8(baseline_bytes).is_ok()
                        && std::str::from_utf8(&local_entry.data).is_ok()
                        && std::str::from_utf8(&remote_entry.data).is_ok();
                    let extension = Path::new(&path.0)
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    match extension.as_str() {
                        "md" | "markdown" if utf8 => {
                            commands.entity(entity).insert(UnionTextMergeRequest);
                        }
                        "ron" if utf8 => {
                            commands.entity(entity).insert(RonMergeRequest);
                        }
                        "toml" => {
                            commands.entity(entity).insert(TomlMergeRequest);
                        }
                        "json" => {
                            commands.entity(entity).insert(JsonMergeRequest);
                        }
                        _ => {
                            commands.entity(entity).insert(OpaqueConflict);
                        }
                    }
                }
                _ => {
                    commands.entity(entity).insert(OpaqueConflict);
                }
            }
        } else if remote_changed && !local_changed {
            commands
                .entity(entity)
                .insert(ProposedMutation(remote.0.clone()));
        } else {
            commands.entity(entity).insert(KeepLocalEntry);
        }
        wake.notify();
    }
}

fn start_ron_merge(
    entries: Query<
        (
            Entity,
            &OperationTarget,
            &BaselineFile,
            &CapturedLocal,
            &RemoteFile,
        ),
        (With<ReconcileEntry>, With<RonMergeRequest>),
    >,
    sessions: Query<&VaultWake, (With<ReconcileSession>, With<ReconcileMerging>)>,
    mut commands: Commands,
) {
    for (entry, target, baseline, local, remote) in &entries {
        let Ok(wake) = sessions.get(target.entity()) else {
            continue;
        };
        commands.entity(entry).remove::<RonMergeRequest>();
        let (Some(local_entry), Some(remote_entry)) = (&local.0, &remote.0) else {
            commands.entity(entry).insert(OpaqueConflict);
            wake.notify();
            continue;
        };
        let baseline_bytes = match &baseline.0 {
            Some(entry) if entry.kind == EntryKind::File => entry.data.as_slice(),
            Some(_) => b"".as_slice(),
            None => b"{}".as_slice(),
        };
        let parsed = (
            std::str::from_utf8(baseline_bytes)
                .ok()
                .and_then(|s| ron::from_str::<ron::Value>(s).ok()),
            std::str::from_utf8(&local_entry.data)
                .ok()
                .and_then(|s| ron::from_str::<ron::Value>(s).ok()),
            std::str::from_utf8(&remote_entry.data)
                .ok()
                .and_then(|s| ron::from_str::<ron::Value>(s).ok()),
        );
        match parsed {
            (Some(baseline), Some(local), Some(remote)) => {
                commands.spawn((
                    OperationTarget::new(entry),
                    FieldRoot,
                    RonField {
                        baseline: Some(baseline),
                        local: Some(local),
                        remote: Some(remote),
                    },
                ));
            }
            _ => {
                commands
                    .entity(entry)
                    .insert((LocalTextMergeRequest, RequireValidRon));
            }
        }
        wake.notify();
    }
}

fn expand_ron_fields(
    fields: Query<
        (Entity, &OperationTarget, &RonField),
        (Without<ResolvedRon>, Without<AwaitingChildren>),
    >,
    entries: Query<&OperationTarget, With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (entity, owner_entry, field) in &fields {
        let resolved = if RonField::options_equal(field.local.as_ref(), field.remote.as_ref()) {
            Some(field.local.clone())
        } else if RonField::options_equal(field.local.as_ref(), field.baseline.as_ref()) {
            Some(field.remote.clone())
        } else if RonField::options_equal(field.remote.as_ref(), field.baseline.as_ref()) {
            Some(field.local.clone())
        } else {
            None
        };
        if let Some(value) = resolved {
            commands.entity(entity).insert(ResolvedRon(value));
        } else if let (
            Some(ron::Value::Map(baseline)),
            Some(ron::Value::Map(local)),
            Some(ron::Value::Map(remote)),
        ) = (&field.baseline, &field.local, &field.remote)
        {
            let keys: BTreeSet<ron::Value> = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect();
            if keys.is_empty() {
                commands
                    .entity(entity)
                    .insert(ResolvedRon(Some(ron::Value::Map(ron::Map::new()))));
            } else {
                for key in keys {
                    commands.spawn((
                        OperationTarget::new(owner_entry.entity()),
                        ParentField(entity),
                        RonFieldKey(key.clone()),
                        RonField {
                            baseline: RonMap(baseline).get(&key).cloned(),
                            local: RonMap(local).get(&key).cloned(),
                            remote: RonMap(remote).get(&key).cloned(),
                        },
                    ));
                }
                commands.entity(entity).insert(AwaitingChildren);
            }
        } else {
            commands
                .entity(entity)
                .insert(ResolvedRon(field.local.clone()));
        }
        if let Ok(session_target) = entries.get(owner_entry.entity())
            && let Ok(wake) = sessions.get(session_target.entity())
        {
            wake.notify();
        }
    }
}

fn assemble_ron_fields(
    parents: Query<(Entity, &OperationTarget), (With<RonField>, With<AwaitingChildren>)>,
    children: Query<(Entity, &ParentField, &RonFieldKey, Option<&ResolvedRon>), With<RonField>>,
    entries: Query<&OperationTarget, With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (parent, owner_entry) in &parents {
        let mut ready = true;
        let mut map = ron::Map::new();
        let mut child_entities = Vec::new();
        for (child, link, key, resolved) in &children {
            if link.0 != parent {
                continue;
            }
            let Some(resolved) = resolved else {
                ready = false;
                break;
            };
            child_entities.push(child);
            if let Some(value) = &resolved.0 {
                map.insert(key.0.clone(), value.clone());
            }
        }
        if !ready {
            continue;
        }
        for child in child_entities {
            commands.entity(child).despawn();
        }
        commands
            .entity(parent)
            .remove::<AwaitingChildren>()
            .insert(ResolvedRon(Some(ron::Value::Map(map))));
        if let Ok(session_target) = entries.get(owner_entry.entity())
            && let Ok(wake) = sessions.get(session_target.entity())
        {
            wake.notify();
        }
    }
}

fn finish_ron_merge(
    roots: Query<(Entity, &OperationTarget, &ResolvedRon), (With<RonField>, With<FieldRoot>)>,
    entries: Query<(&OperationTarget, &CapturedLocal), With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (root, target, resolved) in &roots {
        commands.entity(root).despawn();
        let Ok((session_target, local)) = entries.get(target.entity()) else {
            continue;
        };
        let Some(local_entry) = &local.0 else {
            commands.entity(target.entity()).insert(OpaqueConflict);
            continue;
        };
        let serialized = resolved.0.as_ref().and_then(|value| {
            let mut bytes = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::new())
                .ok()?
                .into_bytes();
            bytes.push(b'\n');
            let source = std::str::from_utf8(&bytes).ok()?;
            ron::from_str::<serde::de::IgnoredAny>(source).ok()?;
            Some(bytes)
        });
        match serialized {
            Some(bytes) => {
                commands.entity(target.entity()).insert((
                    ProposedMutation(Some(local_entry.with_data(bytes))),
                    AutomaticMerge,
                ));
            }
            None => {
                commands
                    .entity(target.entity())
                    .insert((LocalTextMergeRequest, RequireValidRon));
            }
        }
        if let Ok(wake) = sessions.get(session_target.entity()) {
            wake.notify();
        }
    }
}

fn start_toml_merge(
    entries: Query<
        (
            Entity,
            &OperationTarget,
            &BaselineFile,
            &CapturedLocal,
            &RemoteFile,
        ),
        (With<ReconcileEntry>, With<TomlMergeRequest>),
    >,
    sessions: Query<&VaultWake, (With<ReconcileSession>, With<ReconcileMerging>)>,
    mut commands: Commands,
) {
    for (entry, target, baseline, local, remote) in &entries {
        let Ok(wake) = sessions.get(target.entity()) else {
            continue;
        };
        commands.entity(entry).remove::<TomlMergeRequest>();
        let (Some(local_entry), Some(remote_entry)) = (&local.0, &remote.0) else {
            commands.entity(entry).insert(OpaqueConflict);
            wake.notify();
            continue;
        };
        let baseline_bytes = baseline
            .0
            .as_ref()
            .filter(|entry| entry.kind == EntryKind::File)
            .map(|entry| entry.data.as_slice())
            .unwrap_or_default();
        let parsed = (
            std::str::from_utf8(baseline_bytes)
                .ok()
                .and_then(|s| toml::from_str::<toml::Value>(s).ok()),
            std::str::from_utf8(&local_entry.data)
                .ok()
                .and_then(|s| toml::from_str::<toml::Value>(s).ok()),
            std::str::from_utf8(&remote_entry.data)
                .ok()
                .and_then(|s| toml::from_str::<toml::Value>(s).ok()),
        );
        match parsed {
            (Some(baseline), Some(local), Some(remote)) => {
                commands.spawn((
                    OperationTarget::new(entry),
                    FieldRoot,
                    TomlField {
                        baseline: Some(baseline),
                        local: Some(local),
                        remote: Some(remote),
                    },
                ));
            }
            _ => {
                commands.entity(entry).insert(OpaqueConflict);
            }
        }
        wake.notify();
    }
}

fn expand_toml_fields(
    fields: Query<
        (Entity, &OperationTarget, &TomlField),
        (Without<ResolvedToml>, Without<AwaitingChildren>),
    >,
    entries: Query<&OperationTarget, With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (entity, owner_entry, field) in &fields {
        let resolved = if field.local == field.remote {
            Some(field.local.clone())
        } else if field.local == field.baseline {
            Some(field.remote.clone())
        } else if field.remote == field.baseline {
            Some(field.local.clone())
        } else {
            None
        };
        if let Some(value) = resolved {
            commands.entity(entity).insert(ResolvedToml(value));
        } else if let (
            Some(toml::Value::Table(baseline)),
            Some(toml::Value::Table(local)),
            Some(toml::Value::Table(remote)),
        ) = (&field.baseline, &field.local, &field.remote)
        {
            let keys: BTreeSet<String> = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect();
            if keys.is_empty() {
                commands
                    .entity(entity)
                    .insert(ResolvedToml(Some(
                        toml::Value::Table(toml::map::Map::new()),
                    )));
            } else {
                for key in keys {
                    commands.spawn((
                        OperationTarget::new(owner_entry.entity()),
                        ParentField(entity),
                        TomlFieldKey(key.clone()),
                        TomlField {
                            baseline: baseline.get(&key).cloned(),
                            local: local.get(&key).cloned(),
                            remote: remote.get(&key).cloned(),
                        },
                    ));
                }
                commands.entity(entity).insert(AwaitingChildren);
            }
        } else {
            commands
                .entity(entity)
                .insert(ResolvedToml(field.local.clone()));
        }
        if let Ok(session_target) = entries.get(owner_entry.entity())
            && let Ok(wake) = sessions.get(session_target.entity())
        {
            wake.notify();
        }
    }
}

fn assemble_toml_fields(
    parents: Query<(Entity, &OperationTarget), (With<TomlField>, With<AwaitingChildren>)>,
    children: Query<(Entity, &ParentField, &TomlFieldKey, Option<&ResolvedToml>), With<TomlField>>,
    entries: Query<&OperationTarget, With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (parent, owner_entry) in &parents {
        let mut ready = true;
        let mut table = toml::map::Map::new();
        let mut child_entities = Vec::new();
        for (child, link, key, resolved) in &children {
            if link.0 != parent {
                continue;
            }
            let Some(resolved) = resolved else {
                ready = false;
                break;
            };
            child_entities.push(child);
            if let Some(value) = &resolved.0 {
                table.insert(key.0.clone(), value.clone());
            }
        }
        if !ready {
            continue;
        }
        for child in child_entities {
            commands.entity(child).despawn();
        }
        commands
            .entity(parent)
            .remove::<AwaitingChildren>()
            .insert(ResolvedToml(Some(toml::Value::Table(table))));
        if let Ok(session_target) = entries.get(owner_entry.entity())
            && let Ok(wake) = sessions.get(session_target.entity())
        {
            wake.notify();
        }
    }
}

fn finish_toml_merge(
    roots: Query<(Entity, &OperationTarget, &ResolvedToml), (With<TomlField>, With<FieldRoot>)>,
    entries: Query<(&OperationTarget, &CapturedLocal), With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (root, target, resolved) in &roots {
        commands.entity(root).despawn();
        let Ok((session_target, local)) = entries.get(target.entity()) else {
            continue;
        };
        let Some(local_entry) = &local.0 else {
            commands.entity(target.entity()).insert(OpaqueConflict);
            continue;
        };
        let serialized = resolved
            .0
            .as_ref()
            .and_then(|value| toml::to_string_pretty(value).ok().map(String::into_bytes));
        match serialized {
            Some(bytes) => {
                commands.entity(target.entity()).insert((
                    ProposedMutation(Some(local_entry.with_data(bytes))),
                    AutomaticMerge,
                ));
            }
            None => {
                commands.entity(target.entity()).insert(OpaqueConflict);
            }
        }
        if let Ok(wake) = sessions.get(session_target.entity()) {
            wake.notify();
        }
    }
}

fn start_json_merge(
    entries: Query<
        (
            Entity,
            &OperationTarget,
            &BaselineFile,
            &CapturedLocal,
            &RemoteFile,
        ),
        (With<ReconcileEntry>, With<JsonMergeRequest>),
    >,
    sessions: Query<&VaultWake, (With<ReconcileSession>, With<ReconcileMerging>)>,
    mut commands: Commands,
) {
    for (entry, target, baseline, local, remote) in &entries {
        let Ok(wake) = sessions.get(target.entity()) else {
            continue;
        };
        commands.entity(entry).remove::<JsonMergeRequest>();
        let (Some(local_entry), Some(remote_entry)) = (&local.0, &remote.0) else {
            commands.entity(entry).insert(OpaqueConflict);
            wake.notify();
            continue;
        };
        let baseline_bytes = match &baseline.0 {
            Some(entry) if entry.kind == EntryKind::File => entry.data.as_slice(),
            Some(_) => b"".as_slice(),
            None => b"{}".as_slice(),
        };
        let parsed = (
            serde_json::from_slice::<serde_json::Value>(baseline_bytes).ok(),
            serde_json::from_slice::<serde_json::Value>(&local_entry.data).ok(),
            serde_json::from_slice::<serde_json::Value>(&remote_entry.data).ok(),
        );
        match parsed {
            (Some(baseline), Some(local), Some(remote)) => {
                commands.spawn((
                    OperationTarget::new(entry),
                    FieldRoot,
                    JsonField {
                        baseline: Some(baseline),
                        local: Some(local),
                        remote: Some(remote),
                    },
                ));
            }
            _ => {
                commands.entity(entry).insert(OpaqueConflict);
            }
        }
        wake.notify();
    }
}

fn expand_json_fields(
    fields: Query<
        (Entity, &OperationTarget, &JsonField),
        (Without<ResolvedJson>, Without<AwaitingChildren>),
    >,
    entries: Query<&OperationTarget, With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (entity, owner_entry, field) in &fields {
        let resolved = if field.local == field.remote {
            Some(field.local.clone())
        } else if field.local == field.baseline {
            Some(field.remote.clone())
        } else if field.remote == field.baseline {
            Some(field.local.clone())
        } else {
            None
        };
        if let Some(value) = resolved {
            commands.entity(entity).insert(ResolvedJson(value));
        } else if let (
            Some(serde_json::Value::Object(baseline)),
            Some(serde_json::Value::Object(local)),
            Some(serde_json::Value::Object(remote)),
        ) = (&field.baseline, &field.local, &field.remote)
        {
            let keys: BTreeSet<String> = baseline
                .keys()
                .chain(local.keys())
                .chain(remote.keys())
                .cloned()
                .collect();
            if keys.is_empty() {
                commands
                    .entity(entity)
                    .insert(ResolvedJson(Some(serde_json::Value::Object(
                        serde_json::Map::new(),
                    ))));
            } else {
                for key in keys {
                    commands.spawn((
                        OperationTarget::new(owner_entry.entity()),
                        ParentField(entity),
                        JsonFieldKey(key.clone()),
                        JsonField {
                            baseline: baseline.get(&key).cloned(),
                            local: local.get(&key).cloned(),
                            remote: remote.get(&key).cloned(),
                        },
                    ));
                }
                commands.entity(entity).insert(AwaitingChildren);
            }
        } else {
            commands
                .entity(entity)
                .insert(ResolvedJson(field.local.clone()));
        }
        if let Ok(session_target) = entries.get(owner_entry.entity())
            && let Ok(wake) = sessions.get(session_target.entity())
        {
            wake.notify();
        }
    }
}

fn assemble_json_fields(
    parents: Query<(Entity, &OperationTarget), (With<JsonField>, With<AwaitingChildren>)>,
    children: Query<(Entity, &ParentField, &JsonFieldKey, Option<&ResolvedJson>), With<JsonField>>,
    entries: Query<&OperationTarget, With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (parent, owner_entry) in &parents {
        let mut ready = true;
        let mut object = serde_json::Map::new();
        let mut child_entities = Vec::new();
        for (child, link, key, resolved) in &children {
            if link.0 != parent {
                continue;
            }
            let Some(resolved) = resolved else {
                ready = false;
                break;
            };
            child_entities.push(child);
            if let Some(value) = &resolved.0 {
                object.insert(key.0.clone(), value.clone());
            }
        }
        if !ready {
            continue;
        }
        for child in child_entities {
            commands.entity(child).despawn();
        }
        commands
            .entity(parent)
            .remove::<AwaitingChildren>()
            .insert(ResolvedJson(Some(serde_json::Value::Object(object))));
        if let Ok(session_target) = entries.get(owner_entry.entity())
            && let Ok(wake) = sessions.get(session_target.entity())
        {
            wake.notify();
        }
    }
}

fn finish_json_merge(
    roots: Query<(Entity, &OperationTarget, &ResolvedJson), (With<JsonField>, With<FieldRoot>)>,
    entries: Query<(&OperationTarget, &CapturedLocal), With<ReconcileEntry>>,
    sessions: Query<&VaultWake, With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (root, target, resolved) in &roots {
        commands.entity(root).despawn();
        let Ok((session_target, local)) = entries.get(target.entity()) else {
            continue;
        };
        let Some(local_entry) = &local.0 else {
            commands.entity(target.entity()).insert(OpaqueConflict);
            continue;
        };
        let serialized = resolved.0.as_ref().and_then(|value| {
            let mut bytes = serde_json::to_vec_pretty(value).ok()?;
            bytes.push(b'\n');
            Some(bytes)
        });
        match serialized {
            Some(bytes) => {
                commands.entity(target.entity()).insert((
                    ProposedMutation(Some(local_entry.with_data(bytes))),
                    AutomaticMerge,
                ));
            }
            None => {
                commands.entity(target.entity()).insert(OpaqueConflict);
            }
        }
        if let Ok(wake) = sessions.get(session_target.entity()) {
            wake.notify();
        }
    }
}

fn submit_text_merge(
    entries: Query<
        (
            Entity,
            &OperationTarget,
            &BaselineFile,
            &CapturedLocal,
            &RemoteFile,
            Has<UnionTextMergeRequest>,
        ),
        (
            With<ReconcileEntry>,
            Or<(With<UnionTextMergeRequest>, With<LocalTextMergeRequest>)>,
        ),
    >,
    sessions: Query<&VaultWake, (With<ReconcileSession>, With<ReconcileMerging>)>,
    mut commands: Commands,
) {
    for (entity, target, baseline, local, remote, union_strategy) in &entries {
        let Ok(wake) = sessions.get(target.entity()) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(UnionTextMergeRequest, LocalTextMergeRequest)>();
        let (Some(local_entry), Some(remote_entry)) = (&local.0, &remote.0) else {
            commands.entity(entity).insert(OpaqueConflict);
            continue;
        };
        let baseline_bytes = baseline
            .0
            .as_ref()
            .filter(|entry| entry.kind == EntryKind::File)
            .map(|entry| entry.data.clone())
            .unwrap_or_default();
        let local_bytes = local_entry.data.clone();
        let remote_bytes = remote_entry.data.clone();
        let flag = if union_strategy { "--union" } else { "--ours" };
        let task = wake
            .spawn(move || TextMergeTask::run(&baseline_bytes, &local_bytes, &remote_bytes, flag));
        commands.entity(entity).insert(TextMergeTask(task));
    }
}

impl TextMergeTask {
    fn run(
        baseline: &[u8],
        local: &[u8],
        remote: &[u8],
        strategy_flag: &str,
    ) -> Result<Vec<u8>, String> {
        if std::str::from_utf8(baseline).is_err()
            || std::str::from_utf8(local).is_err()
            || std::str::from_utf8(remote).is_err()
        {
            return Err("Vault text merge requires UTF-8 files".to_string());
        }
        let directory = std::env::temp_dir().join(format!("vmux-vault-merge-{}", Hex::random(8)?));
        std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let baseline_path = directory.join("baseline");
        let local_path = directory.join("local");
        let remote_path = directory.join("remote");
        let result = (|| {
            std::fs::write(&baseline_path, baseline).map_err(|error| error.to_string())?;
            std::fs::write(&local_path, local).map_err(|error| error.to_string())?;
            std::fs::write(&remote_path, remote).map_err(|error| error.to_string())?;
            let output = Command::new("git")
                .arg("merge-file")
                .arg(strategy_flag)
                .arg("--stdout")
                .arg(&local_path)
                .arg(&baseline_path)
                .arg(&remote_path)
                .output()
                .map_err(|error| format!("failed to merge Vault text: {error}"))?;
            if output.status.code().is_some_and(|code| code <= 127) {
                Ok(output.stdout)
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        })();
        let _ = std::fs::remove_dir_all(directory);
        result
    }
}

fn drain_text_merge(
    mut tasks: Query<(
        Entity,
        &OperationTarget,
        &CapturedLocal,
        Has<RequireValidRon>,
        &mut TextMergeTask,
    )>,
    sessions: Query<(&VaultWake, Has<ReconcileCanceling>), With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (entity, target, local, require_ron, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(entity)
            .remove::<(TextMergeTask, RequireValidRon)>();
        let Ok((wake, canceling)) = sessions.get(target.entity()) else {
            continue;
        };
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(bytes) => {
                let valid = !require_ron
                    || std::str::from_utf8(&bytes)
                        .ok()
                        .is_some_and(|s| ron::from_str::<serde::de::IgnoredAny>(s).is_ok());
                if valid && let Some(local_entry) = &local.0 {
                    commands.entity(entity).insert((
                        ProposedMutation(Some(local_entry.with_data(bytes))),
                        AutomaticMerge,
                    ));
                } else {
                    commands.entity(entity).insert(OpaqueConflict);
                }
            }
            Err(error) => {
                commands
                    .entity(target.entity())
                    .remove::<ReconcileMerging>()
                    .insert(ReconcileDone(Err(error)));
            }
        }
        wake.notify();
    }
}

fn request_conflict_label(
    sessions: Query<
        (Entity, &VaultWake),
        (
            With<ReconcileSession>,
            With<ReconcileMerging>,
            Without<ConflictLabel>,
            Without<HostnameTask>,
        ),
    >,
    conflicts: Query<&OperationTarget, (With<ReconcileEntry>, With<OpaqueConflict>)>,
    mut commands: Commands,
) {
    for (session, wake) in &sessions {
        let has_conflict = conflicts.iter().any(|target| target.entity() == session);
        if !has_conflict {
            continue;
        }
        let task = wake.spawn(HostnameTask::observe);
        commands.entity(session).insert(HostnameTask(task));
    }
}

impl HostnameTask {
    fn observe() -> String {
        let device = Command::new("/bin/hostname")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|device| !device.is_empty())
            .unwrap_or_else(|| "device".to_string());
        let cleaned = device
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let trimmed = cleaned.trim_matches('-');
        let label_host = if trimmed.is_empty() {
            "device"
        } else {
            trimmed
        };
        format!(
            "{} {}",
            label_host,
            chrono::Local::now().format("%Y-%m-%d %H-%M-%S")
        )
    }
}

fn drain_conflict_label(
    mut sessions: Query<(Entity, &VaultWake, &mut HostnameTask), With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (session, wake, mut task) in &mut sessions {
        let Some(label) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(session)
            .remove::<HostnameTask>()
            .insert(ConflictLabel(label));
        wake.notify();
    }
}

fn allocate_conflict_copies(
    progress: MergeProgress,
    sessions: Query<
        (Entity, &ConflictLabel, &VaultWake),
        (
            With<ReconcileSession>,
            With<ReconcileMerging>,
            Without<ReconcileCanceling>,
        ),
    >,
    all_paths: Query<(&OperationTarget, &RelativePath), With<ReconcileEntry>>,
    conflict_entries: Query<
        (
            Entity,
            &OperationTarget,
            &RelativePath,
            &CapturedLocal,
            &RemoteFile,
        ),
        (With<ReconcileEntry>, With<OpaqueConflict>),
    >,
    mut commands: Commands,
) {
    for (session, label, wake) in &sessions {
        if progress.has_pending(session) {
            continue;
        }
        let mut occupied: BTreeSet<String> = all_paths
            .iter()
            .filter(|(target, _)| target.entity() == session)
            .map(|(_, path)| path.0.clone())
            .collect();
        let mut conflicts: Vec<(Entity, String, Option<LocalEntry>, Option<LocalEntry>)> =
            conflict_entries
                .iter()
                .filter(|(_, target, _, _, _)| target.entity() == session)
                .map(|(entity, _, path, local, remote)| {
                    (entity, path.0.clone(), local.0.clone(), remote.0.clone())
                })
                .collect();
        if conflicts.is_empty() {
            continue;
        }
        conflicts.sort_by(|left, right| left.1.cmp(&right.1));
        for (entry, path, local, remote) in conflicts {
            commands
                .entity(entry)
                .remove::<OpaqueConflict>()
                .insert(ProposedMutation(remote));
            let Some(local_entry) = local else {
                continue;
            };
            match ConflictLabel::allocate_path(&path, &label.0, &mut occupied) {
                Ok(copy_path) => {
                    commands.spawn((
                        ReconcileEntry,
                        OperationTarget::new(session),
                        RelativePath(copy_path),
                        BaselineFile(None),
                        CapturedLocal(None),
                        RemoteFile(None),
                        ProposedMutation(Some(local_entry)),
                        ConflictCopyEntry,
                    ));
                }
                Err(error) => {
                    commands
                        .entity(session)
                        .remove::<ReconcileMerging>()
                        .insert(ReconcileDone(Err(error)));
                    break;
                }
            }
        }
        wake.notify();
    }
}

impl ConflictLabel {
    fn allocate_path(
        path: &str,
        label: &str,
        occupied: &mut BTreeSet<String>,
    ) -> Result<String, String> {
        let path = Path::new(path);
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("file");
        let extension = path.extension().and_then(|extension| extension.to_str());
        for index in 1..=u16::MAX {
            let suffix = if index == 1 {
                String::new()
            } else {
                format!(" {index}")
            };
            let file_name = match extension {
                Some(extension) => format!("{stem} (Conflicted copy {label}){suffix}.{extension}"),
                None => format!("{stem} (Conflicted copy {label}){suffix}"),
            };
            let candidate = parent.join(file_name).to_string_lossy().replace('\\', "/");
            SnapshotPath(&candidate).validate()?;
            if occupied.insert(candidate.clone()) {
                return Ok(candidate);
            }
        }
        Err("failed to allocate Vault conflict copy".to_string())
    }
}

fn advance_to_validate(
    progress: MergeProgress,
    mut sessions: Query<
        (Entity, &mut ReconcileCounts, &VaultWake),
        (
            With<ReconcileSession>,
            With<ReconcileMerging>,
            Without<HostnameTask>,
            Without<ReconcileCanceling>,
            Without<ReconcileDone>,
        ),
    >,
    conflicts: Query<&OperationTarget, (With<ReconcileEntry>, With<OpaqueConflict>)>,
    merges: Query<&OperationTarget, (With<ReconcileEntry>, With<AutomaticMerge>)>,
    copies: Query<&OperationTarget, (With<ReconcileEntry>, With<ConflictCopyEntry>)>,
    mut commands: Commands,
) {
    for (session, mut counts, wake) in &mut sessions {
        if progress.has_pending(session)
            || conflicts.iter().any(|target| target.entity() == session)
        {
            continue;
        }
        counts.automatic_merges = merges
            .iter()
            .filter(|target| target.entity() == session)
            .count();
        counts.conflict_copies = copies
            .iter()
            .filter(|target| target.entity() == session)
            .count();
        wake.notify();
        commands
            .entity(session)
            .remove::<ReconcileMerging>()
            .insert(ReconcileValidating);
    }
}

fn validate_tree(
    sessions: Query<
        (Entity, &ReconcileCounts, &VaultWake),
        (
            With<ReconcileSession>,
            With<ReconcileValidating>,
            Without<ReconcileCanceling>,
        ),
    >,
    entries: Query<
        (
            &OperationTarget,
            &RelativePath,
            &CapturedLocal,
            Option<&ProposedMutation>,
        ),
        With<ReconcileEntry>,
    >,
    mut commands: Commands,
) {
    for (session, counts, wake) in &sessions {
        let mut resulting = BTreeSet::new();
        let mut mutation_count = 0usize;
        let mut error = None;
        for (target, path, local, proposed) in &entries {
            if target.entity() != session {
                continue;
            }
            if let Some(proposed) = proposed {
                mutation_count += 1;
                if let Err(err) = SnapshotPath(&path.0).validate() {
                    error = Some(err);
                    break;
                }
                if proposed.0.is_some() {
                    resulting.insert(path.0.clone());
                }
            } else if local.0.is_some() {
                resulting.insert(path.0.clone());
            }
        }
        if error.is_none() {
            for path in &resulting {
                let mut parent = Path::new(path).parent();
                while let Some(candidate) = parent {
                    if let Some(candidate) = candidate.to_str()
                        && resulting.contains(candidate)
                    {
                        error = Some(format!(
                            "Vault has incompatible file and directory changes: {candidate}, {path}"
                        ));
                        break;
                    }
                    parent = candidate.parent();
                }
                if error.is_some() {
                    break;
                }
            }
        }
        commands.entity(session).remove::<ReconcileValidating>();
        if let Some(error) = error {
            commands.entity(session).insert(ReconcileDone(Err(error)));
        } else if mutation_count == 0 {
            commands.entity(session).insert(ReconcileDone(Ok(*counts)));
        } else {
            commands.entity(session).insert(ReconcileVerifying);
        }
        wake.notify();
    }
}

fn submit_verify_fingerprints(
    sessions: Query<
        (Entity, &ReconcileRoot, &VaultWake),
        (
            With<ReconcileSession>,
            With<ReconcileVerifying>,
            Without<VerifyFingerprintsTask>,
            Without<ReconcileCanceling>,
        ),
    >,
    entries: Query<
        (&OperationTarget, &RelativePath, &CapturedLocal),
        (With<ReconcileEntry>, With<ProposedMutation>),
    >,
    mut commands: Commands,
) {
    for (session, root, wake) in &sessions {
        let mut expected = Vec::new();
        for (target, path, local) in &entries {
            if target.entity() != session {
                continue;
            }
            let fingerprint = local.0.as_ref().map(|entry| LocalFingerprint {
                kind: entry.kind,
                mode: entry.mode,
                size: entry.size,
                modified_secs: entry.modified_secs,
                modified_nanos: entry.modified_nanos,
            });
            expected.push((path.0.clone(), fingerprint));
        }
        let root = root.0.clone();
        let task = wake.spawn(move || VerifyFingerprintsTask::verify(&root, &expected));
        commands
            .entity(session)
            .insert(VerifyFingerprintsTask(task));
    }
}

impl VerifyFingerprintsTask {
    fn verify(root: &Path, expected: &[(String, Option<LocalFingerprint>)]) -> Result<(), String> {
        for (relative, expected) in expected {
            let path = root.join(relative);
            let observed = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    let kind = if metadata.file_type().is_symlink() {
                        EntryKind::Symlink
                    } else if metadata.file_type().is_file() {
                        EntryKind::File
                    } else {
                        return Err(format!("Vault local file changed during sync: {relative}"));
                    };
                    let (modified_secs, modified_nanos) = FileTimestamp(&metadata).get();
                    Some(LocalFingerprint {
                        kind,
                        mode: FileAttributes::mode(&metadata),
                        size: metadata.len(),
                        modified_secs,
                        modified_nanos,
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            let matches = match (expected, observed) {
                (Some(expected), Some(observed)) => {
                    expected.kind == observed.kind
                        && expected.mode == observed.mode
                        && expected.size == observed.size
                        && expected.modified_secs == observed.modified_secs
                        && expected.modified_nanos == observed.modified_nanos
                }
                (None, None) => true,
                _ => false,
            };
            if !matches {
                return Err(format!("Vault local file changed during sync: {relative}"));
            }
        }
        Ok(())
    }
}

fn drain_verify_fingerprints(
    mut sessions: Query<
        (
            Entity,
            &VaultWake,
            Has<ReconcileCanceling>,
            &mut VerifyFingerprintsTask,
        ),
        With<ReconcileSession>,
    >,
    mut commands: Commands,
) {
    for (session, wake, canceling, mut task) in &mut sessions {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands
            .entity(session)
            .remove::<(VerifyFingerprintsTask, ReconcileVerifying)>();
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(()) => {
                commands.entity(session).insert(ReconcileApplying);
            }
            Err(error) => {
                commands.entity(session).insert(ReconcileDone(Err(error)));
            }
        }
        wake.notify();
    }
}

fn submit_apply_mutation(
    sessions: Query<
        (Entity, &ReconcileRoot, &ReconcileCounts, &VaultWake),
        (
            With<ReconcileSession>,
            With<ReconcileApplying>,
            Without<ReconcileCanceling>,
        ),
    >,
    in_flight: Query<&OperationTarget, (With<ReconcileEntry>, With<ApplyMutationTask>)>,
    pending: Query<
        (Entity, &OperationTarget, &RelativePath, &ProposedMutation),
        (
            With<ReconcileEntry>,
            Without<AppliedMutation>,
            Without<ApplyMutationTask>,
        ),
    >,
    mut commands: Commands,
) {
    for (session, root, counts, wake) in &sessions {
        if in_flight.iter().any(|target| target.entity() == session) {
            continue;
        }
        let mut next: Option<(Entity, &str, &Option<LocalEntry>)> = None;
        for (entity, target, path, mutation) in &pending {
            if target.entity() != session {
                continue;
            }
            if next.is_none_or(|(_, best, _)| path.0.as_str() < best) {
                next = Some((entity, path.0.as_str(), &mutation.0));
            }
        }
        let Some((entity, relative, entry)) = next else {
            commands
                .entity(session)
                .remove::<ReconcileApplying>()
                .insert(ReconcileDone(Ok(*counts)));
            wake.notify();
            continue;
        };
        let root = root.0.clone();
        let relative = relative.to_string();
        let entry = entry.clone();
        let task = wake.spawn(move || ApplyMutationTask::apply(&root, &relative, entry.as_ref()));
        commands.entity(entity).insert(ApplyMutationTask(task));
    }
}

impl ApplyMutationTask {
    fn apply(root: &Path, relative: &str, entry: Option<&LocalEntry>) -> Result<(), String> {
        SnapshotPath(relative).validate()?;
        let path = root.join(relative);
        match entry {
            Some(entry) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                LocalPath(&path).remove()?;
                match entry.kind {
                    EntryKind::File => {
                        vmux_path::AtomicFile::write(&path, &entry.data)
                            .map_err(|error| error.to_string())?;
                        FileAttributes::set_mode(&path, entry.mode)?;
                    }
                    EntryKind::Symlink => FileAttributes::create_symlink(&path, &entry.data)?,
                }
            }
            None => {
                LocalPath(&path).remove()?;
                let mut parent = path.parent();
                while let Some(directory) = parent {
                    if directory == root || !directory.starts_with(root) {
                        break;
                    }
                    if std::fs::remove_dir(directory).is_err() {
                        break;
                    }
                    parent = directory.parent();
                }
            }
        }
        Ok(())
    }
}

fn drain_apply_mutation(
    mut tasks: Query<(Entity, &OperationTarget, &mut ApplyMutationTask), With<ReconcileEntry>>,
    sessions: Query<(&VaultWake, Has<ReconcileCanceling>), With<ReconcileSession>>,
    mut commands: Commands,
) {
    for (entity, target, mut task) in &mut tasks {
        let Some(result) = future::block_on(future::poll_once(&mut task.0)) else {
            continue;
        };
        commands.entity(entity).remove::<ApplyMutationTask>();
        let Ok((wake, canceling)) = sessions.get(target.entity()) else {
            continue;
        };
        if canceling {
            wake.notify();
            continue;
        }
        match result {
            Ok(()) => {
                commands.entity(entity).insert(AppliedMutation);
            }
            Err(error) => {
                commands
                    .entity(target.entity())
                    .remove::<ReconcileApplying>()
                    .insert(ReconcileDone(Err(error)));
            }
        }
        wake.notify();
    }
}

fn drain_canceled_session(
    sessions: Query<
        (Entity, &VaultWake),
        (
            With<ReconcileSession>,
            With<ReconcileCanceling>,
            Without<HostnameTask>,
            Without<VerifyFingerprintsTask>,
            Without<ReconcileDone>,
        ),
    >,
    text_tasks: Query<&OperationTarget, With<TextMergeTask>>,
    apply_tasks: Query<&OperationTarget, With<ApplyMutationTask>>,
    mut commands: Commands,
) {
    for (session, wake) in &sessions {
        let busy = text_tasks.iter().any(|target| target.entity() == session)
            || apply_tasks.iter().any(|target| target.entity() == session);
        if busy {
            continue;
        }
        commands
            .entity(session)
            .remove::<ReconcileCanceling>()
            .insert(ReconcileDone(Err(
                "Vault reconciliation canceled".to_string()
            )));
        wake.notify();
    }
}

fn retire_completed(
    sessions: Query<Entity, (With<ReconcileSession>, With<ReconcileRetire>)>,
    entries: Query<(Entity, &OperationTarget), With<ReconcileEntry>>,
    ron_fields: Query<(Entity, &OperationTarget), With<RonField>>,
    toml_fields: Query<(Entity, &OperationTarget), With<TomlField>>,
    json_fields: Query<(Entity, &OperationTarget), With<JsonField>>,
    mut commands: Commands,
) {
    for session in &sessions {
        let mut entry_set = BTreeSet::new();
        for (entity, target) in &entries {
            if target.entity() == session {
                entry_set.insert(entity);
            }
        }
        for (entity, target) in &ron_fields {
            if entry_set.contains(&target.entity()) {
                commands.entity(entity).despawn();
            }
        }
        for (entity, target) in &toml_fields {
            if entry_set.contains(&target.entity()) {
                commands.entity(entity).despawn();
            }
        }
        for (entity, target) in &json_fields {
            if entry_set.contains(&target.entity()) {
                commands.entity(entity).despawn();
            }
        }
        for entity in entry_set {
            commands.entity(entity).despawn();
        }
        commands.entity(session).despawn();
    }
}
