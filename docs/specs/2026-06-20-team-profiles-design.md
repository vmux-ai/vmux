# Team / Profiles — design

- Date: 2026-06-20
- Status: proposed
- Base: PR #112 `space-tab-ownership` (rebase onto `main` once #112 merges)

## Summary

`AppCommand` is dispatched by both the human and AI agents, but the bus has no
notion of *who* issued a command. This feature makes the actor a first-class
concept:

- Model every actor as an ECS entity carrying a shared `Profile` identity
  component plus a sibling **role** marker — `User` or `Agent`. Flat, no
  hierarchy.
- Attribute every issued command to a caller entity, and log it.
- Track an **active profile per space** (remembered; agent "takes over" on an
  MCP call, the user reclaims on a direct action).
- Render the active space's team as a Notion / Google-Docs style **facepile**
  (overlapping avatars) at the right of the header, and open `vmux://team` on
  click.

## Goals

- One uniform actor model: `Profile + User` and `Profile + Agent` are siblings.
- Each `AppCommand` is attributable to a caller and logged with it.
- The active profile is per-space state that survives space switches.
- A presence facepile on the header right; a `vmux://team` roster page.

## Non-goals

- Multiple human profiles. The model allows it (the `User` role is just a
  marker on a `Profile` entity), but we ship exactly one user profile.
- Multi-window / multi-device presence.
- Reworking how agents are spawned, run, or rendered.
- Renaming the legacy `Profile` types (see Naming).

## Background (current state)

- `AppCommand` enum: `crates/vmux_command/src/command.rs:17`. A Bevy `Message`
  broadcast bus: ~40 writer sites, many readers. Ordering sets
  `WriteAppCommands` → `WriteCommandBarSnapshots` → `ReadAppCommands`
  (`crates/vmux_command/src/plugin.rs`).
- Central logger `log_app_commands` (`crates/vmux_command/src/plugin.rs:32`)
  logs every command, with **no caller**.
- Agent → `AppCommand`: `handle_agent_commands`
  (`crates/vmux_agent/src/plugin.rs:323`) resolves MCP ids and
  `app_commands.write(command)`.
- The agent's identity **is available but dropped**:
  `ServiceMessage::AgentToolCall { request_id, sid, name, args_json }`
  (`crates/vmux_service/src/protocol.rs:516`) is forwarded to
  `AgentToolCallRequest` (`crates/vmux_service/src/agent_events.rs:18`) in
  `crates/vmux_terminal/src/plugin.rs` **discarding `sid`**.
- Not every `AgentCommandRequest` is agent-originated:
  `forward_history_open_intent` (`crates/vmux_agent/src/plugin.rs:867`) reuses
  it for a **user** action (history click). Attribution must follow the real
  channel, not the request type.
- Agent identity pieces: `AgentKind` (`crates/vmux_core/src/agent.rs:18`,
  Vibe/Claude/Codex, `display_name`, no color/icon), `AgentSession { kind }`
  (`agent.rs`), `SessionId(String)`, rich page-agent `AgentSession { kind,
  variant, sid, provider, model }`, `AgentSessionToEntity: (AgentKind, sid) ->
  Entity` (`crates/vmux_agent/src/session.rs:22`), `AgentRunState`
  (`crates/vmux_agent/src/run_state.rs`).
- Space (post-#112): a `Space` entity *owns* its tabs; `ActiveSpaceTag`
  (`crates/vmux_layout/src/space.rs:42`); `ActiveSpace` resource.
- Header UI: `vmux://header/` rendered by `crates/vmux_layout/src/page.rs`
  (`HeaderView`, tab row ~216). Bevy emits rkyv host events
  (`StacksHostEvent`, `TabsHostEvent`, …); Dioxus consumes via
  `use_bin_event_listener`; clicks emit command events back
  (`try_cef_bin_emit_rkyv`).
- `vmux_ui` already has an `Avatar` component
  (`crates/vmux_ui/src/components/avatar.rs`).
- A served page pattern exists (`vmux://space`, `crates/vmux_space/src/page.rs`,
  `PageManifest`).

## Model

Each actor is an entity:

```rust
// new crate: vmux_team
#[derive(Component)]
pub struct Profile {
    pub name: String,        // "You", "Claude", …
    pub avatar: AvatarSpec,  // color + initials now; image/logo later
}

#[derive(Component)]
pub struct User;             // role marker: the human

#[derive(Component)]
pub struct Agent {           // role marker: agent identity
    pub sid: String,
    pub kind: AgentKind,
}
```

- **User profile**: one entity `Profile + User`. Global (one human across all
  spaces). Spawned at startup.
- **Agent profile**: the existing agent entity gains `Profile + Agent`. We do
  not create a second identity entity — the agent's running entity *is* its
  profile. `Agent.sid` / `Agent.kind` mirror the session it already carries.
- Roster query: `&Profile` (+ `With<User>` / `With<Agent>` to discriminate).
- Siblings, never nested: nothing makes an `Agent` a kind of `User` or vice
  versa.

`AvatarSpec` (initials + color) keeps the agent avatar deterministic without a
logo asset; brand logos can replace it later without touching the model.

## Caller / attribution

A caller is the `Entity` of a `Profile`-bearing actor.

```rust
#[derive(Message)]
pub struct CommandIssued {
    pub caller: Entity,        // a Profile entity (User or Agent)
    pub command: AppCommand,
}
```

- `AppCommand` stays the executor bus — **readers unchanged**. `CommandIssued`
  is the attribution/log stream, emitted at the two real entry channels:
  1. **Agent (MCP)**: thread `sid` through
     `AgentToolCall.sid → AgentToolCallRequest → AgentCommandRequest` and add a
     `caller` origin to `AgentCommandRequest`. In `handle_agent_commands`,
     resolve `sid → Agent` entity (via `AgentSessionToEntity`) and emit
     `CommandIssued { caller: agent_entity, command }`. The history-intent reuse
     sets `caller = user_entity` (it is a user action).
  2. **User**: at the user entry points (keyboard `shortcut.rs`,
     `native_keyboard.rs`, OS menu `os_menu.rs`, command bar
     `command_bar/handler.rs`, header/tab/side-sheet host-command handlers,
     in-terminal `cmd+w` `terminal/plugin.rs`) emit
     `CommandIssued { caller: user_entity, command }`.
- **Internal cascades** (e.g. `pane.rs`/`tab.rs`/`browser` re-dispatching
  `PaneCommand::Close`, context-menu `Open`) keep writing plain `AppCommand`.
  They are *effects*, not calls — not re-attributed.
- Logging: `log_app_commands` reads `CommandIssued` and logs caller (resolved to
  `Profile.name` + role) and command. Keep a plain-`AppCommand` debug log too if
  useful, but the attributed line is the headline.

Helper to reduce boilerplate at entry points: an extension
`commands_issue(caller, cmd)` that writes both `CommandIssued` and `AppCommand`
in one call. Entry points use it instead of raw `AppCommand` writes.

Rejected alternatives:
- A `caller` field on `AppCommand` itself — invasive (40+ sites, derive macros,
  `Copy`/`Eq`).
- Deriving the caller from the per-space active profile at log time — couples
  logging to UI state and mis-tags async/interleaved agent commands.

## Per-space active profile

```rust
#[derive(Component)]
pub struct ActiveProfile(pub Entity);   // on the Space entity
```

- Default → the `User` entity.
- An `update_active_profile` system reads `CommandIssued`, finds the command's
  space (the caller's space for agents; the active space for the user), and sets
  that space's `ActiveProfile = caller`. Net effect: user action reclaims; agent
  MCP call is a takeover; remembered per space and restored on space switch.
- Explicit override: the `vmux://team` page can set `ActiveProfile` (and focus
  that profile's page).
- If the referenced agent exits (`AgentSessionExited`), revert that space's
  `ActiveProfile` to `User`.
- Independent of "active": any `Agent` whose `AgentRunState` is running gets a
  **pulse**, even when not the active profile.

## Right-side facepile (header)

Presence cluster in the header top row, right-aligned after the `flex-1` tab
strip (`page.rs` `HeaderView`).

Data flow (mirrors existing host events):
- New rkyv event `TeamEvent { members: Vec<TeamMemberRow> }` where
  `TeamMemberRow { id, name, initials, color, role: User|Agent, is_active,
  is_running }`, scoped to the **active space** (global user + this space's
  agents). Define the event + const in `crates/vmux_layout/src/event.rs`.
- A Bevy system builds `TeamEvent` from `Profile`/`User`/`Agent` +
  `ActiveProfile` + `AgentRunState` and emits it to the header webview.
- Dioxus `Page` adds a `use_bin_event_listener::<TeamEvent>` and renders a
  `TeamFacepile` at the right of the tab row.

Facepile (Notion / Google-Docs style):
- Overlapping circular avatars: horizontal row, negative spacing
  (`-space-x-2`), each `ring-2 ring-background` so overlaps read as separate
  discs; later avatars layered over earlier (`z`/DOM order).
- Active member: `ring-primary` (highlight). Running agent: pulse ring or a
  small status dot.
- Overflow: show up to N (e.g. 4–5); collapse the rest into a `+k` chip.
- Reuse `vmux_ui` `Avatar` (`AvatarImageSize::Small`, `AvatarShape::Circle`)
  with initials fallback + `color`.
- Click anywhere on the cluster → emit a host command event opening
  `vmux://team` (new `TeamCommandEvent`, analogous to `HeaderCommandEvent`).

## `vmux://team` page

New served Dioxus page, mirroring `vmux://space`
(`crates/vmux_space/src/page.rs` + `PageManifest`):
- Lists the active space's team: the user profile + agents (siblings, grouped or
  flat with role chips).
- Clicking a member sets `ActiveProfile` and focuses that member's page/stack
  (user → last user page; agent → its page). Behavior reuses existing
  focus/open command paths.
- Shows run state for agents (idle/running/errored).

## Naming & collisions

Two unrelated `Profile`s already exist:
- `vmux_core::profile` — build/data-dir profile (release/dev/local), path
  helpers. Untouched.
- `vmux_layout::profile::Profile { name }` — persisted per-space **account
  label** ("Personal"), shown in the command bar. A different concept (space
  grouping, not a team member). Untouched.

The new member-identity type lives in a new `vmux_team` crate as
`vmux_team::Profile`. Same word, different crate/path — accepted to match the
user-facing term ("active profile", "team"). No legacy renames (the persisted
one is `#[type_path]`-stable; #112's `store.version` schema guard would hard
reset on an incompatible change anyway).

## Persistence

- The `User` profile (name/avatar) may persist via the existing
  moonshine-save flow; agents are runtime-only (their `Profile` is derived from
  the live session, never saved).
- `ActiveProfile` is **not** persisted: it points at live entities (agents die
  on restart). It is in-session per-space state, defaulting to `User`.

## Build order (one PR on top of #112)

1. **Model + attribution + logging**: `vmux_team` crate (`Profile`, `User`,
   `Agent`, `AvatarSpec`); spawn the user profile; add `Profile + Agent` to
   agent entities; thread `sid`; `CommandIssued` + `commands_issue` helper at
   entry points; attributed `log_app_commands`.
2. **Per-space active**: `ActiveProfile` on `Space`; `update_active_profile`;
   revert-on-exit.
3. **Facepile**: `TeamEvent`/`TeamCommandEvent`; emit system; Dioxus
   `TeamFacepile` in the header right; click opens `vmux://team`.
4. **Team page**: `vmux://team` roster + set-active + jump-to-page.

## Testing

- Native unit tests (`cargo test -p vmux_team`, `-p vmux_command`,
  `-p vmux_agent`):
  - `sid` survives `AgentToolCall → AgentCommandRequest`.
  - MCP command → `CommandIssued { caller = Agent(sid) }`; history-intent →
    `caller = User`; a user shortcut → `caller = User`.
  - `update_active_profile`: agent MCP takeover, user reclaim, revert on agent
    exit; per-space isolation (action in space A doesn't change space B).
  - `TeamEvent` membership = user + active space's agents only; `is_active` /
    `is_running` flags correct.
- Source-scrape tests for header `page.rs` text if the existing
  `tests/page_source.rs` pattern applies (see memory: refactors there break
  `include_str!` asserts).
- Runtime: facepile renders, active ring + running pulse update, click opens
  `vmux://team`, attribution lines appear in logs. (User runtime-tests.)

## Open questions

- Agent avatar visual: initials+color now; swap to brand logos later — OK?
- Facepile overflow cap N and order (user pinned left or right?).
