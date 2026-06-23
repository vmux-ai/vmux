# Reopen Closed Page + Archive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **NOTE (this repo):** CEF builds are heavy. Implement directly in this worktree with its own warm `target/` — do NOT set a shared `CARGO_TARGET_DIR`. All paths below are relative to the worktree root `.worktrees/reopen-closed-page/`.

**Goal:** `cmd+shift+t` reopens the most-recently-closed page (web/terminal/agent) from a persisted, bounded archive.

**Architecture:** Closing a stack emits a `PageArchiveRequest`; an isolated archive plugin stores each as a standalone `ArchivedPage` entity (persisted via moonshine-save), enforces a 25-entry cap + 30-day purge, and on `StackCommand::Reopen` pops the newest entry, builds a new tab in its origin space, and reconstructs the page through the live open path (`PageOpenRequest` / `SpawnAgentInStackRequest`).

**Tech Stack:** Rust, Bevy ECS (message + system), moonshine-save (RON persistence), CEF.

---

## File Structure

- **Create** `crates/vmux_core/src/archive.rs` — `ArchivedPage` component + `PageArchiveRequest` message.
- **Modify** `crates/vmux_core/src/lib.rs` — module decl, re-exports, register `ArchivedPage` in `CorePlugin`.
- **Create** `crates/vmux_layout/src/archive.rs` — `ArchivePlugin`: capture, maintain (cap+purge), reopen systems + tests.
- **Modify** `crates/vmux_layout/src/lib.rs` — `pub mod archive;`.
- **Modify** `crates/vmux_layout/src/plugin.rs` — add `ArchivePlugin` to `LayoutPlugin`.
- **Modify** `crates/vmux_layout/src/window.rs` — extract `spawn_tab_scaffold_in_space` helper; reuse in `spawn_requested_tab_layouts`.
- **Modify** `crates/vmux_layout/src/stack.rs` — emit `PageArchiveRequest` in the `StackCommand::Close` arm (+ system params).
- **Modify** `crates/vmux_desktop/src/persistence.rs` — allowlist `ArchivedPage`; track it in `mark_dirty_on_change`.
- **Modify** `crates/vmux_command/src/command.rs` — relabel/unhide `Reopen`, add Linux shortcut.

---

## Task 1: `ArchivedPage` component + `PageArchiveRequest` message (`vmux_core`)

**Files:**
- Create: `crates/vmux_core/src/archive.rs`
- Modify: `crates/vmux_core/src/lib.rs` (module decl at top with other `#[cfg(not(target_arch = "wasm32"))] pub mod ...`; re-export block at lines 19-22; `CorePlugin::build` register chain at lines 35-47)
- Test: `crates/vmux_core/src/archive.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write `crates/vmux_core/src/archive.rs`**

```rust
use bevy::prelude::*;
use moonshine_save::prelude::*;

use crate::terminal::TerminalLaunch;

#[derive(Component, Clone, Debug, Reflect, Default)]
#[reflect(Component, Default)]
#[require(Save)]
#[type_path = "vmux_core::archive"]
pub struct ArchivedPage {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub closed_at: i64,
    pub launch: Option<TerminalLaunch>,
}

#[derive(Message, Clone, Debug)]
pub struct PageArchiveRequest {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub launch: Option<TerminalLaunch>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archived_page_defaults_are_empty() {
        let a = ArchivedPage::default();
        assert!(a.url.is_empty());
        assert!(a.launch.is_none());
        assert_eq!(a.closed_at, 0);
    }

    #[test]
    fn archived_page_is_registered_by_core_plugin() {
        let mut app = App::new();
        app.add_plugins(crate::CorePlugin);
        let registry = app.world().resource::<AppTypeRegistry>().read();
        assert!(
            registry
                .get(std::any::TypeId::of::<ArchivedPage>())
                .is_some()
        );
    }
}
```

- [ ] **Step 2: Wire module + re-export + registration into `crates/vmux_core/src/lib.rs`**

Add the module decl alongside the other gated modules (after line 15 `pub mod team;` block):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod archive;
```

Add to the re-export block (after the `page_open::{...}` use at lines 19-22):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub use archive::{ArchivedPage, PageArchiveRequest};
```

Add `ArchivedPage` to the `CorePlugin::build` register chain (insert before `.register_type::<Active>()` at line 45):

```rust
            .register_type::<ArchivedPage>()
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p vmux_core archive`
Expected: PASS (`archived_page_defaults_are_empty`, `archived_page_is_registered_by_core_plugin`).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_core/src/archive.rs crates/vmux_core/src/lib.rs
git commit -m "feat(core): ArchivedPage component + PageArchiveRequest message"
```

---

## Task 2: Archive capture + cap + purge (`vmux_layout/src/archive.rs`)

This task adds the `ArchivePlugin` with the capture system and the `maintain_archive` system (enforces the 25-entry cap and 30-day purge). Reopen comes in Task 4.

**Files:**
- Create: `crates/vmux_layout/src/archive.rs`
- Modify: `crates/vmux_layout/src/lib.rs` (module decls, ~lines 43-60 block of gated `pub mod`)
- Modify: `crates/vmux_layout/src/plugin.rs` (add `ArchivePlugin` to the `.add_plugins((...))` tuple at lines 71-82)
- Test: `crates/vmux_layout/src/archive.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write `crates/vmux_layout/src/archive.rs` (capture + maintain only)**

```rust
use bevy::prelude::*;
use vmux_core::{ArchivedPage, PageArchiveRequest, now_millis};

const MAX_ARCHIVE_ENTRIES: usize = 25;
const ARCHIVE_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub struct ArchivePlugin;

impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, (capture_archived_pages, maintain_archive));
    }
}

fn capture_archived_pages(
    mut reader: MessageReader<PageArchiveRequest>,
    mut commands: Commands,
) {
    for req in reader.read() {
        if req.url.is_empty() {
            continue;
        }
        commands.spawn(ArchivedPage {
            url: req.url.clone(),
            title: req.title.clone(),
            space_id: req.space_id.clone(),
            closed_at: now_millis(),
            launch: req.launch.clone(),
        });
    }
}

fn maintain_archive(archived: Query<(Entity, &ArchivedPage)>, mut commands: Commands) {
    let now = now_millis();
    let mut live: Vec<(Entity, i64)> = Vec::new();
    for (entity, page) in &archived {
        if now - page.closed_at > ARCHIVE_TTL_MS {
            commands.entity(entity).despawn();
        } else {
            live.push((entity, page.closed_at));
        }
    }
    if live.len() > MAX_ARCHIVE_ENTRIES {
        live.sort_by_key(|(_, closed_at)| *closed_at);
        let overflow = live.len() - MAX_ARCHIVE_ENTRIES;
        for (entity, _) in live.into_iter().take(overflow) {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(url: &str, closed_at: i64) -> ArchivedPage {
        ArchivedPage {
            url: url.to_string(),
            title: String::new(),
            space_id: "s".to_string(),
            closed_at,
            launch: None,
        }
    }

    #[test]
    fn capture_spawns_archived_page() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: "https://a.example".to_string(),
                title: "A".to_string(),
                space_id: "s".to_string(),
                launch: None,
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let all: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://a.example");
    }

    #[test]
    fn capture_skips_empty_url() {
        let mut app = App::new();
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, capture_archived_pages);
        app.world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .write(PageArchiveRequest {
                url: String::new(),
                title: String::new(),
                space_id: "s".to_string(),
                launch: None,
            });
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }

    #[test]
    fn maintain_enforces_cap_dropping_oldest() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        for i in 0..(MAX_ARCHIVE_ENTRIES as i64 + 1) {
            app.world_mut().spawn(page(&format!("u{i}"), now - i));
        }
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls.len(), MAX_ARCHIVE_ENTRIES);
        let oldest = format!("u{}", MAX_ARCHIVE_ENTRIES);
        assert!(!urls.contains(&oldest));
    }

    #[test]
    fn maintain_purges_expired() {
        let mut app = App::new();
        app.add_systems(Update, maintain_archive);
        let now = now_millis();
        app.world_mut().spawn(page("fresh", now));
        app.world_mut()
            .spawn(page("stale", now - ARCHIVE_TTL_MS - 1));
        app.update();
        let mut q = app.world_mut().query::<&ArchivedPage>();
        let urls: Vec<String> = q.iter(app.world()).map(|p| p.url.clone()).collect();
        assert_eq!(urls, vec!["fresh".to_string()]);
    }
}
```

- [ ] **Step 2: Add module decl to `crates/vmux_layout/src/lib.rs`**

Add with the other non-wasm modules (after line 44 `pub mod active;`):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub mod archive;
```

- [ ] **Step 3: Register `ArchivePlugin` in `crates/vmux_layout/src/plugin.rs`**

Add the import near the other plugin imports (after line 5 `use crate::command_bar::handler::CommandBarInputPlugin;`):

```rust
use crate::archive::ArchivePlugin;
```

Add `ArchivePlugin` to the second `.add_plugins((...))` tuple at line 83:

```rust
            .add_plugins((CommandBarInputPlugin, TogglePlugin, WebviewRevealPlugin, ArchivePlugin));
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_layout archive::tests`
Expected: PASS (4 tests: capture spawn, skip empty, cap, purge).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core crates/vmux_layout/src/archive.rs crates/vmux_layout/src/lib.rs crates/vmux_layout/src/plugin.rs
git commit -m "feat(layout): archive plugin — capture closed pages, cap 25, purge 30d"
```

---

## Task 3: Extract `spawn_tab_scaffold_in_space` helper (`window.rs`)

Reopen needs to create a tab in an arbitrary space. The tab/split/leaf/stack scaffold currently lives inline in `spawn_requested_tab_layouts` (`window.rs:490-539`). Extract it so reopen and the existing path share one source of truth.

**Files:**
- Modify: `crates/vmux_layout/src/window.rs` (extract helper; refactor `spawn_requested_tab_layouts` lines 490-539)
- Test: `crates/vmux_layout/src/window.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Add the helper + struct in `crates/vmux_layout/src/window.rs`**

Add immediately above `pub fn spawn_requested_tab_layouts` (line 469). It uses symbols already imported by `window.rs` (`tab_bundle`, `leaf_pane_bundle`, `stack_bundle`, `Pane`, `PaneSplit`, `PaneSplitDirection`, `pane_split_gaps`, `HostWindow`, `LastActivatedAt`, `CreatedAt`):

```rust
pub struct TabScaffold {
    pub tab: Entity,
    pub pane: Entity,
    pub stack: Entity,
}

pub fn spawn_tab_scaffold_in_space(
    commands: &mut Commands,
    space: Entity,
    primary_window: Entity,
    gap_px: f32,
) -> TabScaffold {
    let tab = commands
        .spawn((
            tab_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(space),
        ))
        .id();

    let gap = pane_split_gaps(PaneSplitDirection::Row, gap_px);
    let split_root = commands
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            HostWindow(primary_window),
            ZIndex(0),
            Transform::default(),
            GlobalTransform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: gap.column_gap,
                row_gap: gap.row_gap,
                ..default()
            },
            ChildOf(tab),
        ))
        .id();

    let pane = commands
        .spawn((leaf_pane_bundle(), LastActivatedAt::now(), ChildOf(split_root)))
        .id();

    let stack = commands
        .spawn((
            stack_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(pane),
        ))
        .id();

    TabScaffold { tab, pane, stack }
}
```

- [ ] **Step 2: Refactor `spawn_requested_tab_layouts` to use the helper**

Replace the inline spawn block (current lines 490-539, from `let tab_e = commands` through the `let stack = commands ... .id();`) with:

```rust
        let TabScaffold {
            tab: tab_e,
            pane: leaf,
            stack,
        } = spawn_tab_scaffold_in_space(
            &mut commands,
            parent,
            request.primary_window,
            settings.pane.gap,
        );
        if let Some(name) = request.name.clone() {
            commands.entity(tab_e).insert(Tab { name });
        }
```

Leave everything after (the `clear_pending_stack` block, `new_stack_ctx` updates, the `match &request.content`, and the `request.focus` block) unchanged.

- [ ] **Step 3: Add a test for the helper**

Add to the `#[cfg(test)] mod tests` in `window.rs` (if no test module exists, create one at end of file with `use super::*;`):

```rust
    #[test]
    fn scaffold_builds_tab_pane_stack_under_space() {
        use bevy::ecs::system::SystemState;
        let mut app = App::new();
        let space = app.world_mut().spawn(crate::space::Space).id();
        let window = app.world_mut().spawn_empty().id();
        let result = {
            let world = app.world_mut();
            let mut state = SystemState::<Commands>::new(world);
            let mut commands = state.get_mut(world);
            let r = spawn_tab_scaffold_in_space(&mut commands, space, window, 8.0);
            state.apply(world);
            r
        };
        assert!(app.world().get::<crate::tab::Tab>(result.tab).is_some());
        assert!(app.world().get::<crate::pane::Pane>(result.pane).is_some());
        assert!(app.world().get::<crate::stack::Stack>(result.stack).is_some());
        assert_eq!(app.world().get::<ChildOf>(result.tab).unwrap().get(), space);
    }
```

Note: `spawn_tab_scaffold_in_space`, `App`, `Commands`, `ChildOf` come in via `use super::*`. The `window` arg is only stored in `HostWindow`, so a bare `spawn_empty()` entity is fine.

- [ ] **Step 4: Run tests to verify the refactor + helper**

Run: `cargo test -p vmux_layout window::`
Expected: PASS, including existing `window` tests (no regressions) and `scaffold_builds_tab_pane_stack_under_space`.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/window.rs
git commit -m "refactor(layout): extract spawn_tab_scaffold_in_space helper"
```

---

## Task 4: Reopen handler (`vmux_layout/src/archive.rs`)

Adds `handle_reopen_closed_page`: on `StackCommand::Reopen`, pop the newest `ArchivedPage`, build a tab in its origin space (fallback active space), reconstruct via the live open path, and consume the entry.

**Files:**
- Modify: `crates/vmux_layout/src/archive.rs` (add system + register in `ArchivePlugin`)
- Test: `crates/vmux_layout/src/archive.rs`

- [ ] **Step 1: Add imports + the reopen system to `archive.rs`**

Replace the top `use` lines of `archive.rs` with:

```rust
use std::path::PathBuf;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use vmux_command::{AppCommand, LayoutCommand, ReadAppCommands, StackCommand};
use vmux_core::agent::{AgentKind, SpawnAgentInStackRequest};
use vmux_core::{ArchivedPage, PageArchiveRequest, PageOpenRequest, PageOpenTarget, now_millis};

use crate::event::TERMINAL_PAGE_URL;
use crate::settings::LayoutSettings;
use crate::space::{ActiveSpaceEntity, Space, SpaceId};
use crate::window::spawn_tab_scaffold_in_space;
```

Update `ArchivePlugin::build` to register the reopen system in the `ReadAppCommands` set:

```rust
impl Plugin for ArchivePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PageArchiveRequest>()
            .add_systems(Update, (capture_archived_pages, maintain_archive))
            .add_systems(Update, handle_reopen_closed_page.in_set(ReadAppCommands));
    }
}
```

Add the system (after `maintain_archive`):

```rust
fn handle_reopen_closed_page(
    mut reader: MessageReader<AppCommand>,
    archived: Query<(Entity, &ArchivedPage)>,
    spaces: Query<(Entity, &SpaceId), With<Space>>,
    any_space: Query<Entity, With<Space>>,
    active_space: Res<ActiveSpaceEntity>,
    settings: Res<LayoutSettings>,
    primary_window: Single<Entity, With<PrimaryWindow>>,
    mut page_open: MessageWriter<PageOpenRequest>,
    mut spawn_agent: MessageWriter<SpawnAgentInStackRequest>,
    mut commands: Commands,
) {
    let mut reopen = false;
    for cmd in reader.read() {
        if matches!(
            cmd,
            AppCommand::Layout(LayoutCommand::Stack(StackCommand::Reopen))
        ) {
            reopen = true;
        }
    }
    if !reopen {
        return;
    }

    let Some((entry_entity, page)) = archived
        .iter()
        .max_by_key(|(_, p)| p.closed_at)
        .map(|(e, p)| (e, p.clone()))
    else {
        return;
    };

    let target_space = spaces
        .iter()
        .find(|(_, id)| id.0 == page.space_id)
        .map(|(e, _)| e)
        .or(active_space.0)
        .or_else(|| any_space.iter().next());
    let Some(space) = target_space else {
        return;
    };

    let scaffold =
        spawn_tab_scaffold_in_space(&mut commands, space, *primary_window, settings.pane.gap);
    commands.entity(scaffold.stack).insert(vmux_core::PageMetadata {
        url: page.url.clone(),
        title: page.title.clone(),
        ..default()
    });
    commands.entity(space).insert(vmux_history::LastActivatedAt::now());

    if let Some(kind) = AgentKind::all()
        .into_iter()
        .find(|k| page.url.starts_with(&k.cli_url_prefix()))
    {
        let cwd = page
            .launch
            .as_ref()
            .map(|l| PathBuf::from(&l.cwd))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        spawn_agent.write(SpawnAgentInStackRequest {
            kind,
            cwd,
            session_id: None,
            stack: scaffold.stack,
        });
    } else if page.url.starts_with(TERMINAL_PAGE_URL) {
        let url = match page.launch.as_ref() {
            Some(l) if !l.cwd.is_empty() => format!("{TERMINAL_PAGE_URL}?cwd={}", l.cwd),
            _ => page.url.clone(),
        };
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(scaffold.stack),
            url,
            request_id: None,
        });
    } else {
        page_open.write(PageOpenRequest {
            target: PageOpenTarget::Stack(scaffold.stack),
            url: page.url.clone(),
            request_id: None,
        });
    }

    commands.entity(entry_entity).despawn();
    let _ = now_millis;
}
```

(The trailing `let _ = now_millis;` is a no-op kept only if a clippy `unused_imports` arises; remove it if the import is already used by `capture_archived_pages` in the same file — it is, so delete that line.)

- [ ] **Step 2: Delete the no-op line**

Remove `let _ = now_millis;` from the end of `handle_reopen_closed_page` (`now_millis` is already used by `capture_archived_pages`).

- [ ] **Step 3: Add reopen tests**

Add to `archive.rs` `#[cfg(test)] mod tests`. Only `PageMetadata` and the terminal types need importing — `AppCommand`, `LayoutCommand`, `StackCommand`, `AgentKind`, `SpawnAgentInStackRequest`, `PageOpenRequest`, `PageOpenTarget`, `ArchivedPage`, and `now_millis` all arrive via the module's existing `use super::*;`:

```rust
    use vmux_core::PageMetadata;
    use vmux_core::terminal::{TerminalKind, TerminalLaunch};

    fn reopen_app() -> App {
        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageOpenRequest>()
            .add_message::<SpawnAgentInStackRequest>()
            .init_resource::<crate::space::ActiveSpaceEntity>()
            .init_resource::<crate::settings::LayoutSettings>()
            .add_systems(Update, super::handle_reopen_closed_page);
        app.world_mut().spawn((bevy::window::Window::default(), bevy::window::PrimaryWindow));
        app
    }

    fn dispatch_reopen(app: &mut App) {
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(StackCommand::Reopen)));
        app.update();
    }

    fn drain_opens(app: &mut App) -> Vec<PageOpenRequest> {
        app.world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect()
    }

    #[test]
    fn reopen_web_opens_in_origin_space_and_consumes_entry() {
        let mut app = reopen_app();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())))
            .id();
        let _ = space;
        app.world_mut().spawn(ArchivedPage {
            url: "https://a.example".to_string(),
            title: "A".to_string(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: None,
        });
        dispatch_reopen(&mut app);

        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://a.example");
        assert!(matches!(opens[0].target, PageOpenTarget::Stack(_)));
        let mut q = app.world_mut().query::<&ArchivedPage>();
        assert_eq!(q.iter(app.world()).count(), 0);
        // New stack carries the restored metadata.
        let mut metas = app.world_mut().query::<(&crate::stack::Stack, &PageMetadata)>();
        assert!(metas
            .iter(app.world())
            .any(|(_, m)| m.url == "https://a.example"));
    }

    #[test]
    fn reopen_picks_newest_first() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "https://old.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 1,
            launch: None,
        });
        app.world_mut().spawn(ArchivedPage {
            url: "https://new.example".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 2,
            launch: None,
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "https://new.example");
    }

    #[test]
    fn reopen_terminal_encodes_cwd() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: "vmux://terminal/".to_string(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "/bin/zsh".to_string(),
                args: vec![],
                cwd: "/work".to_string(),
                env: vec![],
                kind: TerminalKind::Plain,
            }),
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        assert_eq!(opens[0].url, "vmux://terminal/?cwd=/work");
    }

    #[test]
    fn reopen_agent_emits_spawn_request_fresh_session() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        app.world_mut().spawn(ArchivedPage {
            url: AgentKind::Claude.cli_url_prefix(),
            title: String::new(),
            space_id: "s1".to_string(),
            closed_at: 5,
            launch: Some(TerminalLaunch {
                command: "claude".to_string(),
                args: vec![],
                cwd: "/proj".to_string(),
                env: vec![],
                kind: TerminalKind::Claude,
            }),
        });
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
        let spawns: Vec<SpawnAgentInStackRequest> = app
            .world_mut()
            .resource_mut::<Messages<SpawnAgentInStackRequest>>()
            .drain()
            .collect();
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].kind, AgentKind::Claude);
        assert_eq!(spawns[0].cwd, PathBuf::from("/proj"));
        assert!(spawns[0].session_id.is_none());
    }

    #[test]
    fn reopen_empty_archive_is_noop() {
        let mut app = reopen_app();
        app.world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("s1".to_string())));
        dispatch_reopen(&mut app);
        assert!(drain_opens(&mut app).is_empty());
    }

    #[test]
    fn reopen_falls_back_to_active_space_when_origin_gone() {
        let mut app = reopen_app();
        let active = app
            .world_mut()
            .spawn((crate::space::Space, crate::space::SpaceId("active".to_string())))
            .id();
        app.world_mut()
            .insert_resource(crate::space::ActiveSpaceEntity(Some(active)));
        app.world_mut().spawn(ArchivedPage {
            url: "https://x.example".to_string(),
            title: String::new(),
            space_id: "ghost".to_string(),
            closed_at: 5,
            launch: None,
        });
        dispatch_reopen(&mut app);
        let opens = drain_opens(&mut app);
        assert_eq!(opens.len(), 1);
        // Tab landed under the active space.
        let mut tabs = app.world_mut().query::<(&crate::tab::Tab, &ChildOf)>();
        assert!(tabs.iter(app.world()).any(|(_, co)| co.get() == active));
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p vmux_layout archive::tests`
Expected: PASS (capture/cap/purge from Task 2 + the 6 reopen tests).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_layout/src/archive.rs
git commit -m "feat(layout): reopen closed page into origin space via archive"
```

---

## Task 5: Emit `PageArchiveRequest` on stack close (`stack.rs`)

Wire the capture: when a stack closes, archive its page before despawn.

**Files:**
- Modify: `crates/vmux_layout/src/stack.rs` (add system params; emit in `StackCommand::Close` arm at line 298, right after the `active` binding at lines 302-304)
- Test: `crates/vmux_layout/src/stack.rs`

- [ ] **Step 1: Add imports to `stack.rs`**

Add to the `use vmux_core::{...}` line (currently `use vmux_core::{PageOpenRequest, PageOpenTarget};` at line 18):

```rust
use vmux_core::{PageArchiveRequest, PageMetadata, PageOpenRequest, PageOpenTarget};
```

- [ ] **Step 2: Add system params to `handle_stack_commands`**

Add these params to the `fn handle_stack_commands(...)` signature (after `mut pending_cursor_warp: ResMut<PendingCursorWarp>,` at line 220):

```rust
    stack_pages: Query<(&PageMetadata, Option<&vmux_core::terminal::TerminalLaunch>), With<Stack>>,
    spaces_q: Query<(), With<crate::space::Space>>,
    space_ids: Query<&crate::space::SpaceId>,
    mut archive_writer: MessageWriter<PageArchiveRequest>,
```

- [ ] **Step 3: Emit in the `Close` arm**

In the `Dispatch::Stack(StackCommand::Close)` arm, immediately after the `let Some(active) = active_stack else { continue; };` block (lines 302-304), insert:

```rust
                if let Ok((meta, launch)) = stack_pages.get(active) {
                    let space_id = crate::space::space_of(active, &child_of_q, &spaces_q)
                        .and_then(|s| space_ids.get(s).ok())
                        .map(|id| id.0.clone())
                        .unwrap_or_default();
                    archive_writer.write(PageArchiveRequest {
                        url: meta.url.clone(),
                        title: meta.title.clone(),
                        space_id,
                        launch: launch.cloned(),
                    });
                }
```

- [ ] **Step 4: Write an integration test for close → archive emission**

Add to `stack.rs` `#[cfg(test)] mod tests` (create the module at end of file with `use super::*;` if absent):

```rust
    #[test]
    fn closing_a_stack_emits_archive_request() {
        // Only Space/SpaceId need importing; Pane, Tab, PendingCursorWarp,
        // AppCommand, LayoutCommand, StackCommand, PageMetadata,
        // PageArchiveRequest, NewStackContext, Stack and LastActivatedAt are all
        // already in scope via the module's `use super::*;`.
        use crate::space::{Space, SpaceId};

        let mut app = App::new();
        app.add_message::<AppCommand>()
            .add_message::<PageOpenRequest>()
            .add_message::<PageArchiveRequest>()
            .init_resource::<NewStackContext>()
            .init_resource::<PendingCursorWarp>()
            .add_systems(Update, handle_stack_commands);

        let space = app
            .world_mut()
            .spawn((Space, SpaceId("s1".to_string()), vmux_core::Active))
            .id();
        let tab = app
            .world_mut()
            .spawn((Tab::default(), vmux_core::Active, LastActivatedAt::now(), ChildOf(space)))
            .id();
        let split = app.world_mut().spawn((Pane, ChildOf(tab))).id();
        let leaf = app
            .world_mut()
            .spawn((Pane, LastActivatedAt::now(), ChildOf(split)))
            .id();
        app.world_mut().spawn((
            Stack::default(),
            PageMetadata {
                url: "https://gone.example".to_string(),
                title: "Gone".to_string(),
                ..default()
            },
            LastActivatedAt::now(),
            ChildOf(leaf),
        ));

        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Layout(LayoutCommand::Stack(StackCommand::Close)));
        app.update();

        let reqs: Vec<PageArchiveRequest> = app
            .world_mut()
            .resource_mut::<Messages<PageArchiveRequest>>()
            .drain()
            .collect();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://gone.example");
        assert_eq!(reqs[0].space_id, "s1");
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p vmux_layout stack::`
Expected: PASS, including existing stack tests + `closing_a_stack_emits_archive_request`.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_layout/src/stack.rs
git commit -m "feat(layout): archive a page's metadata when its stack closes"
```

---

## Task 6: Persist `ArchivedPage` (`persistence.rs`)

Add `ArchivedPage` to the save allowlist and make add/remove of `ArchivedPage` mark the store dirty (otherwise capture/reopen/purge only persist incidentally).

**Files:**
- Modify: `crates/vmux_desktop/src/persistence.rs` (import; allowlist at lines 171-196; `mark_dirty_on_change` at lines 119-144)
- Test: `crates/vmux_desktop/src/persistence.rs`

- [ ] **Step 1: Import + allowlist**

Add to the `use vmux_core::{...}` import (line 9, currently `use vmux_core::{CreatedAt, Order, PageMetadata};`):

```rust
use vmux_core::{ArchivedPage, CreatedAt, Order, PageMetadata};
```

Add to the `SceneFilter` allowlist in `save_space_to_path` (after `.allow::<PageMetadata>()` at line 186):

```rust
            .allow::<ArchivedPage>()
```

- [ ] **Step 2: Track `ArchivedPage` in `mark_dirty_on_change`**

Add two params to `fn mark_dirty_on_change(...)` (after `changed_geometry: Query<(), Changed<WindowGeometry>>,` at line 129):

```rust
    added_archived: Query<(), Added<ArchivedPage>>,
    mut removed_archived: RemovedComponents<ArchivedPage>,
```

Add to the boolean OR condition (extend the `if` at lines 131-140):

```rust
        || !added_archived.is_empty()
        || removed_archived.read().count() > 0
```

- [ ] **Step 3: Write the dirty-tracking test**

Add to `persistence.rs` `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn adding_archived_page_marks_store_dirty() {
        use vmux_core::ArchivedPage;
        let mut app = App::new();
        app.insert_resource(AutoSave {
            debounce: Timer::from_seconds(0.5, TimerMode::Once),
            periodic: Timer::from_seconds(60.0, TimerMode::Repeating),
            dirty: false,
        })
        .add_systems(Update, mark_dirty_on_change);
        app.update();
        app.world_mut().resource_mut::<AutoSave>().dirty = false;
        app.world_mut().spawn(ArchivedPage::default());
        app.update();
        assert!(app.world().resource::<AutoSave>().dirty);
    }
```

(If the existing `persistence.rs` test module gates on a feature or imports differently, mirror its existing harness; `AutoSave` and `mark_dirty_on_change` are in-module.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p vmux_desktop persistence`
Expected: PASS, including `adding_archived_page_marks_store_dirty`.

- [ ] **Step 5: Verify `rebuild_space_views` ignores `ArchivedPage`**

Read `crates/vmux_desktop/src/persistence.rs:307-420`. Confirm every `*_need_view` query is `With<Tab/Space/PaneSplit/Pane/Stack>` and the only despawn targets empty-url Stacks (lines 417-419). `ArchivedPage` entities have none of those markers → untouched. No code change; this is a confirmation step. If any query would match `ArchivedPage`, stop and reconsider.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_desktop/src/persistence.rs
git commit -m "feat(desktop): persist ArchivedPage + mark store dirty on archive change"
```

---

## Task 7: Shortcut + menu label (`command.rs`)

Surface the reopen command: rename to project terminology ("page"), unhide, add the Linux binding. The macOS binding already exists via `accel = "super+shift+t"`.

**Files:**
- Modify: `crates/vmux_command/src/command.rs` (the `Reopen` variant, lines 75-81)

- [ ] **Step 1: Edit the `Reopen` variant**

Replace lines 75-81:

```rust
    #[menu(
        id = "stack_reopen",
        label = "Reopen Closed Stack",
        accel = "super+shift+t",
        hidden
    )]
    Reopen,
```

with:

```rust
    #[menu(id = "stack_reopen", label = "Reopen Closed Page", accel = "super+shift+t")]
    #[shortcut(direct = "Ctrl+Shift+T")]
    Reopen,
```

- [ ] **Step 2: Run the command crate tests**

Run: `cargo test -p vmux_command`
Expected: PASS. If a test asserts a fixed shortcut/menu count or rejects duplicate bindings, update that test to include the new `Ctrl+Shift+T` direct binding for `stack_reopen`.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_command/src/command.rs
git commit -m "feat(command): surface Reopen Closed Page + add Ctrl+Shift+T (Linux)"
```

---

## Task 8: Whole-crate checks + manual runtime verification

- [ ] **Step 1: Format + clippy + build the affected crates**

Run:
```bash
cargo fmt --all
cargo clippy -p vmux_core -p vmux_layout -p vmux_command -p vmux_desktop --all-targets
```
Expected: no warnings/errors. Fix any before proceeding.

- [ ] **Step 2: Run the full test set for touched crates**

Run:
```bash
cargo test -p vmux_core -p vmux_layout -p vmux_command -p vmux_desktop
```
Expected: all PASS.

- [ ] **Step 3: Manual runtime verification (user-driven)**

Build/run the desktop app from the worktree. Verify, in order:
1. Open a web page; `cmd+w` to close it; `cmd+shift+t` → the page reopens as a new tab in the same space.
2. Press `cmd+shift+t` again → the previously-closed page (one older) reopens (LIFO walk-back).
3. Close a terminal; `cmd+shift+t` → a fresh terminal reopens at the same cwd.
4. Close an agent page; `cmd+shift+t` → a fresh agent session of the same kind opens.
5. Close a page, fully quit and relaunch, then `cmd+shift+t` → the page still reopens (archive persisted in `store.ron`).
6. Confirm the menu shows "Reopen Closed Page".

Report any failures; do not mark the feature complete until the golden paths above pass in the real app.

- [ ] **Step 4: Final commit (if fmt/clippy made changes)**

```bash
git add -A
git commit -m "chore: fmt + clippy for reopen-closed-page feature"
```

---

## Self-Review Notes

- **Spec coverage:** data model (T1), capture/cap/purge (T2), tab-in-origin-space (T3+T4), reopen all kinds (T4), close→archive (T5), persistence + no schema bump (T6), shortcut/label (T7). All spec sections mapped.
- **Spec correction:** the spec said removals "auto-persist via existing dirty-tracking" — that was inaccurate (`mark_dirty_on_change` did not watch `ArchivedPage`). T6 Step 2 fixes this by adding explicit add/remove tracking.
- **No schema bump:** intentionally omitted; bumping `STORE_SCHEMA_VERSION` (currently 2) deletes the user's store on upgrade (`persistence.rs:209`). Adding a component to the allowlist is backward-compatible.
- **Type consistency:** `ArchivedPage { url, title, space_id, closed_at, launch }` used identically across T1/T2/T4/T5/T6. `spawn_tab_scaffold_in_space` / `TabScaffold { tab, pane, stack }` used identically in T3/T4. `PageArchiveRequest { url, title, space_id, launch }` identical in T1/T2/T5.
- **Agent session:** reopen uses `session_id: None` (fresh) per the spec's non-goal (no conversation continuity).
