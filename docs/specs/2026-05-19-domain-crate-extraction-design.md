# Domain Crate Extraction Design

## Problem

`vmux_desktop` is ~16k lines and owns logic that belongs to four other domain crates:

| Source (in `vmux_desktop`) | Should belong to | Lines |
|----------------------------|------------------|-------|
| `settings.rs`, `settings_view.rs`, `themes.rs` | `vmux_settings` | 1718 |
| `agent.rs`, parts of `agent_query.rs` | `vmux_agent` | ~1850 |
| `command_bar.rs`, `command.rs` | `vmux_command` | 2759 |
| `terminal.rs`, `terminal/launch.rs`, `terminal/pid.rs` | `vmux_terminal` | 3011 |

After this PR, `vmux_desktop` owns only desktop-shell concerns: top-level `VmuxPlugin`, primary window setup, OS menu, tray, persistence, updater, background lifecycle, profile, scene, browser, spaces, processes monitor, clipboard, shortcut.

This follows the boundary pattern established in VMX-121 for `vmux_layout`: cross-crate communication via Bevy `Message` types and ECS components, not direct function calls. Owner crates expose `Plugin`s registered by `VmuxPlugin`.

## Scope and non-goals

In scope:
- Move four file groups into their owning crates.
- Establish message-typed boundaries where cross-crate calls were previously direct.
- Each extracted crate exposes one top-level `Plugin`.
- All existing tests pass; new tests cover each new message round-trip.
- **Companion cleanup:** rename `PROCESSES_WEBVIEW_URL` → `SERVICES_WEBVIEW_URL` and dedupe to a single definition (see below).

Out of scope:
- Extracting `browser.rs` (no `vmux_browser` crate today; would require creating one).
- Extracting `spaces.rs` into `vmux_space` (`vmux_space` currently owns DTOs/events only; moving in the Bevy systems is its own design).
- Renaming the `Tab` component to `Space` (per VMX-121 follow-up notes).
- Renaming `PROCESSES_LIST_EVENT`/`PROCESSES_NAVIGATE_EVENT`/`ProcessesMonitor` component/`processes_monitor.rs` module (follow-up).
- Any user-visible behavior changes.
- Saved-layout migrations.

## Companion cleanup: `PROCESSES_WEBVIEW_URL` rename + dedup

Two crates currently define the same constant:

```
crates/vmux_layout/src/event.rs:4         pub const PROCESSES_WEBVIEW_URL: &str = "vmux://services/";
crates/vmux_service/src/webview/event.rs  pub const PROCESSES_WEBVIEW_URL: &str = "vmux://services/";
```

The name lies (`PROCESSES_*` but the URL is `vmux://services/`). The `Process` / `ProcessId` types in `vmux_service` are unrelated — those are PTY child processes managed by the daemon, not the services-monitor webview.

**Fix:**
1. Rename the constant to `SERVICES_WEBVIEW_URL` in `vmux_service::webview::event` (the single canonical location).
2. Delete the duplicate from `vmux_layout::event`.
3. Update all importers to use `vmux_service::webview::event::SERVICES_WEBVIEW_URL`. `vmux_layout` already depends on `vmux_service` after VMX-121, so this adds no new dep edge.

Renaming the related event names (`PROCESSES_LIST_EVENT`, `PROCESSES_NAVIGATE_EVENT`), the `ProcessesMonitor` component, and `processes_monitor.rs` module is **out of scope** — listed for a follow-up so this PR doesn't grow further. Constant rename alone is a small, mechanical change that lands as one commit in this PR.

## Sequencing (single PR, multiple commits)

The four extractions are mostly independent but share some helpers. The PR lands them in this order so intermediate commits always compile:

1. **`vmux_settings`** — least coupled. Reads settings file, watches for changes, broadcasts via Bevy resource. `settings_view.rs` is a `SettingsView` webview app already structured as a plugin.
2. **`vmux_command`** — `command_bar.rs` is a webview-driven command dispatcher. Touches focused-stack, settings, agent providers — but only reads, doesn't own anything they own. Routes commands via `AppCommand` (already in `vmux_command::command`).
3. **`vmux_terminal`** — `terminal.rs` owns Terminal entity lifecycle, CEF/IPC, PID tracking. Couples to service client and to layout's `LayoutSpawnRequest` (already in `vmux_layout`). Browser interaction is one-way: `spawn_url_into_stack` spawns `Browser` from `vmux_desktop::browser::Browser` — stays in desktop, called via a new spawn-request message.
4. **`vmux_agent`** — moved last because it has the most cross-coupling. References everything else: settings, command, terminal, browser, spaces, layout. After steps 1-3, those boundaries are clean messages/events; agent extraction is mostly mechanical reparenting.

## Architecture per crate

### `vmux_settings`

**Files moved:**
- `vmux_desktop/src/settings.rs` → `vmux_settings/src/plugin.rs` (or `settings_runtime.rs`)
- `vmux_desktop/src/settings_view.rs` → `vmux_settings/src/view.rs`
- `vmux_desktop/src/themes.rs` → `vmux_settings/src/themes.rs`

**Public surface:** `SettingsPlugin` registers all of:
- `AppSettings` resource and the file-watcher system
- `SettingsView` component + its broadcast/observer systems
- `SettingsSchemaEvent` / `SettingsListEvent` / `UpdateSettingsEvent` message types (already in `vmux_settings::event` per existing crate structure)

**Boundary with desktop:** `vmux_desktop` adds `SettingsPlugin` to `VmuxPlugin`. Other crates that need to read settings continue to use `Res<AppSettings>` — `AppSettings` itself moves into `vmux_settings`.

**`AppCommand::UpdateSettings`** dispatch stays where it lives (currently `vmux_desktop::agent.rs::handle_agent_commands`); the agent crate will own that after step 4 anyway.

### `vmux_command`

**Files moved:**
- `vmux_desktop/src/command_bar.rs` → `vmux_command/src/command_bar.rs`
- `vmux_desktop/src/command.rs` (currently 1 line — drop or fold into `vmux_command::command`)

**Public surface:** `CommandBarPlugin` (renamed from `CommandBarInputPlugin`) owns:
- The command bar's webview entity, opening/closing, payload broadcast, navigation handler
- `NewStackContext` resource (currently shared with `vmux_layout` and `vmux_desktop`)
- The `CommandBarSpace` DTO + `WebviewMaterial`/Mesh setup for the bar

**Boundary with other crates:**
- Reads `Res<AppSettings>` (from `vmux_settings`).
- Reads `Res<ActiveSpace>` (from `vmux_desktop::spaces` — stays in desktop for this PR).
- Writes `LayoutSpawnRequest` (in `vmux_layout`).
- Writes `AppCommand` (the dispatched command). The agent crate consumes via its own dispatch system.

**Cross-crate references:** `command_bar.rs` has 50+ direct references to `crate::agent::AgentStrategies`, `crate::browser::Browser`, `crate::terminal::Terminal`, `crate::spaces::ActiveSpace`, `crate::processes_monitor`. After the moves these become `vmux_agent::strategy::AgentStrategies`, `vmux_desktop::browser::Browser`, `vmux_terminal::Terminal`, `vmux_desktop::spaces::ActiveSpace`, `vmux_desktop::processes_monitor`. New dep edges introduced: `vmux_command → vmux_agent` and `vmux_command → vmux_terminal`. Neither creates a cycle (verified: `vmux_agent` and `vmux_terminal` don't depend on `vmux_command`).

### `vmux_terminal`

**Files moved:**
- `vmux_desktop/src/terminal.rs` → `vmux_terminal/src/runtime.rs` (or split)
- `vmux_desktop/src/terminal/launch.rs` → `vmux_terminal/src/launch.rs`
- `vmux_desktop/src/terminal/pid.rs` → `vmux_terminal/src/pid.rs`

**Public surface:** `TerminalRuntimePlugin` (distinct from existing `TerminalPlugin` which is the webview app definition):
- `Terminal` component, `ProcessExited`, `RestartPty` event
- `ServiceClient` resource (terminal owns the IPC channel)
- All keyboard/scroll/mouse routing systems
- `spawn_layout_requested_content` (handler for `LayoutSpawnRequest::Terminal`)

**Boundary with other crates:**
- Reads `Res<AppSettings>` (`vmux_settings`).
- Reads `Res<ActiveSpace>` (`vmux_desktop::spaces`).
- Spawns `Browser` for `LayoutSpawnRequest::OpenUrl` paths — depends on `vmux_desktop::browser`. Either (a) terminal crate adds dep on desktop (cycle!), or (b) the URL-routing system splits: terminal handles `vmux://terminal/`, a new system in desktop handles browser/agent/processes/spaces routes. **Resolution: (b)** — move only the terminal-spawn branch into `vmux_terminal`; the rest stays in `vmux_desktop` as a small `layout_spawn_router.rs`.

**Boundary with vmux_agent:** `terminal.rs` currently references `vmux_agent::session::AgentSession`, `vmux_agent::strategy::AgentStrategies`, `vmux_agent::AgentKind`. These come from `vmux_agent` which is a dep of `vmux_terminal`? No — currently `vmux_terminal` has no `vmux_agent` dep. The cross-reference is via `vmux_desktop::agent::*` which re-exports/uses them. After agent extraction, these become direct `vmux_agent::*` references; need to verify no cycle.

### `vmux_agent`

**Files moved:**
- `vmux_desktop/src/agent.rs` → `vmux_agent/src/desktop_plugin.rs` (or fold into existing `vmux_agent::plugin`)
- `vmux_desktop/src/agent_query.rs` (the `GetSettings` arm) → moves to `vmux_settings` as a small handler

**Public surface:** Extended `AgentSessionPlugin` (existing) or new `AgentDesktopPlugin`:
- `AgentCommandRequest` / `AgentQueryRequest` message types
- `handle_agent_commands` system (the big one)
- `AgentProviders` resource and registration

**Boundary with other crates:**
- Writes `LayoutApplyRequest` / `LayoutSnapshotRequest` (from `vmux_layout`).
- Reads `Res<AppSettings>` (`vmux_settings`).
- Reads/writes various components in `vmux_terminal`, `vmux_desktop::browser`, `vmux_desktop::spaces`.

Most of `agent.rs::handle_agent_commands` is dispatch — `BrowserNavigate`, `TerminalSend`, `RunShell`, etc. After this PR, those dispatchers live in `vmux_agent` and call into the owner crates via either (a) public API on those crates' components, or (b) typed messages (`BrowserNavigateRequest`, `TerminalSendRequest`).

**Preferred: (b)** — each owner crate exposes a request message, `vmux_agent` writes it, the owner consumes. Symmetric with VMX-121's `LayoutApplyRequest`. Trade-off: ~6 new message types. Decision: do it; the boundary clarity outweighs the type proliferation.

## Final dep graph

```
vmux_core
  ↑
vmux_protocol-like types (Layout DTOs in vmux_layout)
  ↑
vmux_settings  vmux_layout  vmux_terminal  vmux_agent  vmux_command
  ↑              ↑             ↑             ↑           ↑
  └──────────────┴──── vmux_desktop ──────────┴───────────┘
                         (shell concerns)
```

New edges to add (none should create a cycle):
- `vmux_settings` → `vmux_core` (already), `vmux_webview_app`, `vmux_ui` (for the SettingsView Dioxus app)
- `vmux_command` → `vmux_layout` (for `LayoutSpawnRequest`, `Pane`, etc.), `vmux_agent` (for agent-provider lookup during command dispatch — confirm needed), `vmux_terminal` (for `Terminal` marker in pane queries)
- `vmux_terminal` → `vmux_settings` (for `AppSettings`), `vmux_layout` (for layout types it references — already implicit via `LayoutSpawnRequest`)
- `vmux_agent` → `vmux_settings`, `vmux_layout`, `vmux_terminal`, `vmux_desktop::browser` (cycle hazard — see below)

**Cycle hazard:** `vmux_agent` writing into `vmux_desktop::browser` would create `vmux_agent → vmux_desktop` and (transitively) `vmux_desktop → vmux_agent`. Resolution: introduce a `BrowserNavigateRequest` message owned by some neutral location (probably `vmux_layout` since `LayoutSpawnRequest::OpenUrl` already covers most cases), and have `vmux_desktop::browser` consume it. Then `vmux_agent` writes the message, not direct component access.

## Message inventory (new)

To eliminate direct cross-crate calls from `vmux_agent` → other domains:

| Message | Owner | Producer | Consumer |
|---------|-------|----------|----------|
| `BrowserNavigateRequest { url, pane }` | `vmux_layout` (extends `LayoutSpawnRequest`) | `vmux_agent`, `vmux_command` | `vmux_desktop::browser` |
| `TerminalSendRequest { text, terminal_id }` | `vmux_terminal` | `vmux_agent` | `vmux_terminal` |
| `RunShellRequest { command, cwd, mode }` | `vmux_terminal` | `vmux_agent` | `vmux_terminal` |
| `AppCommandRequest { id }` | `vmux_command` | `vmux_agent` | `vmux_command` (already exists as `AppCommand` event) |

## Testing

- Each extracted crate's pre-existing tests move alongside the code.
- New round-trip tests for each new message boundary (mirror VMX-121's pattern in `vmux_layout::reconcile::tests`).
- Manual smoke after each commit: app launches, layout works, MCP `read_layout`/`update_layout` still functions, command bar opens, agent panes spawn.

## Risks

- **Cycle discovery during impl** — if a planned dep edge creates a cycle, the fix is to introduce another message hop, which is mechanical but adds review surface.
- **Bevy ECS API surface** — public re-exports may need adjustment as types cross crate boundaries (e.g., `Component` derives need `pub use` chains).
- **Saved-layout/persistence breakage** — `moonshine_save` traits may need `#[type_path]` updates when components move crates. Test save/load round-trip after each extraction.

## Implementation order

Within the PR, commits land in this order:

0. **Companion:** rename `PROCESSES_WEBVIEW_URL` → `SERVICES_WEBVIEW_URL` in `vmux_service::webview::event`, delete duplicate from `vmux_layout::event`, update importers. One commit, mechanical.
1. Create empty `vmux_settings::plugin` skeleton + `SettingsPlugin`; move `themes.rs` (purely data, no Bevy systems).
2. Move `settings.rs` into `vmux_settings` (file watcher, `AppSettings` resource).
3. Move `settings_view.rs` into `vmux_settings`.
4. Update all `vmux_desktop` callers to import `AppSettings` from `vmux_settings`.
5. Add `BrowserNavigateRequest` / `TerminalSendRequest` / `RunShellRequest` messages (consumers still in desktop at this point — just routing through messages).
6. Move `command_bar.rs` into `vmux_command::command_bar`.
7. Move `terminal.rs` + `terminal/*` into `vmux_terminal::runtime`.
8. Move `agent.rs` (and `agent_query.rs` GetSettings arm) into `vmux_agent`.
9. Delete `vmux_desktop::layout_response.rs` if no longer needed (depends on whether the forwarder logic stays in desktop or moves to vmux_agent).
10. Update PR description with final dep graph.

Each commit verified via per-crate `fmt + clippy + test` per AGENTS.md.
