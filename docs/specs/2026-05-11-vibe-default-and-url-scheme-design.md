# Vibe-default new pane + URL scheme realignment

**Date:** 2026-05-11
**Status:** Draft

## Summary

Four coordinated changes to vmux:

1. Add a configurable `startup_url` setting. When vmux creates a fresh empty pane (new stack/tab/pane/space, or app start with no restored layout), it navigates that pane to `startup_url`. Default is `vmux://vibe/` when the `vibe` CLI is detected on disk, else `vmux://terminal/`.
2. Remove the auto-open of the command bar on empty-pane creation. The command bar is now invoked exclusively via its keybinding; it works as a manual toggle on every page.
3. Add a new URL route `vmux://vibe/<session>` that identifies a vibe pane by its vibe CLI session UUID.
4. Change the existing terminal URL form from `vmux://terminal/<process-id-uuid>` to `vmux://terminal/<os-pid>`. Hard cut, no UUID compatibility.

## Motivation

`vibe` is the primary day-to-day surface for users who have it installed; making them dismiss the command bar to launch it is friction. A `startup_url` is the right primitive — it's a single point of override that covers the common case (default to vibe) without coupling default-pane behavior to the command-bar machinery, and it lets power users redirect new panes to anything addressable by URL (`vmux://services/`, `vmux://spaces/`, a regular http URL, etc.).

Exposing OS PIDs and vibe session IDs in URLs makes panes addressable by the same identifiers users see in `ps` / `ls ~/.vibe/logs/session/`, which is what users (and external tooling like the MCP server) reach for when they want to point at a specific pane.

## Architecture

### Identity model

- **`ProcessId` (UUID, internal)** — unchanged. Stays as the entity-level handle across the desktop ↔ service protocol. All component lookups, `ServiceProcessHandle`, and protocol messages keep using `ProcessId`.
- **OS PID (`u32`, URL-only)** — exposed in URLs and via a reverse-lookup map. Stamped on the entity as a `Pid` component once the service responds with `ProcessCreated`.
- **Session id (UUID string, URL-only)** — exposed in URLs and via a reverse-lookup map. Stamped on the entity as a generic `SessionId` component once discovery succeeds.

The internal UUID is kept because (a) it exists at entity-creation time before the service has spawned anything, (b) it is stable across PTY respawn (preserves history/webview handle), and (c) it sidesteps PID-reuse correctness bugs at the protocol layer.

### Composable markers

The existing `Pane` marker (`crates/vmux_layout/src/pane.rs:69`) marks any layout-occupying entity. Two new markers compose with it:

- `Terminal` — entity is backed by a PTY. Stamped on every shell-style entity.
- `Vibe` — entity is running the vibe CLI. Stamped on vibe-launched entities. A vibe entity also has `Terminal` (vibe runs over PTY).

Components are kept generic and orthogonal:

- `Pid(u32)` — OS PID; can be stamped on anything backed by a process. Today only `Terminal` entities get one.
- `SessionId(String)` — generic agent/session identifier. Today only `Vibe` entities get one; future agents (e.g., a claude pane) would stamp their own marker plus reuse `SessionId`.

The URL formatter system reads `(Vibe, Terminal)` markers and `(Pid, SessionId)` components to decide which scheme to emit. Vibe wins precedence over Terminal when both are present.

### Startup URL

A new field on `AppSettings`:

```rust
pub struct AppSettings {
    // ... existing fields
    #[serde(default)]
    pub startup_url: Option<String>,
}
```

A resolver returns the effective URL each time it's needed:

```rust
fn resolve_startup_url(settings: &AppSettings) -> String {
    settings.startup_url.clone().unwrap_or_else(|| {
        if vibe::vibe_available() {
            "vmux://vibe/".to_string()
        } else {
            "vmux://terminal/".to_string()
        }
    })
}
```

`vibe_available()` is called inside the resolver; the result is **not** cached. Each new-pane request re-evaluates. If this proves too costly (filesystem scan over PATH + fallback dirs), we cache lazily via `OnceCell` on first call, then re-resolve only on settings reload. Out of scope for this change — re-evaluate if a perf regression shows up.

A malformed or unparseable `startup_url` falls back to spawning an empty terminal pane (same as `vmux://terminal/`) and logs a warning. We do not crash the new-pane path.

### Command bar — manual toggle only

Today the command bar opens automatically in two paths:

- `NewStackContext.needs_open = true` (set in `vmux_layout::stack::handle_stack_command` when a new stack is created)
- `open_command_bar_if_no_stacks` (runs each frame; opens the modal when the active space has zero stacks)

Both auto-open paths are removed. Their replacement: when a new empty pane is created, navigate it to `startup_url`. The command bar is openable only via its existing keybinding, from any page.

`NewStackContext` itself is repurposed (or partially gutted) — we still need to track which stack is "newly created and awaiting initial content" so the URL-dispatch system can route the startup_url to the right entity.

## Components

### Service protocol change

**File:** `crates/vmux_service/src/protocol.rs`

```rust
// Before
ProcessCreated { process_id: ProcessId },

// After
ProcessCreated { process_id: ProcessId, pid: u32 },
```

The service already tracks `Process::pid: u32`. Propagating it into the existing message is a one-field addition with no other protocol-shape changes.

If `child.process_id()` returns `None` at spawn time, the service emits `ProcessCreateFailed { reason }` instead of falling back to PID `0`. The `unwrap_or(0)` at `process.rs:229` becomes a hard error path. Defaulting to `0` would make `vmux://terminal/0` a possible URL, which would alias every spawn-failure pane.

### Desktop components & resources

**New file:** `crates/vmux_desktop/src/terminal/pid.rs` (filename-based module per project rules — no `mod.rs`).

```rust
#[derive(Component)]
pub struct Terminal;

#[derive(Component)]
pub struct Pid(pub u32);

#[derive(Resource, Default)]
pub struct PidToEntity(pub HashMap<u32, Entity>);
```

**New file:** `crates/vmux_desktop/src/vibe/session.rs`.

```rust
#[derive(Component)]
pub struct Vibe;

#[derive(Component)]
pub struct SessionId(pub String);

#[derive(Component)]
pub struct PendingVibeSession {
    pub spawn_time: SystemTime,
    pub cwd: PathBuf,
    pub attempts: u8,
}

#[derive(Resource, Default)]
pub struct VibeSessionToEntity(pub HashMap<String, Entity>);
```

`SessionId` lives in `vibe/session.rs` because vibe is the only consumer today. If a second agent ships, promote it to `crates/vmux_desktop/src/agent/session.rs` (or wherever agent-shared code lands) — refactor at that point, not before.

The `VibeSessionToEntity` map name stays vibe-scoped because the `vmux://vibe/<session>` URL specifically resolves to vibe panes; a future `vmux://claude/<session>` would have its own `ClaudeSessionToEntity` map and own marker.

No `DefaultPaneKind` resource — replaced by the `startup_url` setting + resolver.

### URL formatter system

Replaces the static `format!` in `terminal.rs:317`. A Bevy system runs each frame over entities whose URL inputs changed. Marker precedence: `Vibe` beats `Terminal` because every vibe entity also has `Terminal`.

- Entity has `Vibe`:
  - With `SessionId(id)` → `PageMetadata.url = format!("vmux://vibe/{id}")`.
  - Without → `PageMetadata.url = "vmux://vibe/"` (placeholder during discovery).
- Else entity has `Terminal`:
  - With `Pid(pid)` → `PageMetadata.url = format!("vmux://terminal/{pid}")`.
  - Without → `PageMetadata.url = "vmux://terminal/"` (placeholder during spawn race).

Reverse-lookup maps stay in sync via `RemovedComponents<Pid>` and `RemovedComponents<SessionId>` cleanup systems running before any URL-resolution system in the same frame. The `SessionId` cleanup hook only removes from `VibeSessionToEntity` if the despawning entity also had `Vibe` (avoids touching the wrong map when other agent types arrive).

### Vibe session discovery

**File:** `crates/vmux_desktop/src/vibe/session.rs`.

A polling system runs every 200ms over entities with `PendingVibeSession`:

1. Read `~/.vibe/logs/session/` directory listing.
2. Filter to subdirs whose `meta.json` parses with:
   - `working_directory == entity.cwd` (string equality on canonical paths).
   - `start_time >= entity.spawn_time` (parse ISO-8601, compare as `SystemTime`).
   - `session_id` is **not** already present in `VibeSessionToEntity` (excludes sessions already claimed by other panes).
3. If multiple match, pick the earliest `start_time` (closest to this pane's spawn — more likely to be ours than a later concurrent spawn).
4. On match: read `session_id` field from `meta.json`, stamp `SessionId` component, remove `PendingVibeSession`, insert into `VibeSessionToEntity`.
5. On no match: increment `attempts`. If `attempts >= 30` (~6s), log warning and remove `PendingVibeSession`. Pane stays functional with placeholder URL.

`meta.json` is written by vibe at session start (verified by inspecting an existing session: the file contains `session_id`, `start_time`, `environment.working_directory` from the moment vibe boots). End-of-session fields like `end_time` are added later but the start-time fields are present immediately.

### Vibe launch helpers

**File:** `crates/vmux_desktop/src/vibe.rs` — extend the existing launch builder at lines 203–211.

Two callers:

- **Fresh session** (`vmux://vibe/` no-path, including default `startup_url` flow): `vibe --trust`. Discovery runs.
- **Resume** (`vmux://vibe/<session>` for unknown session): `vibe --trust --resume <session>`. Discovery skipped — session ID is already known and stamped on the entity at spawn time.

Both share the existing `bash -lc 'cd "$1" && VIBE_MCP_SERVERS="$2" exec "$3" --trust [...]' bash <cwd> <mcp_json> <vibe_path>` template.

### URL parser & dispatcher

**File:** `crates/vmux_desktop/src/agent.rs` — extend `spawn_vmux_tab` (lines 266–308).

Add `vibe` host arm. Updated routing table:

| Host | Path | Action |
|------|------|--------|
| `terminal` | empty | Spawn new terminal |
| `terminal` | `<u32>` | Look up `PidToEntity`. Hit → focus pane. Miss → log error, navigate to `startup_url`. |
| `vibe` | empty | Spawn new fresh vibe pane (discovery flow) |
| `vibe` | `<session>` | Look up `VibeSessionToEntity`. Hit → focus pane. Miss → spawn new pane with `--resume <session>`, stamp `SessionId(session)` immediately. |
| `sessions`, `services` | — | unchanged |

**File:** `crates/vmux_desktop/src/command_bar.rs` — `parse_process_id_from_url` at lines 42–45 becomes `parse_pid_from_url` returning `Option<u32>`. The `on_command_bar_action` handler's terminal arm uses `PidToEntity` for reattach lookups.

UUID-form URLs (`vmux://terminal/<uuid>`) no longer parse — hard cut.

### New-stack handler

**File:** `crates/vmux_layout/src/stack.rs` — `handle_stack_command` (line 188 onward).

Today's path: `StackCommand::New` → set `NewStackContext.needs_open = true` → command bar modal opens.

New path:

1. `StackCommand::New` creates the empty stack entity.
2. Handler reads `AppSettings.startup_url` (via resolver) and dispatches it through the same machinery the URL navigation uses (`spawn_vmux_tab` / equivalent).
3. The dispatched URL bundles whatever components that URL kind needs (e.g., `Vibe` + `Terminal` + `PendingVibeSession` + `PendingServiceCreate` for `vmux://vibe/`), targeting the new stack entity.
4. `NewStackContext.needs_open` is no longer set. The field stays for the bookkeeping it does (`previous_stack`, `dismiss_modal`) but loses `needs_open` if nothing else reads it; that's a code-cleanup detail of the implementation.

`open_command_bar_if_no_stacks` (lines 537+): replaced with `open_startup_url_if_no_stacks` that calls the same dispatch logic. Renamed to reflect new behavior.

The command bar's existing keybinding handler is untouched.

## Data flow

### New pane (default `startup_url = "vmux://vibe/"`)

```
User: Cmd+T (or shortcut)
  → StackCommand::New
  → handle_stack_command creates empty stack entity
  → reads resolve_startup_url(&settings) → "vmux://vibe/"
  → dispatches via spawn_vmux_tab(url, target_entity)
  → vibe-host arm with empty path:
       bundle entity with:
         PendingServiceCreate { shell: vibe-cmd, cwd }
         Terminal
         Vibe
         PendingVibeSession { spawn_time: now(), cwd, attempts: 0 }
         PageMetadata.url = "vmux://vibe/" (placeholder)
  → poll_service_messages sends CreateProcess to service
  → service spawns PTY, returns ProcessCreated { process_id, pid }
  → desktop adds ServiceProcessHandle(process_id) + Pid(pid)
  → discovery system polls ~/.vibe/logs/session/ each 200ms
  → on match: stamp SessionId(id), remove PendingVibeSession,
     insert into VibeSessionToEntity
  → URL formatter updates PageMetadata.url to "vmux://vibe/<session>"
```

### URL navigation

```
vmux://terminal/12345
  → parse 12345 as u32
  → PidToEntity.get(&12345)
  → Some(entity): focus that pane's tab
  → None: log "no pane for pid 12345", navigate to startup_url instead

vmux://terminal/
  → spawn new terminal

vmux://vibe/ae724a54-c387-...
  → VibeSessionToEntity.get(&id)
  → Some(entity): focus that pane's tab
  → None: spawn new entity with vibe-cmd containing --resume <session>,
     mark Terminal + Vibe, stamp SessionId(id) at spawn time, skip discovery,
     insert into VibeSessionToEntity

vmux://vibe/
  → spawn fresh vibe (same path as default new-pane flow)
```

## Error handling & edge cases

- **PID 0 / spawn failure** — service emits `ProcessCreateFailed`; entity gets a failure marker, URL stays empty. No `vmux://terminal/0` ever produced.
- **PID reuse** — `RemovedComponents<Pid>` cleanup system runs before URL-nav resolution each frame. Worst case: same-frame reuse falls into the new entity, which is the correct behavior (the OS has decided that PID 12345 is now this new process).
- **Discovery race** — two vibe panes in the same cwd spawned within the same poll window. The "not already in `VibeSessionToEntity`" filter ensures each session_id is claimed by exactly one entity. The "earliest `start_time` ≥ spawn_time" tiebreak means the first entity polls and grabs the earlier-created dir; the second entity sees that dir already claimed and falls through to the later dir. Each entity ends up with its own session.
- **Discovery timeout** — pane stays usable, URL placeholder. Log warning.
- **Vibe binary disappears mid-session** — same handling as a regular shell exit. Out of scope.
- **Old `vmux://terminal/<uuid>` references** (e.g., MCP commands stored in vibe history) — fail to parse as `u32`, return error, log. Acceptable: pre-1.0, no public API contract.
- **Malformed `startup_url`** — log warning, fall back to spawning empty terminal (`vmux://terminal/`).
- **`startup_url` set to a specific-pane URL** (e.g., `vmux://terminal/12345` or `vmux://vibe/<session>`) — every new pane tries to focus that pane, which means subsequent `Cmd+T` presses focus the same existing tab instead of creating new tabs. Behavior is consistent with the URL semantics; we do not special-case it. Documented in changelog/settings notes.
- **`startup_url` pointing at a regular http URL** (e.g., `https://example.com`) — works; the URL dispatcher already handles non-vmux URLs through the browser webview path.
- **Multiple spaces** — `PidToEntity` and `VibeSessionToEntity` are global resources, not space-scoped. URL navigation can target a pane in any space and the existing pane-focus machinery handles cross-space switching.

## Testing

**Unit:**
- URL formatter for both schemes including placeholder cases (`vmux://terminal/` when no PID, `vmux://vibe/` when no session).
- URL parser for `vmux://terminal/<pid>`: numeric only, leading zeros, bounds (`u32::MAX`, overflow).
- URL parser for `vmux://vibe/<session>`: UUID string round-trip.
- `PidToEntity` insert on `Pid` add, removal on entity despawn.
- `VibeSessionToEntity` same lifecycle.
- Vibe session discovery against a fixture `~/.vibe/logs/session/` directory: cwd match, start_time filter, multiple-match tiebreak.
- `resolve_startup_url`: returns user override when set; returns `vmux://vibe/` when override is None and `vibe_available()` is true; returns `vmux://terminal/` when override is None and `vibe_available()` is false.
- New-stack handler dispatches `startup_url` instead of opening command bar (regression — the existing tests at `stack.rs:621+` that assert `needs_open == true` must be updated).
- `open_startup_url_if_no_stacks`: when active space has no stacks, dispatches startup_url to a freshly spawned stack.

**Integration:**
- Spawn vibe → assert URL transitions from `vmux://vibe/` to `vmux://vibe/<session>` within 6s. Test guarded with `#[cfg]` or skipped when `vibe` binary not present (matches existing pattern in vibe.rs).

**Manual (vibe-installed machine):**
- `Cmd+T` (or new-stack shortcut) launches vibe; no command bar appears.
- Command-bar keybinding opens command bar from any page.
- Open vibe pane, copy its URL, close pane, paste URL into command bar → spawns `vibe --resume <session>`.
- Spawn terminal, copy `vmux://terminal/<pid>`, switch tabs, paste URL → focuses original terminal.
- Set `startup_url = "vmux://services/"` in settings.ron, restart vmux, `Cmd+T` opens services view.
- Uninstall vibe binary, restart vmux, `Cmd+T` opens regular terminal (not command bar).

## Affected crates (lint/test scope per AGENTS.md)

- `vmux_service` — protocol message field added, spawn-failure path.
- `vmux_desktop` — settings (new `startup_url` field), terminal, vibe, agent, command_bar modules; new `terminal/pid.rs` and `vibe/session.rs` files (filename-based modules per project rules).
- `vmux_layout` — `stack.rs` new-stack handler dispatches `startup_url`; `open_command_bar_if_no_stacks` renamed/refactored to `open_startup_url_if_no_stacks`; existing tests updated.

The lint loop in AGENTS.md will pick these up automatically based on `git diff main`.

## Out of scope

- Vibe `--continue` / interactive picker variants on default-pane spawn (locked: always fresh).
- UUID-form URL backwards compatibility (locked: hard cut).
- Replacing `ProcessId` UUID throughout the protocol (locked: keep internal; PID is URL-only).
- Per-space `startup_url` override (always global setting).
- Caching `vibe_available()` result (re-evaluated on each new-pane request; revisit only if perf regresses).
- Settings-reload hook for `startup_url` (changes take effect on next pane creation; no dynamic re-route of existing panes).
- Dynamic URL update on PTY respawn (out of normal flow; respawn keeps same `ProcessId` so the existing flow already handles it).
- Removing `NewStackContext` entirely — we keep the bookkeeping fields it provides; only `needs_open` becomes vestigial.
