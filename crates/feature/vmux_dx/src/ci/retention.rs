use super::{Arguments, Completed, Context, Io, Root, Sequence, Stage};
use crate::Settings;
use crate::ci_io::fs::{Print, Written};
use crate::ci_io::process::Captured;
use bevy_app::{App, Update};
use bevy_ecs::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vmux_ecs::cli::CliInvocation;

pub(super) struct CiRetentionPlugin;

impl bevy_app::Plugin for CiRetentionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (start, verify_closed, collect, plan, delete, finish)
                .chain()
                .in_set(Stage::Step),
        );
    }
}

#[derive(Component)]
struct Retention {
    repository: String,
    apply: bool,
    closed_pr: Option<u64>,
    page: u32,
    total: Option<u64>,
    current: Option<Entity>,
}

#[derive(Component, Deserialize)]
struct Entry {
    id: u64,
    key: String,
    #[serde(rename = "ref")]
    reference: String,
    created_at: String,
    last_accessed_at: String,
    size_in_bytes: u64,
}

impl Entry {
    fn group(&self) -> Option<String> {
        if !self.key.starts_with("vmux-source-v4-") {
            return None;
        }
        let (prefix, sha) = self.key.rsplit_once('-')?;
        let (prefix, inputs) = prefix.rsplit_once('-')?;
        let (_, compiler) = prefix.rsplit_once('-')?;
        for (value, length) in [(sha, 40), (inputs, 64), (compiler, 64)] {
            if value.len() != length
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return None;
            }
        }
        Some(prefix.into())
    }
}

#[derive(Component)]
struct EntryOwner(Entity);
#[derive(Component)]
struct Ready;
#[derive(Component)]
struct Listed;
#[derive(Component)]
struct Planned;
#[derive(Component)]
struct Prunable;
#[derive(Component)]
struct Deleted;
#[derive(Component)]
struct PullSlot;
#[derive(Component)]
struct ListSlot;
#[derive(Component)]
struct DeleteSlot;
#[derive(Component)]
struct PrintSlot;

fn start(mut requests: StartRequests<'_, '_>, mut io: Io) {
    for (entity, invocation, context, settings, mut sequence) in &mut requests {
        if !invocation.is("dx.ci.cache-prune") {
            continue;
        }
        let apply = match Arguments::values(invocation) {
            [] => false,
            [value] if value == "--apply" => true,
            _ => {
                io.fail(entity, "cache-prune accepts only --apply");
                continue;
            }
        };
        let repository = match Arguments::text(&settings.0, "GITHUB_REPOSITORY") {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        let parts: Vec<_> = repository.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
            })
        {
            io.fail(entity, "Invalid cache repository");
            continue;
        }
        let closed_pr = match settings.0.get("VMUX_CACHE_CLOSED_PR") {
            None => None,
            Some(value) if value.is_empty() => None,
            Some(value) => match value
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
            {
                Some(value) => Some(value),
                None => {
                    io.fail(entity, "Invalid closed PR number");
                    continue;
                }
            },
        };
        if let Some(number) = closed_pr {
            io.request(
                entity,
                &mut sequence,
                context.process(
                    &settings.0,
                    "gh",
                    &["api", &format!("repos/{repository}/pulls/{number}")],
                ),
                PullSlot,
            );
        } else {
            io.commands.entity(entity).insert(Ready);
        }
        io.commands.entity(entity).insert(Retention {
            repository,
            apply,
            closed_pr,
            page: 0,
            total: None,
            current: None,
        });
    }
}

fn verify_closed(
    requests: ClosedRequests<'_, '_>,
    results: super::Reads<Captured, PullSlot>,
    mut io: Io,
) {
    for (entity, retention) in &requests {
        let Some(output) = results.get(entity) else {
            continue;
        };
        let pull: serde_json::Value = match output
            .text()
            .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        {
            Ok(value) => value,
            Err(error) => {
                io.fail(entity, error);
                continue;
            }
        };
        if pull["number"].as_u64() != retention.closed_pr
            || pull["state"].as_str() != Some("closed")
        {
            io.fail(entity, "PR is not closed; caches retained");
            continue;
        }
        io.commands.entity(entity).insert(Ready);
    }
}

fn collect(
    mut requests: ListRequests<'_, '_>,
    entries: Entries<'_, '_>,
    results: super::Reads<Captured, ListSlot>,
    mut io: Io,
) {
    for (entity, context, settings, mut retention, mut sequence) in &mut requests {
        if retention.page > 0 {
            let Some(output) = results.get(entity) else {
                continue;
            };
            let value: serde_json::Value = match output
                .text()
                .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
            {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error);
                    continue;
                }
            };
            let Some(total) = value["total_count"].as_u64() else {
                io.fail(entity, "Incomplete cache inventory");
                continue;
            };
            let rows: Vec<Entry> = match serde_json::from_value(value["actions_caches"].clone()) {
                Ok(value) => value,
                Err(error) => {
                    io.fail(entity, error.to_string());
                    continue;
                }
            };
            if total > 2000 || retention.total.is_some_and(|previous| previous != total) {
                io.fail(
                    entity,
                    "Cache inventory changed or exceeds bound; caches retained",
                );
                continue;
            }
            retention.total = Some(total);
            let existing: Vec<_> = entries
                .iter()
                .filter(|(owner, _)| owner.0 == entity)
                .map(|(_, entry)| entry.id)
                .collect();
            let mut seen = BTreeSet::new();
            if rows.iter().any(|row| {
                row.id == 0
                    || existing.contains(&row.id)
                    || !seen.insert(row.id)
                    || retention
                        .closed_pr
                        .is_some_and(|number| row.reference != format!("refs/pull/{number}/merge"))
            }) {
                io.fail(entity, "Cache inventory identity differs; caches retained");
                continue;
            }
            let count = existing.len() as u64 + rows.len() as u64;
            if count > total || rows.is_empty() && count < total {
                io.fail(entity, "Incomplete cache page; caches retained");
                continue;
            }
            for row in rows {
                io.commands
                    .spawn((row, EntryOwner(entity), ChildOf(entity)));
            }
            if count == total {
                io.commands.entity(entity).insert(Listed);
                continue;
            }
        }
        retention.page += 1;
        let mut endpoint = format!(
            "repos/{}/actions/caches?per_page=100&page={}",
            retention.repository, retention.page
        );
        if let Some(number) = retention.closed_pr {
            endpoint.push_str(&format!("&ref=refs%2Fpull%2F{number}%2Fmerge"));
        }
        io.request(
            entity,
            &mut sequence,
            context.process(&settings.0, "gh", &["api", &endpoint]),
            ListSlot,
        );
    }
}

fn plan(mut requests: PlanRequests<'_, '_>, entries: EntriesWithIds<'_, '_>, mut io: Io) {
    for (entity, retention, mut sequence) in &mut requests {
        let mut groups: BTreeMap<(String, String), Vec<(Entity, &Entry)>> = BTreeMap::new();
        let mut selected = Vec::new();
        for (id, owner, entry) in &entries {
            if owner.0 != entity {
                continue;
            }
            if retention.closed_pr.is_some() {
                selected.push((id, entry));
                continue;
            }
            if entry.reference != "refs/heads/main" && !entry.reference.starts_with("refs/pull/") {
                continue;
            }
            if let Some(group) = entry.group() {
                groups
                    .entry((entry.reference.clone(), group))
                    .or_default()
                    .push((id, entry));
            }
        }
        for ((reference, _), mut group) in groups {
            group.sort_by(|(_, left), (_, right)| {
                right
                    .created_at
                    .cmp(&left.created_at)
                    .then(right.id.cmp(&left.id))
            });
            let keep = if reference == "refs/heads/main" { 1 } else { 2 };
            selected.extend(group.into_iter().skip(keep));
        }
        let mut report = Vec::new();
        for (id, entry) in selected {
            io.commands.entity(id).insert(Prunable);
            report.push(serde_json::json!({"id": entry.id, "key": entry.key, "ref": entry.reference, "bytes": entry.size_in_bytes, "last_accessed_at": entry.last_accessed_at}));
        }
        let output =
            serde_json::json!({"apply": retention.apply, "delete": report}).to_string() + "\n";
        io.emit(entity, &mut sequence, Print(output.into_bytes()), PrintSlot);
        io.commands.entity(entity).insert(Planned);
    }
}

fn delete(
    mut requests: DeleteRequests<'_, '_>,
    candidates: Candidates<'_, '_>,
    results: super::Reads<Captured, DeleteSlot>,
    mut io: Io,
) {
    for (entity, context, settings, mut retention, mut sequence) in &mut requests {
        if !retention.apply {
            continue;
        }
        if let Some(current) = retention.current {
            let Some(output) = results.get(entity) else {
                continue;
            };
            if let Err(error) = output.text() {
                io.fail(entity, error);
                continue;
            }
            io.commands.entity(current).insert(Deleted);
            retention.current = None;
            continue;
        }
        let Some((id, _, entry)) = candidates
            .iter()
            .filter(|(_, owner, _)| owner.0 == entity)
            .min_by_key(|(_, _, entry)| entry.id)
        else {
            continue;
        };
        io.request(
            entity,
            &mut sequence,
            context.process(
                &settings.0,
                "gh",
                &[
                    "api",
                    "--method",
                    "DELETE",
                    &format!("repos/{}/actions/caches/{}", retention.repository, entry.id),
                ],
            ),
            DeleteSlot,
        );
        retention.current = Some(id);
    }
}

fn finish(
    requests: FinishRequests<'_, '_>,
    candidates: Candidates<'_, '_>,
    prints: super::Reads<Written, PrintSlot>,
    mut io: Io,
) {
    for (entity, retention) in &requests {
        if prints.get(entity).is_none() || retention.current.is_some() {
            continue;
        }
        if retention.apply && candidates.iter().any(|(_, owner, _)| owner.0 == entity) {
            continue;
        }
        io.commands.entity(entity).insert(Completed(Ok(0)));
    }
}

type StartRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static CliInvocation,
        &'static Context,
        &'static Settings,
        &'static mut Sequence,
    ),
    (With<Root>, Without<Retention>, Without<Completed>),
>;
type ClosedRequests<'w, 's> =
    Query<'w, 's, (Entity, &'static Retention), (With<Root>, Without<Ready>, Without<Completed>)>;
type ListRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Settings,
        &'static mut Retention,
        &'static mut Sequence,
    ),
    (With<Root>, With<Ready>, Without<Listed>, Without<Completed>),
>;
type PlanRequests<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static Retention, &'static mut Sequence),
    (
        With<Root>,
        With<Listed>,
        Without<Planned>,
        Without<Completed>,
    ),
>;
type DeleteRequests<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Context,
        &'static Settings,
        &'static mut Retention,
        &'static mut Sequence,
    ),
    (With<Root>, With<Planned>, Without<Completed>),
>;
type FinishRequests<'w, 's> =
    Query<'w, 's, (Entity, &'static Retention), (With<Root>, With<Planned>, Without<Completed>)>;
type Entries<'w, 's> = Query<'w, 's, (&'static EntryOwner, &'static Entry)>;
type EntriesWithIds<'w, 's> = Query<'w, 's, (Entity, &'static EntryOwner, &'static Entry)>;
type Candidates<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static EntryOwner, &'static Entry),
    (With<Prunable>, Without<Deleted>),
>;

#[cfg(test)]
mod tests {
    use crate::ci::testing::Fixture;
    use serde_json::json;

    #[test]
    fn retention_preserves_latest_main_and_two_pr_generations_and_unknown_cache_types() {
        let mut fixture = Fixture::new();
        fixture.environment.set("VMUX_CACHE_CLOSED_PR", "");
        let prefix = format!(
            "vmux-source-v4-Linux-X64-test-{}-{}",
            "c".repeat(64),
            "d".repeat(64)
        );
        let mut rows = Vec::new();
        for (id, reference, created) in [
            (1, "refs/heads/main", "2026-01-01T00:00:00Z"),
            (2, "refs/heads/main", "2026-01-02T00:00:00Z"),
            (3, "refs/pull/5/merge", "2026-01-01T00:00:00Z"),
            (4, "refs/pull/5/merge", "2026-01-02T00:00:00Z"),
            (5, "refs/pull/5/merge", "2026-01-03T00:00:00Z"),
        ] {
            rows.push(json!({"id": id, "key": format!("{prefix}-{id:040x}"), "ref": reference, "created_at": created, "last_accessed_at": created, "size_in_bytes": 100}));
        }
        rows.push(json!({"id": 6, "key": "sccache-content-key", "ref": "refs/heads/main", "created_at": "2026-01-01T00:00:00Z", "last_accessed_at": "2026-01-01T00:00:00Z", "size_in_bytes": 10}));
        rows.push(json!({"id": 7, "key": format!("{prefix}-{}", "e".repeat(40)), "ref": "refs/heads/feature", "created_at": "2026-01-01T00:00:00Z", "last_accessed_at": "2026-01-01T00:00:00Z", "size_in_bytes": 10}));
        fixture.json(
            "caches.json",
            &json!({"total_count": rows.len(), "actions_caches": rows}),
        );
        fixture.mock_gh("#!/bin/sh\ncase \"$*\" in\n*DELETE*) echo \"$*\" >> deleted ;;\n*) cat caches.json ;;\nesac\n");
        let output = fixture.run("cache-prune", &[]).unwrap();
        let report: serde_json::Value = serde_json::from_str(&output).unwrap();
        let ids: Vec<_> = report["delete"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1));
        assert!(ids.contains(&3));
        assert!(!fixture.root().join("deleted").exists());
        fixture.run("cache-prune", &["--apply"]).unwrap();
        let deleted = std::fs::read_to_string(fixture.root().join("deleted")).unwrap();
        assert_eq!(
            deleted.lines().collect::<Vec<_>>(),
            [
                "api --method DELETE repos/fixture/repository/actions/caches/1",
                "api --method DELETE repos/fixture/repository/actions/caches/3"
            ]
        );
    }

    #[test]
    fn closed_pr_cleanup_refuses_open_prs_foreign_branches_and_incomplete_pages() {
        let mut fixture = Fixture::new();
        fixture.environment.set("VMUX_CACHE_CLOSED_PR", "5");
        fixture.json("pull.json", &json!({"number": 5, "state": "open"}));
        fixture.json(
            "caches.json",
            &json!({"total_count": 0, "actions_caches": []}),
        );
        fixture.mock_gh("#!/bin/sh\ncase \"$*\" in\n*DELETE*) echo deleted >> deleted ;;\n*pulls*) cat pull.json ;;\n*) cat caches.json ;;\nesac\n");
        assert!(
            fixture
                .run("cache-prune", &["--apply"])
                .unwrap_err()
                .contains("not closed")
        );
        assert!(!fixture.root().join("deleted").exists());
        fixture.json("pull.json", &json!({"number": 5, "state": "closed"}));
        fixture.run("cache-prune", &["--apply"]).unwrap();
        fixture.json("caches.json", &json!({"total_count": 1, "actions_caches": [{"id": 1, "key": "cache", "ref": "refs/pull/6/merge", "created_at": "2026-01-01T00:00:00Z", "last_accessed_at": "2026-01-01T00:00:00Z", "size_in_bytes": 1}]}));
        assert!(
            fixture
                .run("cache-prune", &["--apply"])
                .unwrap_err()
                .contains("identity")
        );
        assert!(!fixture.root().join("deleted").exists());
        fixture.json(
            "caches.json",
            &json!({"total_count": 1, "actions_caches": []}),
        );
        assert!(
            fixture
                .run("cache-prune", &["--apply"])
                .unwrap_err()
                .contains("Incomplete")
        );
        assert!(!fixture.root().join("deleted").exists());
    }

    #[test]
    fn paginated_retention_waits_for_the_complete_inventory_before_deleting() {
        let mut fixture = Fixture::new();
        fixture.environment.set("VMUX_CACHE_CLOSED_PR", "");
        let prefix = format!(
            "vmux-source-v4-Linux-X64-test-{}-{}",
            "c".repeat(64),
            "d".repeat(64)
        );
        let mut rows = Vec::new();
        for id in 1..=3 {
            rows.push(json!({"id": id, "key": format!("{prefix}-{id:040x}"), "ref": "refs/heads/main", "created_at": format!("2026-01-0{id}T00:00:00Z"), "last_accessed_at": "2026-01-01T00:00:00Z", "size_in_bytes": 100}));
        }
        fixture.json(
            "first.json",
            &json!({"total_count": 3, "actions_caches": &rows[..2]}),
        );
        fixture.json(
            "second.json",
            &json!({"total_count": 3, "actions_caches": &rows[2..]}),
        );
        fixture.mock_gh("#!/bin/sh\ncase \"$*\" in\n*DELETE*) test -f complete || exit 1; echo \"$*\" >> deleted ;;\n*'&page=1') cat first.json ;;\n*'&page=2') touch complete; cat second.json ;;\n*) exit 1 ;;\nesac\n");
        fixture.run("cache-prune", &["--apply"]).unwrap();
        assert_eq!(
            std::fs::read_to_string(fixture.root().join("deleted")).unwrap(),
            "api --method DELETE repos/fixture/repository/actions/caches/1\napi --method DELETE repos/fixture/repository/actions/caches/2\n"
        );
        std::fs::remove_file(fixture.root().join("deleted")).unwrap();
        for response in [
            json!({"total_count": 4, "actions_caches": &rows[2..]}),
            json!({"total_count": 3, "actions_caches": &rows[..1]}),
            json!({"total_count": 3, "actions_caches": []}),
        ] {
            fixture.json("second.json", &response);
            assert!(fixture.run("cache-prune", &["--apply"]).is_err());
            assert!(!fixture.root().join("deleted").exists());
        }
    }

    #[test]
    fn closed_pr_cleanup_deletes_only_verified_cache_ids_and_reports_delete_failures() {
        let mut fixture = Fixture::new();
        fixture.environment.set("VMUX_CACHE_CLOSED_PR", "5");
        fixture.json("pull.json", &json!({"number": 5, "state": "closed"}));
        let row = json!({"id": 1, "key": "opaque-cache-key", "ref": "refs/pull/5/merge", "created_at": "2026-01-01T00:00:00Z", "last_accessed_at": "2026-01-01T00:00:00Z", "size_in_bytes": 1});
        fixture.mock_gh("#!/bin/sh\ncase \"$*\" in\n*DELETE*) echo \"$*\" >> deleted; test ! -f refuse ;;\n*pulls/5*) cat pull.json ;;\n*ref=refs%2Fpull%2F5%2Fmerge*) cat caches.json ;;\n*) exit 1 ;;\nesac\n");
        for response in [
            json!({"total_count": 2, "actions_caches": [&row, &row]}),
            json!({"total_count": 2001, "actions_caches": [&row]}),
        ] {
            fixture.json("caches.json", &response);
            assert!(fixture.run("cache-prune", &["--apply"]).is_err());
            assert!(!fixture.root().join("deleted").exists());
        }
        fixture.json(
            "caches.json",
            &json!({"total_count": 1, "actions_caches": [&row]}),
        );
        fixture.run("cache-prune", &["--apply"]).unwrap();
        assert_eq!(
            std::fs::read_to_string(fixture.root().join("deleted")).unwrap(),
            "api --method DELETE repos/fixture/repository/actions/caches/1\n"
        );
        fixture.write("refuse", "");
        assert!(fixture.run("cache-prune", &["--apply"]).is_err());
    }
}
