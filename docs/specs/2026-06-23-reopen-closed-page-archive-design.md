# Reopen Closed Page + Archive — Design

Date: 2026-06-23
Status: Approved
Branch: `feat/reopen-closed-page`

## Goal

`cmd+shift+t` (macOS) / `ctrl+shift+t` (Linux) reopens the most-recently-closed
page. Closed pages enter a persisted **archive** (a LIFO of recently closed
pages). The archive is bounded: at most 25 entries, and any entry older than 30
days is purged. Reopen **consumes** the entry (repeated presses walk further
back) and lands the page as a **new tab in its origin space** (fallback: the
active space).

This is a **backing store only** — there is no archive panel, list, or search.

## Background (current state)

- A user "page" is a `Browser` entity (web / terminal / agent) under
  `Space → Tab → Pane → Stack`. `url`/`title` live in `PageMetadata`
  (`vmux_core`), readable on the live entity at close time.
- Closing a stack = recursive `despawn` inside the `StackCommand::Close` arm
  (`vmux_layout/src/stack.rs`). The page's position in the pane tree is implicit
  and is lost on close (the collapse mutates the tree in place).
- `StackCommand::Reopen` **already exists** (`vmux_command/src/command.rs`) with
  `accel = "super+shift+t"` and is `hidden`, but its handler is a **no-op**
  (`stack.rs`, the `Reopen | Duplicate | MoveToPane => {}` arm). No closed-page
  storage exists anywhere.
- Persistence is `store.ron` (moonshine-save `DynamicScene`, component-based via
  reflection — not a serde struct). Persisted components are an allowlist in
  `vmux_desktop/src/persistence.rs` (`save_space_to_path`). A schema-version
  sidecar (`store.version`) gates loads; a version mismatch **deletes** the
  store.
- `rebuild_space_views` (`persistence.rs`) already reconstructs any kind of page
  from a saved `Stack` (`PageMetadata.url` + optional `TerminalLaunch`),
  branching by URL prefix — this is the existing restore-from-disk logic and the
  model the archive mirrors.
- All three kinds open through `PageOpenRequest` → `PageOpenTask` → per-kind
  systems dispatched by URL prefix. Agents also have a lower-level
  `SpawnAgentInStackRequest { kind, cwd, session_id, stack }` that carries an
  explicit cwd.

## Decisions (locked)

| Question | Decision |
| --- | --- |
| Archive role | Backing store only — no UI. |
| Page kinds | All: web, terminal, agent. Terminal respawns fresh at cwd; agent **resumes its session id** if one was captured, else fresh. |
| Restore target | Origin space, new tab. Fallback: active space if origin gone. |
| Bounds | Cap 25 most-recent **and** purge entries older than 30 days. |
| Capture mechanism | Approach C: message-driven archive plugin. |
| Schema version | **No bump** — bumping wipes saved layouts on upgrade. |

## Architecture

### 1. Data model (`vmux_core`)

`ArchivedPage` component:

```rust
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
#[require(Save)]
pub struct ArchivedPage {
    pub url: String,                  // kind encoded by prefix
    pub title: String,
    pub space_id: String,             // origin SpaceId
    pub closed_at: i64,               // now_millis() at close
    pub launch: Option<TerminalLaunch>, // terminal cmd/args/cwd/env; agent cwd
}
```

- Kind is derived from `url`: `http…`/`https…`/`file…` = web,
  `vmux://terminal…` = terminal, `vmux://agent…` = agent.
- One **standalone entity** per archived page — not parented into the space
  tree. It carries only `ArchivedPage` (+ `Save`), so the view rebuild never
  treats it as a live page.
- The shape mirrors a persisted `Stack` so restore reuses the same per-kind open
  path as `rebuild_space_views`.

### 2. Capture: close → archive (approach C)

New message:

```rust
#[derive(Message)]
pub struct PageArchiveRequest {
    pub url: String,
    pub title: String,
    pub space_id: String,
    pub launch: Option<TerminalLaunch>,
}
```

- **Emit site:** the `StackCommand::Close` arm (`vmux_layout/src/stack.rs`), at
  the top of the arm **before** any despawn/`continue` branch. Read the closing
  stack's child page: `PageMetadata` (url/title), `TerminalLaunch` if present,
  `AgentSession`/cwd if present; resolve `space_of(stack)` → `SpaceId`. Emit
  `PageArchiveRequest`. **Skip when url is empty** (an empty new stack has
  nothing to restore).
- **Capture system** (`vmux_layout/src/archive.rs`): consume
  `PageArchiveRequest`, spawn an `ArchivedPage` entity with `closed_at = now`.
  Enforce the cap: if more than 25 entries exist, despawn the oldest by
  `closed_at`.
- v1 captures the **stack close** path (`cmd+w`), the primary close gesture.
  Tab/pane bulk-close emitting one `PageArchiveRequest` per contained page is a
  noted future extension, out of scope here.

### 3. Reopen: `cmd+shift+t`

- **Shortcut:** already registered via `accel = "super+shift+t"`. Add a Linux
  binding `#[shortcut(direct = "Ctrl+Shift+T")]`, relabel the menu item
  "Reopen Closed Stack" → **"Reopen Closed Page"** (project terminology: "page",
  not "stack", in user-facing copy), and unhide it.
- **Handler:** replace the no-op `StackCommand::Reopen` arm (`stack.rs`). Pick
  the `ArchivedPage` with the max `closed_at`. If none, no-op.
- **Resolve target space:** match `space_id` against `(Entity, &SpaceId)` with
  `With<Space>`. If not found, use the active space.
- **Build target tab:** create a new `Tab → PaneSplit → leaf Pane → Stack` under
  the resolved Space entity (mirror the hierarchy build in
  `vmux_layout/src/window.rs`, `spawn_requested_tab_layouts`). This is required
  because no existing primitive opens a tab into a non-active space.
- **Reconstruct by kind** into the new stack:
  - web → `PageOpenRequest { target: Stack(new), url, request_id: None }`
  - terminal → `PageOpenRequest` with `vmux://terminal/?cwd=…` (fresh shell at the
    captured cwd)
  - agent → `SpawnAgentInStackRequest { kind, cwd, session_id, stack: new }`,
    where `session_id` is recovered from the captured url suffix
    (`vmux://agent/{kind}/{sid}`) if present, else `None`. This mirrors how
    `rebuild_space_views` resumes agents on restart — reopening an agent page
    resumes its session when one existed, and starts fresh otherwise.
- **Consume:** despawn the `ArchivedPage` entity. Repeated presses reopen
  successively older pages.

### 4. Purge + cap

- **`maintain_archive` system (Update):** scans all `ArchivedPage` entities each
  frame (≤25, so trivially cheap) and despawns any where
  `now_millis() - closed_at > 30 * 86_400_000` (30 days), then trims the survivors
  to the 25 most-recent by `closed_at`, despawning the oldest overflow.
- Running every Update keeps the bound continuously enforced and needs no timer
  resource; it only does work (despawns) when something is expired or over cap.

### 5. Persistence

- Add `.allow::<ArchivedPage>()` to the allowlist in `save_space_to_path`
  (`persistence.rs`), and `app.register_type::<ArchivedPage>()` where the other
  `vmux_core` types are registered.
- **Extend dirty-tracking:** `mark_dirty_on_change` does **not** currently watch
  `ArchivedPage`, so add `Added<ArchivedPage>` + `RemovedComponents<ArchivedPage>`
  to it. Otherwise capture/reopen/purge would only persist incidentally (capture
  co-occurs with a `Stack` despawn, but a standalone purge would wait for the 60s
  periodic save).
- **No `STORE_SCHEMA_VERSION` bump.** Bumping deletes the user's store on
  upgrade (and would wipe all saved spaces). Old stores simply load with zero
  `ArchivedPage` entities. (A downgrade to an older binary reading a newer store
  would hit `UnregisteredButReflectedType`, which is logged — acceptable.)
- **Verification required during implementation:** confirm `rebuild_space_views`
  ignores standalone `ArchivedPage` entities (no `PageMetadata`, not under a
  `Stack`) and that its pre-rebuild despawn does not remove them.

## Non-goals

- No archive panel, list, or search.
- Web restore is **URL only** — no scroll position, back/forward history, or form
  state.
- Terminal restore is a **fresh respawn** at the captured cwd — no scrollback, no
  live process. Agent restore resumes the captured session id when present (no
  live process is revived, but the agent CLI reattaches its session), else starts
  fresh.
- No dedup — closing the same URL twice produces two entries.
- No exact pane-split restoration — a reopened page lands as a new tab in its
  origin space, not its former split position.

## Testing

Bevy message + system integration tests, chained `App` builder (per project
convention):

- **Capture:** emit `PageArchiveRequest` → assert an `ArchivedPage` is spawned
  with the right fields. Emit 26 → assert the oldest is dropped (cap 25). Emit
  with empty url → assert skipped.
- **Reopen:** seed two `ArchivedPage` entities → dispatch `StackCommand::Reopen`
  → assert the correct open message is emitted (most-recent url, target stack in
  the origin space) and the entry is consumed. Empty archive → no-op. Missing
  origin space → active-space fallback.
- **Purge:** seed one old (`closed_at` > 30d) and one new entry → run the sweep
  → assert only the old one is despawned.

## Files touched

- `vmux_core` — `ArchivedPage` component, `PageArchiveRequest` message, type
  registration.
- `vmux_layout/src/archive.rs` — new plugin: capture, purge, reopen systems.
- `vmux_layout/src/stack.rs` — emit `PageArchiveRequest` at close; implement the
  `Reopen` arm.
- `vmux_command/src/command.rs` — Linux shortcut, menu relabel, unhide.
- `vmux_desktop/src/persistence.rs` — add `ArchivedPage` to the allowlist.
