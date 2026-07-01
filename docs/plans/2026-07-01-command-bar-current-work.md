# Command Bar "Current Work" Section — Implementation Plan

> **For agentic workers:** Implement directly in this session (vmux CEF builds are heavy; do NOT subagent-drive). Warm the target dir with a background `cargo build` first, then incremental. Defer runtime testing to ONE pass at the end. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add a "current work" section to the command bar (Cmd+K modal **and** `vmux://start`), rendered after the vmux:// page entries: open-pane working directories, then recently-opened files (persisted via the existing browser history store).

**Architecture:** File opens are recorded into the existing `Visit`/`Url` ECS history via a new `vmux_core` `RecordVisitRequest` message (editor produces, `vmux_history` consumes — shares one `record_visit` helper with `spawn_visits`). A new `CommandBarWorkSnapshot` resource (in `vmux_command`, populated by two `vmux_layout` backend systems in the `WriteCommandBarSnapshots` set) carries open-pane dirs + top-N `file://` history. It rides the existing `CommandBarOpenEvent` payload into the shared `CommandPalette`/`filter_results` frontend, which both surfaces use.

**Tech Stack:** Rust, Bevy ECS, Dioxus/WASM, rkyv/serde, moonshine-save.

Spec: `docs/specs/2026-07-01-command-bar-current-work-design.md`.

---

## File map

- Create: `crates/vmux_layout/src/command_bar/work_snapshot.rs` — the two snapshot updater systems (backend).
- Modify: `crates/vmux_core/src/event.rs` — add `RecordVisitRequest` message.
- Modify: `crates/vmux_history/src/spawn.rs` — extract `record_visit`, add `record_requested_visits`.
- Modify: `crates/vmux_history/src/plugin.rs` — register message + system.
- Modify: `crates/vmux_editor/src/plugin.rs` — emit `RecordVisitRequest` on file open.
- Modify: `crates/vmux_desktop/src/persistence.rs` — dirty on `Added<Visit>`.
- Modify: `crates/vmux_command/src/event.rs` — `CommandBarWorkDir`, `CommandBarRecentFile`, `CommandBarOpenEvent` fields.
- Modify: `crates/vmux_command/src/snapshot.rs` — `CommandBarWorkSnapshot` resource.
- Modify: `crates/vmux_command/src/plugin.rs` — `init_resource::<CommandBarWorkSnapshot>()`.
- Modify: `crates/vmux_layout/src/command_bar.rs` — declare `work_snapshot` module (backend).
- Modify: `crates/vmux_layout/src/command_bar/handler.rs` — thread work snapshot into payload; register updaters; `focus_dir` action.
- Modify: `crates/vmux_layout/src/start/plugin.rs` — pass work snapshot into start payload.
- Modify: `crates/vmux_layout/src/command_bar/results.rs` — result variants + `filter_results` ordering.
- Modify: `crates/vmux_layout/src/command_bar/palette.rs` — render + dispatch the new variants.

Wire types (`{ path, kind_label }`, `{ url, title }`) are reused verbatim by both the snapshot resource and the payload, mirroring how `CommandBarPagesSnapshot` stores `Vec<CommandBarPage>`.

---

## Task 1: `RecordVisitRequest` message (vmux_core)

**Files:**
- Modify: `crates/vmux_core/src/event.rs`

- [ ] **Step 1: Add the message.** Append to `crates/vmux_core/src/event.rs`:

```rust
/// Request to record a page/file visit into browser history (the `Visit`/`Url`
/// ECS store). Sent by the editor when a `file://` view opens so file opens are
/// persisted like any browser navigation; consumed by `vmux_history`.
#[derive(bevy::prelude::Message, Clone, Debug, PartialEq, Eq)]
pub struct RecordVisitRequest {
    pub url: String,
    pub title: String,
}
```

- [ ] **Step 2: Build.** Run: `cargo build -p vmux_core`. Expected: compiles.

- [ ] **Step 3: Commit.**

```bash
git add crates/vmux_core/src/event.rs
git commit -m "feat(core): RecordVisitRequest message for history recording"
```

---

## Task 2: Shared `record_visit` + `record_requested_visits` (vmux_history)

**Files:**
- Modify: `crates/vmux_history/src/spawn.rs`
- Modify: `crates/vmux_history/src/plugin.rs`

- [ ] **Step 1: Write failing test** for `record_requested_visits`. Add to the `system_tests` module in `spawn.rs` (after the existing tests):

```rust
    #[test]
    fn record_request_spawns_url_with_title() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CorePlugin)
            .add_message::<vmux_core::event::RecordVisitRequest>()
            .add_systems(Update, record_requested_visits);
        app.world_mut()
            .resource_mut::<Messages<vmux_core::event::RecordVisitRequest>>()
            .write(vmux_core::event::RecordVisitRequest {
                url: "file:///Users/me/main.rs".into(),
                title: "main.rs".into(),
            });
        app.update();
        let mut q = app
            .world_mut()
            .query::<(&PageMetadata, &VisitCount)>();
        let (meta, count) = q.iter(app.world()).next().expect("url recorded");
        assert_eq!(meta.url, "file:///Users/me/main.rs");
        assert_eq!(meta.title, "main.rs");
        assert_eq!(count.0, 1);
    }
```

- [ ] **Step 2: Run it — fails to compile** (`record_requested_visits` missing). Run: `cargo test -p vmux_history record_request_spawns_url_with_title`. Expected: compile error.

- [ ] **Step 3: Extract `record_visit` and add `record_requested_visits`.** In `spawn.rs`, replace the body of `spawn_visits` (lines 90-129, the `for ev in events.read()` loop) so it delegates, and add the helper + new system directly below `spawn_visits`:

```rust
pub fn spawn_visits(
    mut events: bevy::ecs::message::MessageReader<
        bevy_cef_core::prelude::WebviewCommittedNavigationEvent,
    >,
    mut commands: Commands,
    mut urls: Query<(Entity, &PageMetadata, &mut VisitCount, &mut LastVisitedAt), With<Url>>,
) {
    for ev in events.read() {
        if !ev.is_main_frame {
            continue;
        }
        if ev.url.starts_with("vmux://") || ev.url.is_empty() {
            continue;
        }
        let now = now_millis();
        let transition = crate::transition::map(ev.transition, ev.qualifiers);
        record_visit(&mut commands, &mut urls, &ev.url, "", transition, now);
    }
}

/// Find-or-create the `Url` entity for `url` (bumping `VisitCount`/`LastVisitedAt`),
/// then spawn a `Visit` unless this was a back/forward navigation. Sets the title on
/// newly-created urls (browser visits pass ""); existing urls keep their title.
pub(crate) fn record_visit(
    commands: &mut Commands,
    urls: &mut Query<(Entity, &PageMetadata, &mut VisitCount, &mut LastVisitedAt), With<Url>>,
    url: &str,
    title: &str,
    transition: TransitionType,
    now: i64,
) {
    let mut url_entity = None;
    for (e, meta, mut count, mut last) in urls.iter_mut() {
        if meta.url == url {
            count.0 = count.0.saturating_add(1);
            last.0 = now;
            url_entity = Some(e);
            break;
        }
    }
    let url_e = match url_entity {
        Some(e) => e,
        None => commands
            .spawn((
                Url,
                PageMetadata {
                    url: url.to_string(),
                    title: title.to_string(),
                    ..default()
                },
                VisitCount(1),
                LastVisitedAt(now),
                CreatedAt(now),
            ))
            .id(),
    };
    if transition != TransitionType::BackForward {
        commands.spawn((Visit, CreatedAt(now), VisitedUrl(url_e), transition));
    }
}

/// Record visits requested by other domains (the editor's `file://` opens) into the
/// same history store, so file opens persist and rank like browser navigations.
pub fn record_requested_visits(
    mut reader: bevy::ecs::message::MessageReader<vmux_core::event::RecordVisitRequest>,
    mut commands: Commands,
    mut urls: Query<(Entity, &PageMetadata, &mut VisitCount, &mut LastVisitedAt), With<Url>>,
) {
    let now = now_millis();
    for req in reader.read() {
        if req.url.is_empty() || req.url.starts_with("vmux://") {
            continue;
        }
        record_visit(
            &mut commands,
            &mut urls,
            &req.url,
            &req.title,
            TransitionType::Typed,
            now,
        );
    }
}
```

- [ ] **Step 4: Register in the plugin.** In `crates/vmux_history/src/plugin.rs`, update the import (line 19) and the systems/message registration (lines 28, 50-51):

```rust
use crate::spawn::{record_requested_visits, spawn_visits};
```

```rust
        app.world_mut().spawn(crate::PAGE_MANIFEST);
        app.add_systems(
            Update,
            (spawn_visits, record_requested_visits, broadcast_history_changed).chain(),
        )
```

and add to the `.add_message` calls at the end of `build` (next to `.add_message::<CefPageAttachRequest>()`):

```rust
            .add_message::<vmux_core::event::RecordVisitRequest>();
```

- [ ] **Step 5: Run tests.** Run: `cargo test -p vmux_history`. Expected: all pass (existing `spawn_visits` tests + the new one).

- [ ] **Step 6: Commit.**

```bash
git add crates/vmux_history/src/spawn.rs crates/vmux_history/src/plugin.rs
git commit -m "feat(history): record_visit helper + RecordVisitRequest consumer"
```

---

## Task 3: Editor emits `RecordVisitRequest` on file open (vmux_editor)

**Files:**
- Modify: `crates/vmux_editor/src/plugin.rs`

- [ ] **Step 1: Write failing test.** In the `page_open_tests` module, update the test app builder to register the message, then add a test. Replace the `app()` fn in `page_open_tests` (lines 1799-1806) with:

```rust
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_core::event::RecordVisitRequest>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<WebviewExtendStandardMaterial>>()
            .add_systems(Update, handle_file_page_open);
        app
    }
```

and add:

```rust
    #[test]
    fn file_open_records_history_visit() {
        use bevy::ecs::message::Messages;
        let mut app = app();
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "file:///etc/hostname#L3".to_string(),
            request_id: None,
        });
        app.update();
        let msgs = app
            .world()
            .resource::<Messages<vmux_core::event::RecordVisitRequest>>();
        let mut reader = msgs.get_cursor();
        let recorded: Vec<_> = reader.read(msgs).collect();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].url, "file:///etc/hostname");
        assert_eq!(recorded[0].title, "hostname");
    }
```

- [ ] **Step 2: Run it — fails** (no message written). Run: `cargo test -p vmux_editor file_open_records_history_visit`. Expected: assertion failure / panic on missing resource before the fix compiles the writer in.

- [ ] **Step 3: Emit the message.** In `handle_file_page_open` (lines 190-221), add a writer param and write after computing `clean_url`:

Change the signature to add the writer:

```rust
pub fn handle_file_page_open(
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    children_q: Query<&Children>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut webview_mt: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut record_writer: MessageWriter<vmux_core::event::RecordVisitRequest>,
) {
```

Insert immediately after `let clean_url = task.url.split('#').next().unwrap_or(&task.url).to_string();` (line 207):

```rust
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        record_writer.write(vmux_core::event::RecordVisitRequest {
            url: clean_url.clone(),
            title,
        });
```

- [ ] **Step 4: Register the message in `EditorPlugin`.** In `impl Plugin for EditorPlugin` `build` (around line 1647, chained with the other `insert_non_send`/`add_plugins`), add:

```rust
            .add_message::<vmux_core::event::RecordVisitRequest>()
```

(place it in the existing builder chain, e.g. right after `.insert_non_send(SelfWrites::default())`).

- [ ] **Step 5: Run tests.** Run: `cargo test -p vmux_editor`. Expected: all pass (existing + new).

- [ ] **Step 6: Commit.**

```bash
git add crates/vmux_editor/src/plugin.rs
git commit -m "feat(editor): record file:// opens into browser history"
```

---

## Task 4: Persist visits promptly (vmux_desktop)

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs`

- [ ] **Step 1: Write failing test.** Add to `persistence.rs` `tests` module (after `adding_archived_page_marks_store_dirty`):

```rust
    #[test]
    fn adding_visit_marks_store_dirty() {
        let mut app = App::new();
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_systems(Update, mark_dirty_on_change);
        app.update();
        app.world_mut().resource_mut::<AutoSave>().dirty = false;
        app.world_mut().spawn(vmux_history::Visit);
        app.update();
        assert!(app.world().resource::<AutoSave>().dirty);
    }
```

- [ ] **Step 2: Run it — fails.** Run: `cargo test -p vmux_desktop adding_visit_marks_store_dirty`. Expected: `assert!(dirty)` fails (visits don't dirty the store yet).

- [ ] **Step 3: Add the watcher.** In `mark_dirty_on_change` (lines 119-148) add a param and an OR clause:

Add to the parameter list (after `added_archived`):

```rust
    added_visits: Query<(), Added<vmux_history::Visit>>,
```

Add to the `if` condition (before the closing `{`):

```rust
        || !added_visits.is_empty()
```

- [ ] **Step 4: Run tests.** Run: `cargo test -p vmux_desktop adding_visit_marks_store_dirty adding_archived_page_marks_store_dirty`. Expected: both pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/vmux_desktop/src/persistence.rs
git commit -m "fix(persistence): flush store on new history visits"
```

---

## Task 5: Wire types + snapshot resource (vmux_command)

**Files:**
- Modify: `crates/vmux_command/src/event.rs`
- Modify: `crates/vmux_command/src/snapshot.rs`
- Modify: `crates/vmux_command/src/plugin.rs`

- [ ] **Step 1: Add wire structs + payload fields.** In `crates/vmux_command/src/event.rs`, add two structs (next to `CommandBarTab`) and two fields to `CommandBarOpenEvent`.

New structs:

```rust
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarWorkDir {
    pub path: String,
    pub kind_label: String,
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct CommandBarRecentFile {
    pub url: String,
    pub title: String,
}
```

Add to `CommandBarOpenEvent` (after the `pages` field, before `target`):

```rust
    #[serde(default)]
    pub work_dirs: Vec<CommandBarWorkDir>,
    #[serde(default)]
    pub recent_files: Vec<CommandBarRecentFile>,
```

- [ ] **Step 2: Add snapshot resource.** In `crates/vmux_command/src/snapshot.rs`, update the top import and add the resource:

```rust
use crate::event::{CommandBarPage, CommandBarRecentFile, CommandBarWorkDir};
```

```rust
#[derive(Resource, Default, Clone, Debug)]
pub struct CommandBarWorkSnapshot {
    pub work_dirs: Vec<CommandBarWorkDir>,
    pub recent_files: Vec<CommandBarRecentFile>,
}
```

- [ ] **Step 3: Init the resource.** In `crates/vmux_command/src/plugin.rs`, add to the `use crate::snapshot::{...}` import (line 5-8) `CommandBarWorkSnapshot`, and add `.init_resource::<CommandBarWorkSnapshot>()` in `build` (next to the other snapshot inits, lines 19-22).

- [ ] **Step 4: Add a round-trip test.** In `event.rs` `tests`, add:

```rust
    #[test]
    fn command_bar_open_event_carries_work_and_recent() {
        let event = CommandBarOpenEvent {
            work_dirs: vec![CommandBarWorkDir {
                path: "/work/proj".into(),
                kind_label: "Terminal".into(),
            }],
            recent_files: vec![CommandBarRecentFile {
                url: "file:///work/proj/main.rs".into(),
                title: "main.rs".into(),
            }],
            ..Default::default()
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).expect("ser");
        let recovered =
            rkyv::from_bytes::<CommandBarOpenEvent, rkyv::rancor::Error>(&bytes).expect("de");
        assert_eq!(recovered.work_dirs.len(), 1);
        assert_eq!(recovered.work_dirs[0].path, "/work/proj");
        assert_eq!(recovered.recent_files[0].title, "main.rs");
    }
```

- [ ] **Step 5: Run tests.** Run: `cargo test -p vmux_command`. Expected: all pass.

- [ ] **Step 6: Commit.**

```bash
git add crates/vmux_command/src/event.rs crates/vmux_command/src/snapshot.rs crates/vmux_command/src/plugin.rs
git commit -m "feat(command): CommandBarWorkSnapshot + work/recent wire types"
```

---

## Task 6: Snapshot updaters (vmux_layout backend)

**Files:**
- Create: `crates/vmux_layout/src/command_bar/work_snapshot.rs`
- Modify: `crates/vmux_layout/src/command_bar.rs`

- [ ] **Step 1: Create the updater file.** Write `crates/vmux_layout/src/command_bar/work_snapshot.rs`:

```rust
use bevy::prelude::*;
use vmux_command::event::{CommandBarRecentFile, CommandBarWorkDir};
use vmux_command::snapshot::CommandBarWorkSnapshot;
use vmux_core::terminal::{Terminal, TerminalKind, TerminalLaunch};
use vmux_core::{LastVisitedAt, PageMetadata, Url, VisitCount};
use vmux_history::LastActivatedAt;

/// How many entries each work-section group carries in the payload.
const WORK_GROUP_CAP: usize = 8;

/// Frecency: visit count decayed by recency (mirrors `vmux_history`'s ranking;
/// inlined to avoid depending on that crate's module visibility).
fn frecency(visit_count: u32, last_visited_at: i64, now: i64) -> f32 {
    let age_hours = ((now - last_visited_at).max(0) as f32) / 3_600_000.0;
    let decay = 1.0 / (1.0 + age_hours / 24.0);
    (visit_count as f32) * decay
}

fn kind_label(kind: &TerminalKind) -> &'static str {
    match kind {
        TerminalKind::Plain => "Terminal",
        TerminalKind::Vibe => "Vibe",
        TerminalKind::Claude => "Claude",
        TerminalKind::Codex => "Codex",
    }
}

/// Rebuild the open-pane working-dir list from every open terminal/agent (`Terminal`
/// entities carry `TerminalLaunch`), deduped by cwd, most-recently-active first.
pub fn update_work_dirs_snapshot(
    terminals: Query<(&TerminalLaunch, Option<&LastActivatedAt>), With<Terminal>>,
    mut snapshot: ResMut<CommandBarWorkSnapshot>,
) {
    let mut by_cwd: Vec<(String, &'static str, i64)> = Vec::new();
    for (launch, last) in &terminals {
        if launch.cwd.is_empty() {
            continue;
        }
        let ts = last.map(|l| l.0).unwrap_or(0);
        if let Some(existing) = by_cwd.iter_mut().find(|(p, _, _)| *p == launch.cwd) {
            if ts > existing.2 {
                existing.1 = kind_label(&launch.kind);
                existing.2 = ts;
            }
        } else {
            by_cwd.push((launch.cwd.clone(), kind_label(&launch.kind), ts));
        }
    }
    by_cwd.sort_by(|a, b| b.2.cmp(&a.2));
    let work_dirs: Vec<CommandBarWorkDir> = by_cwd
        .into_iter()
        .take(WORK_GROUP_CAP)
        .map(|(path, kind_label, _)| CommandBarWorkDir {
            path,
            kind_label: kind_label.to_string(),
        })
        .collect();
    if work_dirs != snapshot.work_dirs {
        snapshot.work_dirs = work_dirs;
    }
}

/// Rebuild the recent-files list: top-N `file://` history urls by frecency. Recomputes
/// only when a visit was added or a url's last-visited time changed.
pub fn update_recent_files_snapshot(
    changed: Query<(), Or<(Added<Url>, Changed<LastVisitedAt>)>>,
    urls: Query<(&PageMetadata, &VisitCount, &LastVisitedAt), With<Url>>,
    mut initialized: Local<bool>,
    mut snapshot: ResMut<CommandBarWorkSnapshot>,
) {
    if *initialized && changed.is_empty() {
        return;
    }
    *initialized = true;
    let now = vmux_core::now_millis();
    let mut scored: Vec<(f32, CommandBarRecentFile)> = urls
        .iter()
        .filter(|(meta, _, _)| meta.url.starts_with("file://"))
        .map(|(meta, count, last)| {
            (
                frecency(count.0, last.0, now),
                CommandBarRecentFile {
                    url: meta.url.clone(),
                    title: meta.title.clone(),
                },
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let recent_files: Vec<CommandBarRecentFile> = scored
        .into_iter()
        .take(WORK_GROUP_CAP)
        .map(|(_, f)| f)
        .collect();
    if recent_files != snapshot.recent_files {
        snapshot.recent_files = recent_files;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(cwd: &str, kind: TerminalKind) -> TerminalLaunch {
        TerminalLaunch {
            command: "/bin/zsh".into(),
            args: vec![],
            cwd: cwd.into(),
            env: vec![],
            kind,
        }
    }

    #[test]
    fn work_dirs_dedupe_by_cwd() {
        let mut app = App::new();
        app.init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Update, update_work_dirs_snapshot);
        app.world_mut()
            .spawn((Terminal, launch("/work/a", TerminalKind::Plain)));
        app.world_mut()
            .spawn((Terminal, launch("/work/a", TerminalKind::Vibe)));
        app.world_mut()
            .spawn((Terminal, launch("/work/b", TerminalKind::Plain)));
        app.update();
        let snap = app.world().resource::<CommandBarWorkSnapshot>();
        assert_eq!(snap.work_dirs.len(), 2);
        assert!(snap.work_dirs.iter().any(|d| d.path == "/work/a"));
        assert!(snap.work_dirs.iter().any(|d| d.path == "/work/b"));
    }

    #[test]
    fn recent_files_only_file_urls_ranked() {
        use vmux_core::CreatedAt;
        let mut app = App::new();
        app.init_resource::<CommandBarWorkSnapshot>()
            .add_systems(Update, update_recent_files_snapshot);
        app.world_mut().spawn((
            Url,
            PageMetadata { url: "https://example.com".into(), ..default() },
            VisitCount(9),
            LastVisitedAt(1000),
            CreatedAt(0),
        ));
        app.world_mut().spawn((
            Url,
            PageMetadata {
                url: "file:///work/main.rs".into(),
                title: "main.rs".into(),
                ..default()
            },
            VisitCount(1),
            LastVisitedAt(1000),
            CreatedAt(0),
        ));
        app.update();
        let snap = app.world().resource::<CommandBarWorkSnapshot>();
        assert_eq!(snap.recent_files.len(), 1);
        assert_eq!(snap.recent_files[0].title, "main.rs");
    }
}
```

- [ ] **Step 2: Declare the module.** In `crates/vmux_layout/src/command_bar.rs`, add alongside the other backend (`not(wasm32)`) module declarations:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod work_snapshot;
```

(Match the existing cfg-gating style used for `handler`/`plugin` in that file.)

- [ ] **Step 3: Run tests.** Run: `cargo test -p vmux_layout work_snapshot`. Expected: both pass.

- [ ] **Step 4: Commit.**

```bash
git add crates/vmux_layout/src/command_bar/work_snapshot.rs crates/vmux_layout/src/command_bar.rs
git commit -m "feat(layout): work-dir + recent-file snapshot updaters"
```

---

## Task 7: Thread the work snapshot into the payload (vmux_layout)

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/handler.rs`
- Modify: `crates/vmux_layout/src/start/plugin.rs`

- [ ] **Step 1: Extend `command_bar_open_payload`.** In `handler.rs` (lines 922-944) add two params and set the fields (the codebase already tolerates these wide payload builders; keep the style — no `#[allow]` unless clippy flags it in Task 11):

```rust
fn command_bar_open_payload(
    open_id: u64,
    native_windowed: bool,
    space_name: String,
    url: String,
    spaces: Vec<CommandBarSpace>,
    tabs: Vec<CommandBarTab>,
    commands: Vec<CommandBarCommandEntry>,
    target: Option<vmux_command::open_target::OpenTarget>,
    pages: Vec<CommandBarPage>,
    work_dirs: Vec<vmux_command::event::CommandBarWorkDir>,
    recent_files: Vec<vmux_command::event::CommandBarRecentFile>,
) -> CommandBarOpenEvent {
    CommandBarOpenEvent {
        open_id,
        native_windowed,
        url,
        space_name,
        spaces,
        tabs,
        commands,
        pages,
        work_dirs,
        recent_files,
        target,
    }
}
```

- [ ] **Step 2: Extend `build_command_bar_open_payload`.** Add a `work_snapshot` param (after `pages_snapshot`) and pass the cloned vecs through. Change the signature (lines 1020-1031) to add:

```rust
    pages_snapshot: &CommandBarPagesSnapshot,
    work_snapshot: &vmux_command::snapshot::CommandBarWorkSnapshot,
```

and change the final `command_bar_open_payload(...)` call (lines 1074-1084) to append:

```rust
    command_bar_open_payload(
        open_id,
        native_windowed,
        space_name,
        url,
        spaces,
        tabs,
        commands,
        target,
        pages,
        work_snapshot.work_dirs.clone(),
        work_snapshot.recent_files.clone(),
    )
```

- [ ] **Step 3: Pass it from the modal caller.** `handle_open_command_bar` already has 16 system params (Bevy's tuple max), so the work snapshot must go **inside** the existing `snapshot_params` `ParamSet` (a `ParamSet` counts as one param and supports up to 8 members; it currently has 6). Add it as `p6` (lines 557-564):

```rust
    mut snapshot_params: ParamSet<(
        Res<CommandBarAgentsSnapshot>,
        Res<CommandBarSpacesSnapshot>,
        ResMut<NewStackContext>,
        Option<Res<crate::settings::EffectiveStartupUrl>>,
        MessageWriter<PageOpenRequest>,
        Res<CommandBarPagesSnapshot>,
        Res<vmux_command::snapshot::CommandBarWorkSnapshot>,
    )>,
```

Clone it alongside the other snapshot clones (near line 572, with `let pages_snap = snapshot_params.p5().clone();`):

```rust
    let work_snap = snapshot_params.p6().clone();
```

and update the `build_command_bar_open_payload(...)` call (lines 868-879) to pass `&work_snap` right after `&pages_snap`:

```rust
    let payload = build_command_bar_open_payload(
        open_id,
        native_windowed,
        space_name,
        current_url,
        &spaces_snapshot,
        &agents_snap,
        &pages_snap,
        &work_snap,
        active_stack_count,
        bar_tabs,
        target,
    );
```

- [ ] **Step 4: Register the updaters in `CommandBarInputPlugin`.** In `handler.rs` add the import at top:

```rust
use crate::command_bar::work_snapshot::{update_recent_files_snapshot, update_work_dirs_snapshot};
use vmux_command::snapshot::WriteCommandBarSnapshots;
```

and in `impl Plugin for CommandBarInputPlugin::build` add a systems registration in the builder chain:

```rust
            .add_systems(
                Update,
                (update_work_dirs_snapshot, update_recent_files_snapshot)
                    .in_set(WriteCommandBarSnapshots),
            )
```

- [ ] **Step 5: Pass it from the start caller.** In `crates/vmux_layout/src/start/plugin.rs`:
  - Add `use vmux_command::snapshot::CommandBarWorkSnapshot;` to the existing snapshot import (lines 8-10).
  - Add `work_snapshot: &CommandBarWorkSnapshot` param to `build_start_payload` (line 216-221) and pass `work_snapshot` into `build_command_bar_open_payload` after `pages_snapshot` (line 235-246):

```rust
    build_command_bar_open_payload(
        0,
        false,
        space_name,
        String::new(),
        spaces_snapshot,
        agents_snapshot,
        pages_snapshot,
        work_snapshot,
        active_stack_count,
        tabs,
        Some(OpenTarget::InPlace),
    )
```

  - Add `work_snapshot: Res<CommandBarWorkSnapshot>,` to both `on_start_data_request` (lines 189-197) and `on_start_spare_revealed` (lines 159-166), and pass `&work_snapshot` to `build_start_payload(...)` in each (lines 168-173 and 202-207).

- [ ] **Step 6: Update existing payload tests.** In `handler.rs` `tests`:
  - `build_payload_includes_commands_and_target` (line 1831): add `let work = CommandBarWorkSnapshot::default();` and pass `&work` after `&pages` in the call. Add `use vmux_command::snapshot::CommandBarWorkSnapshot;` to the test module or fully-qualify.
  - `command_bar_payload_includes_space_name` (line 2181) and `command_bar_payload_includes_spaces` (line 2199): append `Vec::new(), Vec::new(),` to each `command_bar_open_payload(...)` call (the two new params).

- [ ] **Step 7: Run tests.** Run: `cargo test -p vmux_layout command_bar`. Expected: pass. Also `cargo test -p vmux_layout --lib` for start plugin tests.

- [ ] **Step 8: Commit.**

```bash
git add crates/vmux_layout/src/command_bar/handler.rs crates/vmux_layout/src/start/plugin.rs
git commit -m "feat(layout): thread work snapshot into command-bar + start payload"
```

---

## Task 8: Result variants + ordering (vmux_layout results.rs)

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/results.rs`

- [ ] **Step 1: Write failing ordering test.** Add to the `tests` module. This asserts work entries land directly after pages on an empty query:

```rust
    fn sample_work_dirs() -> Vec<vmux_command::event::CommandBarWorkDir> {
        vec![vmux_command::event::CommandBarWorkDir {
            path: "/work/proj".into(),
            kind_label: "Terminal".into(),
        }]
    }

    fn sample_recent_files() -> Vec<vmux_command::event::CommandBarRecentFile> {
        vec![vmux_command::event::CommandBarRecentFile {
            url: "file:///work/proj/main.rs".into(),
            title: "main.rs".into(),
        }]
    }

    #[test]
    fn empty_query_puts_work_after_pages() {
        let results = filter_results(
            "",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &sample_work_dirs(),
            &sample_recent_files(),
        );
        let last_page = results
            .iter()
            .rposition(|r| matches!(r, CommandBarResultItem::Page { .. }))
            .expect("pages present");
        let first_work = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::WorkDir { .. }))
            .expect("work dir present");
        let first_recent = results
            .iter()
            .position(|r| matches!(r, CommandBarResultItem::RecentFile { .. }))
            .expect("recent file present");
        assert!(last_page < first_work, "work dirs come after pages");
        assert!(first_work < first_recent, "dirs before recent files");
    }

    #[test]
    fn work_dir_matched_by_query() {
        let results = filter_results(
            "proj",
            &[],
            &[],
            &[],
            &sample_pages(),
            false,
            &[],
            &sample_work_dirs(),
            &sample_recent_files(),
        );
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::WorkDir { path, .. } if path == "/work/proj"
        )));
        assert!(results.iter().any(|r| matches!(
            r, CommandBarResultItem::RecentFile { title, .. } if title == "main.rs"
        )));
    }
```

- [ ] **Step 2: Run — fails to compile** (variants + params missing). Run: `cargo test -p vmux_layout empty_query_puts_work_after_pages`. Expected: compile error.

- [ ] **Step 3: Add variants.** In `CommandBarResultItem` (lines 6-50) add:

```rust
    WorkDir {
        path: String,
        kind_label: String,
    },
    RecentFile {
        url: String,
        title: String,
    },
```

- [ ] **Step 4: Add the import + helpers.** Update the top import (line 1-3) to include the new wire types:

```rust
use vmux_command::event::{
    CommandBarCommandEntry, CommandBarPage, CommandBarRecentFile, CommandBarSpace, CommandBarTab,
    CommandBarWorkDir, HistoryEntry,
};
```

Add helper fns (near `page_results`):

```rust
fn work_dir_results(dirs: &[CommandBarWorkDir], search_lower: &str) -> Vec<CommandBarResultItem> {
    dirs.iter()
        .filter(|d| {
            search_lower.is_empty()
                || d.path.to_lowercase().contains(search_lower)
                || d.kind_label.to_lowercase().contains(search_lower)
        })
        .map(|d| CommandBarResultItem::WorkDir {
            path: d.path.clone(),
            kind_label: d.kind_label.clone(),
        })
        .collect()
}

fn recent_file_results(
    files: &[CommandBarRecentFile],
    search_lower: &str,
) -> Vec<CommandBarResultItem> {
    files
        .iter()
        .filter(|f| {
            search_lower.is_empty()
                || f.title.to_lowercase().contains(search_lower)
                || f.url.to_lowercase().contains(search_lower)
        })
        .map(|f| CommandBarResultItem::RecentFile {
            url: f.url.clone(),
            title: f.title.clone(),
        })
        .collect()
}
```

- [ ] **Step 5: Extend `filter_results`.** Change the signature (lines 148-156) to add two params at the end:

```rust
pub fn filter_results(
    query: &str,
    tabs: &[CommandBarTab],
    commands: &[CommandBarCommandEntry],
    spaces: &[CommandBarSpace],
    pages: &[CommandBarPage],
    new_tab: bool,
    history: &[HistoryEntry],
    work_dirs: &[CommandBarWorkDir],
    recent_files: &[CommandBarRecentFile],
) -> Vec<CommandBarResultItem> {
```

In the empty-query branch, after `items.extend(page_results(pages, ""))` (line 181), insert:

```rust
        items.extend(work_dir_results(work_dirs, ""));
        items.extend(recent_file_results(recent_files, ""));
```

In the query branch, inside `if !starts_with_cmd && !is_path {` (lines 221-224), after `items.extend(space_list_items(spaces, &search_lower));`, insert:

```rust
        items.extend(work_dir_results(work_dirs, &search_lower));
        items.extend(recent_file_results(recent_files, &search_lower));
```

- [ ] **Step 6: Fix existing test call sites.** Every existing `filter_results(...)` call in the `tests` module currently ends with `&[]` (the `history` arg). Append `, &[], &[]` to each of those calls (the `spaces_url_lists_all_spaces`, `spaces_url_includes_normal_commands`, `spaces_query_includes_spaces_page_and_command`, `space_names_are_searchable`, `page_matched_by_keyword`, `agent_page_matched_by_vmux_prefix_carries_favicon`, `agent_page_matched_by_name`, `settings_page_reachable_by_name`, `empty_query_lists_all_pages_before_commands`, `pages_listed_alphabetically_by_url`, `page_carries_shortcut`, `command_prefix_excludes_pages` tests).

- [ ] **Step 7: Run tests.** Run: `cargo test -p vmux_layout --lib command_bar::results`. Expected: all pass.

- [ ] **Step 8: Commit.**

```bash
git add crates/vmux_layout/src/command_bar/results.rs
git commit -m "feat(layout): work-dir + recent-file result variants and ordering"
```

---

## Task 9: Render + dispatch in the palette (vmux_layout palette.rs)

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/palette.rs`

- [ ] **Step 1: Extract the new fields + pass to `filter_results`.** After the existing `let pages = state_val.pages.clone();` (line 150) add:

```rust
    let work_dirs = state_val.work_dirs.clone();
    let recent_files = state_val.recent_files.clone();
```

Change the `filter_results` call (line 157) to:

```rust
        let r = filter_results(
            &q, &tabs, &commands, &spaces, &pages, is_new_tab, &history, &work_dirs,
            &recent_files,
        );
```

- [ ] **Step 2: Add `execute` arms.** In the `execute` closure `match item` (lines 253-286), add before the closing brace:

```rust
            ResultItem::WorkDir { path, .. } => {
                emit_action("focus_dir", path);
            }
            ResultItem::RecentFile { url, .. } => {
                emit_action_with_target("open", url, open_target);
            }
```

- [ ] **Step 3: Add `display_text` arms.** In the `match &active_item` for `display_text` (lines 196-213) add:

```rust
            Some(ResultItem::WorkDir { path, .. }) => path.clone(),
            Some(ResultItem::RecentFile { title, url }) => {
                if title.is_empty() { url.clone() } else { title.clone() }
            }
```

- [ ] **Step 4: Add nav-icon arms.** In the `match &active_item` tuple block (lines 313-326) add (both render with the path/file icon):

```rust
            Some(ResultItem::WorkDir { .. }) => (false, true, false),
            Some(ResultItem::RecentFile { .. }) => (false, true, false),
```

- [ ] **Step 5: Add render arms.** In the results `match item` (lines 460-598) add two arms:

```rust
                            ResultItem::WorkDir { path, kind_label } => {
                                let name = path
                                    .trim_end_matches('/')
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or(path.as_str())
                                    .to_string();
                                rsx! {
                                    div { class: result_content_row_class(),
                                        Icon { class: result_leading_icon_class(),
                                            path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            span { class: result_secondary_text_class(), "{path}" }
                                        }
                                    }
                                    span { class: result_trailing_slot_class(), "{kind_label}" }
                                }
                            }
                            ResultItem::RecentFile { url, title } => {
                                let display = url.strip_prefix("file://").unwrap_or(url.as_str()).to_string();
                                let name = if title.is_empty() {
                                    display
                                        .trim_end_matches('/')
                                        .rsplit('/')
                                        .next()
                                        .unwrap_or(display.as_str())
                                        .to_string()
                                } else {
                                    title.clone()
                                };
                                rsx! {
                                    div { class: result_content_row_class(),
                                        Icon { class: result_leading_icon_class(),
                                            path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                            path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            span { class: result_secondary_text_class(), "{display}" }
                                        }
                                    }
                                    span { class: result_trailing_slot_class(), "\u{21b5}" }
                                }
                            }
```

- [ ] **Step 6: Typecheck WASM.** Run: `cargo check -p vmux_layout --target wasm32-unknown-unknown`. Expected: compiles (exhaustive matches satisfied).

- [ ] **Step 7: Run native lib tests** (source-scrape tests live here). Run: `cargo test -p vmux_layout`. Expected: pass.

- [ ] **Step 8: Commit.**

```bash
git add crates/vmux_layout/src/command_bar/palette.rs
git commit -m "feat(layout): render + dispatch work-dir and recent-file entries"
```

---

## Task 10: `focus_dir` action (vmux_layout handler.rs)

**Files:**
- Modify: `crates/vmux_layout/src/command_bar/handler.rs`

- [ ] **Step 1: Write a failing helper test.** Add a small pure helper + test so the cwd→pane selection is unit-tested (the observer itself is integration-heavy). Add near `parse_pid_from_url`:

```rust
/// Pick the entity whose recorded cwd matches `dir`, preferring the most-recently
/// active. Used by the `focus_dir` action to focus an open pane for a work dir.
pub(crate) fn pick_terminal_for_cwd(
    candidates: &[(Entity, String, i64)],
    dir: &str,
) -> Option<Entity> {
    candidates
        .iter()
        .filter(|(_, cwd, _)| cwd == dir)
        .max_by_key(|(_, _, ts)| *ts)
        .map(|(e, _, _)| *e)
}
```

Test (in the `tests` module):

```rust
    #[test]
    fn pick_terminal_prefers_most_recent_for_cwd() {
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        let c = Entity::from_bits(3);
        let cands = vec![
            (a, "/work".to_string(), 10),
            (b, "/work".to_string(), 30),
            (c, "/other".to_string(), 99),
        ];
        assert_eq!(pick_terminal_for_cwd(&cands, "/work"), Some(b));
        assert_eq!(pick_terminal_for_cwd(&cands, "/missing"), None);
    }
```

- [ ] **Step 2: Run — fails to compile.** Run: `cargo test -p vmux_layout pick_terminal_prefers_most_recent_for_cwd`. Expected: compile error.

- [ ] **Step 3: Add the query + action branch.** In `on_command_bar_action`, add a read-only param (after `user_q`):

```rust
    terminals_cwd: Query<
        (Entity, &vmux_core::terminal::TerminalLaunch, Option<&LastActivatedAt>),
        With<Terminal>,
    >,
```

Add a match arm in `match evt.action.as_str()` (after the `"terminal"` arm, before `"command"`):

```rust
        "focus_dir" => {
            let candidates: Vec<(Entity, String, i64)> = terminals_cwd
                .iter()
                .map(|(e, l, ts)| (e, l.cwd.clone(), ts.map(|t| t.0).unwrap_or(0)))
                .collect();
            if let Some(entity) = pick_terminal_for_cwd(&candidates, &evt.value) {
                focus_pane_entity(entity, &mut commands, &queries.child_of_q);
                new_stack_ctx.stack = None;
                new_stack_ctx.previous_stack = None;
                custom_keyboard_restore = true;
                if let Some(stack_e) = empty_stack {
                    commands.entity(stack_e).despawn();
                }
            } else {
                let cwd = std::path::PathBuf::from(&evt.value);
                if let Some(stack_e) = empty_stack {
                    commands.entity(stack_e).insert(PageMetadata {
                        url: terminal_page_url.clone(),
                        title: format!("Terminal ({})", cwd.display()),
                        ..default()
                    });
                    writer_params.p3().write(TerminalSpawnRequest {
                        cwd: Some(cwd),
                        target_stack: Some(stack_e),
                    });
                    new_stack_ctx.stack = None;
                    new_stack_ctx.previous_stack = None;
                    custom_keyboard_restore = true;
                } else {
                    let (_, active_pane_opt, _) = focused_stack(
                        queries.active_tab_param.get(),
                        &queries.all_children,
                        &queries.leaf_panes,
                        &queries.pane_ts,
                        &queries.pane_children,
                        &queries.stack_ts,
                    );
                    if let Some(pane_e) = active_pane_opt {
                        let stack_e = commands
                            .spawn((
                                crate::stack::stack_bundle(),
                                LastActivatedAt::now(),
                                ChildOf(pane_e),
                            ))
                            .id();
                        commands.entity(stack_e).insert(PageMetadata {
                            url: terminal_page_url.clone(),
                            title: format!("Terminal ({})", cwd.display()),
                            ..default()
                        });
                        writer_params.p3().write(TerminalSpawnRequest {
                            cwd: Some(cwd),
                            target_stack: Some(stack_e),
                        });
                        custom_keyboard_restore = true;
                    }
                }
            }
        }
```

Note: `LastActivatedAt` and `Terminal` are already imported at the top of `handler.rs` (lines 37, 39). `TerminalLaunch` is referenced fully-qualified so no new import is required.

- [ ] **Step 4: Run tests.** Run: `cargo test -p vmux_layout`. Expected: pass.

- [ ] **Step 5: Commit.**

```bash
git add crates/vmux_layout/src/command_bar/handler.rs
git commit -m "feat(layout): focus_dir action focuses or spawns a terminal for a work dir"
```

---

## Task 11: Final verification

- [ ] **Step 1: Format.** Run: `cargo fmt`. Then `git checkout -- patches/` (fmt reformats vendored patches; only commit `crates/` changes).

- [ ] **Step 2: Clippy on touched crates.** Run:
```bash
cargo clippy -p vmux_core -p vmux_history -p vmux_editor -p vmux_command -p vmux_layout -p vmux_desktop --all-targets
```
Expected: no warnings. Fix any before proceeding.

- [ ] **Step 3: Workspace tests.** Run: `cargo test --workspace`. Expected: green. (If `vmux_git` tests dirty the branch via a GIT_DIR leak, verify commit authors afterward per project notes.)

- [ ] **Step 4: WASM typecheck.** Run: `cargo check -p vmux_layout --target wasm32-unknown-unknown`. Expected: compiles.

- [ ] **Step 5: Commit any fmt-only changes.**
```bash
git add crates
git commit -m "style: cargo fmt"
```

- [ ] **Step 6: Manual runtime test (single pass).** Warm-build then `make dev` (user runs it). Verify:
  1. Open a couple of terminals in different dirs; open a file in the editor. Press Cmd+K → after the vmux:// page entries there is a work-dir group (deduped, kind badge) then a recent-files group (open file shown). On `vmux://start` the same groups appear on the default (empty) view.
  2. Selecting a work dir focuses the existing terminal pane in that dir.
  3. Selecting a recent file reopens it in the editor.
  4. Typing part of a dir/filename filters both groups.
  5. Restart the app → the opened file still appears under recent files (persisted), and shows in `vmux://history/`.

- [ ] **Step 7: Delete this plan** once fully implemented and verified: `git rm docs/plans/2026-07-01-command-bar-current-work.md`.

---

## Notes / known limitations

- "Current" dir = launch dir (no OSC 7 live-cwd tracking) — by design.
- Only initial file opens (`handle_file_page_open`) are recorded; in-editor navigations (goto-def, dir browsing) are not recorded as separate visits. Acceptable for MVP.
- File opens now also appear in `vmux://history/` and history search (intended — they are navigations). Minor: a typed query could surface the same file in both the recent-files group and the history group; not deduped.
- `recent_files` path shown in the row is the url with `file://` stripped (may contain `%20` for space-containing paths); the filename title is always clean.
