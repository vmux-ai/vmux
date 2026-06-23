# Agent-Done Notification + Avatar Indicator — Design

Date: 2026-06-23
Status: Approved (pending spec review)

## Goal

When a CLI agent (claude / codex / vibe running in a vmux terminal) wants
attention — typically because it finished its turn — surface it two ways:

1. **OS notification** — a native macOS notification, but only when the user is
   *not currently looking at that agent's page* (vmux window unfocused, or a
   different stack/tab is active).
2. **Avatar indicator** — a "done" dot on that agent's avatar in the top-right
   facepile, which persists until the user views the agent's page, then clears.

Two triggers feed one shared downstream:

- **Terminal bell (BEL)** — automatic, passive. Fires with no agent cooperation
  if the agent rings the bell on completion. Coarse (no message text).
- **`vmux_notify` MCP tool** — intentional, rich. The agent calls it with an
  optional title/body. Won't fire automatically at turn end (the model must
  choose to call it), but gives a real message and is on-brand with vmux's
  self-testing MCP tools (screenshot, read_layout).

## How the triggers resolve to an agent entity

A CLI agent entity (spawned at `crates/vmux_agent/src/plugin.rs:1419-1439`)
carries, together: the `Terminal` bundle, a `ProcessId` (its PTY id),
`AgentSession`, `vmux_core::team::Profile`, `vmux_core::team::Agent`, and
`ChildOf(stack)`. Both triggers resolve to this same entity via its `ProcessId`:

- **Bell** carries the PTY `process_id`. The app already matches `process_id`
  against terminal entities in `poll_service_messages`
  (`crates/vmux_terminal/src/plugin.rs:1135`).
- **MCP** rides the existing `AgentCommand` path, which threads an
  `anchor: Option<ProcessId>` end-to-end (each MCP server is spawned with
  `--anchor <ProcessId>` = the agent's own PTY id,
  `crates/vmux_agent/src/mcp.rs:19-27`). The GUI command handler resolves the
  caller by matching `anchor` against each agent's `ProcessId` component
  (`crates/vmux_agent/src/plugin.rs:374-381`). (Note: the `AgentQuery` path used
  by `screenshot` does NOT carry identity — it broadcasts — which is why
  `vmux_notify` uses the `AgentCommand` path instead.)

## Why the terminal bell (and not run-state)

CLI agents have **no per-turn run-state**: `AgentRunState`
(`crates/vmux_agent/src/run_state.rs`) is only populated for in-app *Page* agents
(`client/page/plugin.rs:176`), so the facepile "running" dot is always off for
CLI agents. The cross-agent completion signal that already exists is the
terminal **bell** (BEL, `0x07`). Every PTY runs through `alacritty_terminal`,
whose event listener delivers `TermEvent::Bell` on a real bell. BELs that merely
terminate an OSC sequence are consumed by the parser and surface separately
(`osc_dispatch(.., bell_terminated:true)`), so they do **not** raise
`TermEvent::Bell` — no false positives.

**Dependency / caveat:** the bell path *surfaces* bells; it does not make agents
ring. Whether claude/codex/vibe ring on completion is each agent's own config —
out of scope. The `vmux_notify` tool exists precisely for agents that prefer an
explicit signal.

Platform scope: the OS notification is **macOS only**; non-macOS builds compile
the notify system to a no-op (the avatar dot works everywhere).

## Shared types (live in `vmux_core`)

`vmux_agent` depends on `vmux_terminal`, so the shared types cannot live in
`vmux_agent` (the bell producer is `vmux_terminal`). They live in
`vmux_core` (everyone depends on it), in a new `crates/vmux_core/src/notify.rs`:

```rust
#[derive(Message, Clone)]
pub struct BellReceived { pub process_id: ProcessId }   // bell, pre-resolution

#[derive(Message, Clone)]
pub struct AgentAttention {
    pub entity: Entity,
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Message, Clone)]
pub struct OsNotify { pub title: String, pub body: String }

#[derive(Component)]
pub struct AgentDoneUnseen;   // no `Save` → never persisted across restarts
```

The three messages are registered once in `AgentPlugin` (`add_message`). The
component needs no registration.

`BellReceived` keeps `vmux_terminal` dumb: `poll_service_messages` is a giant
multi-`SystemParam` system that is impractical to unit-test, so it only does a
trivial re-emit. The testable `process_id → agent entity` resolution lives in
`vmux_agent` (`agent_bell_to_attention`), which naturally filters non-agent
terminal bells (their `process_id` matches no `team::Agent` entity).

## Flow

```
TRIGGER A — bell:
  agent PTY emits BEL
   → alacritty → ServiceEventProxy::send_event(TermEvent::Bell)   [process.rs:33]
   → ServiceMessage::Bell { process_id }                          [protocol.rs:456, IPC → app]
   → poll_service_messages re-emits BellReceived{process_id}      [trivial passthrough]
   → agent_bell_to_attention: find team::Agent entity whose ProcessId == process_id
        → AgentAttention{entity, None, None}  (non-agent bells match nothing → dropped)

TRIGGER B — MCP:
  agent calls vmux_notify{title?,body?}
   → McpParamTool::Notify → AgentCommand::Notify{title,body}
   → ClientMessage::AgentCommand{request_id, anchor, command}     [carries anchor]
   → broker → ServiceMessage::AgentCommand{request_id, anchor, command}
   → GUI relay → AgentCommandRequest{origin: Agent{anchor}}
   → handle_agent_commands: resolve caller via ProcessId(anchor)
        → AgentAttention{entity: caller, title, body}; reply AgentCommandResult::Ok → MCP "ok"

SHARED downstream:
  AgentAttention
   → mark_agent_done:
       foreground? = window.focused && window.visible
                     && FocusedStack.stack == agent's ChildOf(stack)
       ├─ foreground → nothing (user already sees it)
       └─ background →
            insert AgentDoneUnseen (idempotent)
            + if no OsNotify for this entity within DEDUP_WINDOW → send OsNotify{title,body}
                ├─ OsNotify → vmux_desktop post_os_notifications → UNUserNotification
                └─ AgentDoneUnseen → emit_team sets TeamMemberRow.is_done_unseen
                     → TeamEvent → facepile amber dot                [page.rs:566]

CLEAR:
  clear_agent_done: for each AgentDoneUnseen entity, if
  window.focused && FocusedStack.stack == agent's stack → remove AgentDoneUnseen
   → emit_team re-broadcasts → dot disappears
```

`DEDUP_WINDOW` (~3 s) coalesces a bell and an MCP notify for the same turn into a
single OS notification, while still letting a later, separate turn re-notify.
The `AgentDoneUnseen` insert is unconditional/idempotent.

## Components

### 1. `vmux_core`
- `src/notify.rs` (new): `AgentAttention`, `OsNotify`, `AgentDoneUnseen` (above);
  `pub mod notify;` + re-exports in `lib.rs`.
- `src/event/team.rs`: add `pub is_done_unseen: bool` to `TeamMemberRow`
  (after `is_running`, line 46). Update the two struct-literal tests in this file.

### 2. `vmux_service`
- `protocol.rs`: add `AgentCommand::Notify { title: Option<String>, body: Option<String> }`
  (after `Run`, ~line 131) and `ServiceMessage::Bell { process_id: ProcessId }`
  (in `ServiceMessage`, ~line 530). rkyv round-trip tests.
- `process.rs`: in `ServiceEventProxy::send_event` (line 33), add
  `TermEvent::Bell => { let _ = self.patch_tx.send(ServiceMessage::Bell { process_id: self.process_id }); }`.
  Test mirrors the existing `Title → ProcessTitle` test (process.rs:2073-2088).

### 3. `vmux_mcp`
- `tools.rs`: add `McpParamTool::Notify { title: Option<String>, body: Option<String> }`
  with `#[mcp(description = "Notify the user that you (this agent) need their
  attention — typically that you have finished. Shows a macOS notification when
  they're not looking at your page, and a dot on your avatar until they view it.
  Optional title/body customize the message.")]`, and a `to_agent_command` arm →
  `AgentCommand::Notify { title, body }`. The macro auto-registers the tool in
  `tool_definitions()` and routes it through the existing
  `McpParamTool::from_mcp_call` branch (tools.rs:594); `tool_call_result`'s
  `DispatchTarget::Command(command) => run_agent_command(command, anchor)` arm
  (protocol.rs:135) already threads the anchor and returns `ok` on
  `AgentCommandResult::Ok`. Tests: `vmux_notify` listed; dispatches to
  `AgentCommand::Notify`.

### 4. `vmux_terminal`
- `plugin.rs` `poll_service_messages` (match at line 1100): add
  `ServiceMessage::Bell { process_id } => { writers.bell.write(BellReceived { process_id }); }`.
  Add a `BellReceived` writer to `PollServiceWriters`. No entity lookup here.
- (No unit test for this passthrough — the system's many `SystemParam`s,
  including `NonSend<Browsers>`, make it impractical; the meaningful logic is
  tested in `vmux_agent` below.)

### 5. `vmux_agent`
- `plugin.rs` `agent_bell_to_attention` (new system): read `BellReceived`; for
  each, find the entity whose `ProcessId` component equals `process_id` among
  `team::Agent` entities (same match as `handle_agent_commands` caller
  resolution, line 374-381) and write `AgentAttention { entity, title: None,
  body: None }`. Test: an agent entity with a matching `ProcessId` yields one
  `AgentAttention`; an unknown `process_id` yields none.
- `plugin.rs` `handle_agent_commands` (match at line 390): add
  `ServiceAgentCommand::Notify { title, body }` arm → if `caller` resolved, write
  `AgentAttention { entity: caller, title, body }` and return
  `AgentCommandResult::Ok`; else `AgentCommandResult::Error("notify: caller not found")`.
  Add `attention: MessageWriter<AgentAttention>` to the system.
- `plugin.rs` (new systems):
  - `mark_agent_done` (reads `AgentAttention`): compute foreground from
    `Query<&Window, With<PrimaryWindow>>` (`focused && visible`) and
    `Res<FocusedStack>` vs the agent's `ChildOf(stack)`. If background: insert
    `AgentDoneUnseen`; dedup via `Local<HashMap<Entity, f64>>` keyed on
    `Res<Time>` elapsed; if outside `DEDUP_WINDOW`, send `OsNotify` (title/body
    from the message, defaulting to `"<agent name> finished"` / space-or-tab
    context resolved from `Profile`).
  - `clear_agent_done`: for each `AgentDoneUnseen` entity, remove it when
    `window.focused && FocusedStack.stack == agent stack`.
  - Register `add_message::<BellReceived>()`, `add_message::<AgentAttention>()`,
    `add_message::<OsNotify>()`, and the systems `agent_bell_to_attention`,
    `mark_agent_done`, `clear_agent_done` (the latter two ordered
    `.after(vmux_layout::stack::ComputeFocusSet)` so `FocusedStack` is current).
- Tests: gating matrix {focused/unfocused}×{stack active/inactive} → `OsNotify`
  + `AgentDoneUnseen` only when background; dedup (two `AgentAttention` within the
  window → one `OsNotify`); `clear_agent_done` removes the marker when viewed.

### 6. `vmux_team`
- `plugin.rs`: add `Option<&AgentDoneUnseen>` to `agent_q` in both `emit_team`
  (line 184) and `build_team_members` (line 130); thread an `is_done_unseen`
  through `team_member_row` (line 66) and into the row (line 168).
- Test: an agent entity with `AgentDoneUnseen` yields a row with
  `is_done_unseen == true`; toggling re-broadcasts `TeamEvent`.

### 7. `vmux_layout` + `vmux_team` pages (the dot)
- `vmux_layout/src/page.rs` `TeamFacepile` (line 566): after the `is_running`
  emerald dot, add `else if m.is_done_unseen { span { class: "absolute
  -bottom-0.5 -right-0.5 size-2 rounded-full bg-amber-400 ring-2 ring-background
  animate-pulse" } }` (running takes precedence).
- `vmux_team/src/page.rs`: mirror at the row pill (line 107) and avatar dot
  (line 146) with amber `is_done_unseen` variants.

### 8. `vmux_desktop`
- `Cargo.toml`: add (macOS target) `objc2-user-notifications = { version = "0.3",
  features = ["UNUserNotificationCenter", "UNNotificationRequest",
  "UNMutableNotificationContent", "UNNotificationContent",
  "UNNotificationTrigger"] }` (`block2` is already present for the auth block).
- `src/notify.rs` (new), mirroring `screenshot.rs`'s cfg structure:
  - `request_notification_auth` (Startup, macOS): get
    `UNUserNotificationCenter::currentNotificationCenter()` and
    `requestAuthorizationWithOptions:completionHandler:` for `.Alert | .Sound`.
  - `post_os_notifications` (Update, reads `OsNotify`, macOS): build a
    `UNMutableNotificationContent` (title/body, default sound), wrap in a
    `UNNotificationRequest` with a fresh UUID identifier + nil trigger, and
    `addNotificationRequest:`. On non-macOS the system just drains the reader.
  - Guard: if `currentNotificationCenter()` is unavailable (unbundled dev),
    log once and skip — never panic. (Shipped app has bundle id `ai.vmux.desktop`,
    so it works there.)
- `src/lib.rs`: `mod notify;` and register the two systems (Startup + Update)
  next to the screenshot systems (line 139).

## Constants

- `DEDUP_WINDOW = Duration::from_secs(3)` — coalesces bell + MCP notify per turn.

## Error handling

- Bell for an unknown/non-agent `process_id` → ignored.
- MCP `Notify` with no resolvable caller → `AgentCommandResult::Error` → MCP error.
- `UNUserNotificationCenter` unavailable / authorization denied → log once, skip.
- Non-macOS → notify system no-ops.
- Agent finishes while foreground → intentionally nothing.

## Testing

Bevy message/system integration (per AGENTS.md — register types, send messages,
run schedules, assert ECS state/messages; no ad-hoc helper calls). Per-crate
unit tests as listed in each component above, plus:

- **Service bell mapping** (`vmux_service`): `ServiceEventProxy` given
  `TermEvent::Bell` enqueues `ServiceMessage::Bell { process_id }`; rkyv
  round-trips for `AgentCommand::Notify` and `ServiceMessage::Bell`.
- Run `cargo test -p vmux_layout` after editing `page.rs` (its source-scrape
  tests are sensitive to that file).
- Native `UNUserNotificationCenter` delivery is verified manually in the built
  app (needs bundle id + permission); it sits behind the `OsNotify` message
  boundary, so all gating/dedup logic is testable headlessly.

## Out of scope (v1)

- "Running" (green) dot for CLI agents — needs start-of-turn detection; this
  adds only the done/attention dot for CLI agents.
- Click-the-notification-to-focus-the-specific-page (needs a
  `UNUserNotificationCenterDelegate`); v1 clicking just activates the app.
- A deterministic auto-fire on Claude turn-end via a Stop hook → `vmux notify`
  CLI subcommand (a third channel). Natural follow-up if bell coverage proves
  insufficient.
- Aggregate "something is done" badge on the user pill; Linux notifications;
  notification sound/grouping customization.
