# Team / Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the actor (human "profile" vs each live "agent") a first-class, flat ECS concept; attribute and log every `AppCommand` to its caller; track a per-space active profile; and render the active space's team as an overlapping-avatar facepile in the header that opens `vmux://team`.

**Architecture:** A new `vmux_team` crate defines `Profile` (shared identity) plus sibling role markers `User` / `Agent`. A `CommandIssued { caller: Entity, command }` message carries attribution from the two real entry channels (user input; the agent MCP path with `sid` threaded through). `AppCommand` stays the executor bus (readers unchanged). Per-space `ActiveProfile(Entity)` lives on the `Space` entity. The header (Dioxus, "layout" host) gains a `TeamEvent`-fed facepile; `vmux://team` is a new served host mirroring `vmux_space`.

**Tech Stack:** Rust, Bevy 0.19-rc.2 ECS, bevy_cef (rkyv host events), Dioxus (wasm pages), moonshine-save (persistence).

**Spec:** `docs/specs/2026-06-20-team-profiles-design.md`

**Base:** branch `feat/team-profiles` (off `main` @ #112 merge). Worktree `.worktrees/team-profiles`.

**Conventions:**
- No code comments (AGENTS.md). No `Co-Authored-By`/`Generated` commit trailers (global CLAUDE.md).
- No `mod.rs`; use `foo.rs` + `foo/` filename modules.
- Chain consecutive `App` builder calls in one expression.
- Run targeted tests during the loop: `cargo test -p <crate> <name>`.
- All edits happen in the worktree; paths below are repo-relative to it.

---

## File map

**Create**
- `crates/vmux_team/Cargo.toml` — new crate.
- `crates/vmux_team/src/lib.rs` — exports + `PAGE_MANIFEST` + `web` re-exports.
- `crates/vmux_team/src/profile.rs` — `Profile`, `User`, `Agent`, `AvatarSpec`.
- `crates/vmux_team/src/plugin.rs` — `TeamPlugin` (spawn user profile, attach agent profiles, active-profile systems, team-event emit, team page open/broadcast).
- `crates/vmux_team/src/page.rs` — `vmux://team` Dioxus page (wasm32).
- `crates/vmux_team/src/event.rs` — `TeamEvent`, `TeamMemberRow`, `TEAM_EVENT`, `TeamCommandEvent` (shared host/web types).

**Modify**
- `Cargo.toml` (workspace) — nothing (members = `crates/*` auto-includes).
- `crates/vmux_command/src/command.rs` or new `crates/vmux_command/src/issued.rs` — `CommandIssued` + `IssueCommand` helper.
- `crates/vmux_command/src/plugin.rs:13,32` — register `CommandIssued`; rewrite `log_app_commands`.
- `crates/vmux_service/src/agent_events.rs:18` — add `sid` to `AgentToolCallRequest`.
- `crates/vmux_terminal/src/plugin.rs` (~1314) — forward `sid`.
- `crates/vmux_agent/src/events.rs` + `crates/vmux_service/src/agent_events.rs:6` — add `origin` to `AgentCommandRequest`.
- `crates/vmux_agent/src/plugin.rs:288,323,867` — set origin; emit `CommandIssued`; add `Profile+Agent` on spawn; revert-on-exit.
- `crates/vmux_layout/src/space.rs` — `ActiveProfile` component + default.
- User entry points (emit `CommandIssued`): `crates/vmux_desktop/src/shortcut.rs:85`, `native_keyboard.rs:232`, `os_menu.rs:426,448`, `crates/vmux_layout/src/command_bar/handler.rs:1055`, `crates/vmux_layout/src/tab.rs:301` (TabsCommand handler), header/side-sheet command handlers, `crates/vmux_terminal/src/plugin.rs:1223`.
- `crates/vmux_layout/src/page.rs` — facepile listener + `TeamFacepile` component.
- `crates/vmux_server/src/lib.rs:41` — `web_pages!` add `render_team: "team" => vmux_team::page::Page`.
- `crates/vmux_desktop/src/lib.rs` (plugin assembly) — add `TeamPlugin`.

---

## PHASE 1 — Model + attribution + logging

### Task 1: `vmux_team` crate with the profile model

**Files:**
- Create: `crates/vmux_team/Cargo.toml`, `crates/vmux_team/src/lib.rs`, `crates/vmux_team/src/profile.rs`

- [ ] **Step 1: Write `Cargo.toml`**

Mirror a small native+wasm crate (compare `crates/vmux_space/Cargo.toml` for the exact dep versions/workspace refs and the `cfg(target_arch=wasm32)` split). Minimum:

```toml
[package]
name = "vmux_team"
version.workspace = true
edition.workspace = true

[dependencies]
bevy = { workspace = true }
vmux_core = { path = "../vmux_core" }
serde = { workspace = true }
rkyv = { workspace = true }

[target.'cfg(target_arch = "wasm32")'.dependencies]
dioxus = { workspace = true }
vmux_ui = { path = "../vmux_ui" }
```

(Confirm each dep line against `vmux_space/Cargo.toml`; add `bevy`/`vmux_layout`/`vmux_command`/`vmux_agent`/`bevy_cef` to deps as later tasks require — start minimal, extend per task.)

- [ ] **Step 2: Write `profile.rs` (failing test first)**

```rust
use bevy::prelude::*;
use vmux_core::agent::AgentKind;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AvatarSpec {
    pub initials: String,
    pub color: String,
}

#[derive(Component, Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub avatar: AvatarSpec,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct User;

#[derive(Component, Clone, Debug)]
pub struct Agent {
    pub sid: String,
    pub kind: AgentKind,
}

impl AvatarSpec {
    pub fn for_user() -> Self {
        Self { initials: "You".into(), color: "#3b82f6".into() }
    }
    pub fn for_agent(kind: AgentKind) -> Self {
        let (initials, color) = match kind {
            AgentKind::Claude => ("CL", "#d97757"),
            AgentKind::Codex => ("CX", "#10a37f"),
            AgentKind::Vibe => ("VB", "#7c3aed"),
        };
        Self { initials: initials.into(), color: color.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_avatar_is_kind_specific() {
        assert_eq!(AvatarSpec::for_agent(AgentKind::Claude).initials, "CL");
        assert_ne!(
            AvatarSpec::for_agent(AgentKind::Codex).color,
            AvatarSpec::for_agent(AgentKind::Vibe).color
        );
    }
}
```

- [ ] **Step 3: Write `lib.rs`**

```rust
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod profile;
pub use profile::{Agent, AvatarSpec, Profile, User};

#[cfg(not(target_arch = "wasm32"))]
pub mod plugin;
#[cfg(not(target_arch = "wasm32"))]
pub use plugin::TeamPlugin;
```

- [ ] **Step 4: Run** `cargo test -p vmux_team agent_avatar_is_kind_specific` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_team
git commit -m "feat(team): add Profile/User/Agent model crate"
```

### Task 2: spawn the user profile via `TeamPlugin`

**Files:** Create `crates/vmux_team/src/plugin.rs`

- [ ] **Step 1: Failing test**

```rust
// in plugin.rs
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    #[test]
    fn user_profile_spawned_at_startup() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TeamPlugin));
        app.world_mut().run_schedule(Startup);
        let mut q = app.world_mut().query_filtered::<&Profile, With<User>>();
        let profiles: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "You");
    }
}
```

- [ ] **Step 2: Implement**

```rust
use bevy::prelude::*;
use crate::profile::{AvatarSpec, Profile, User};

pub struct TeamPlugin;

impl Plugin for TeamPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_user_profile);
    }
}

fn spawn_user_profile(mut commands: Commands) {
    commands.spawn((
        Profile { name: "You".into(), avatar: AvatarSpec::for_user() },
        User,
        Name::new("Profile: You"),
    ));
}
```

- [ ] **Step 3: Run** `cargo test -p vmux_team user_profile_spawned_at_startup` → PASS.
- [ ] **Step 4: Commit** `git commit -am "feat(team): spawn the user profile at startup"`

### Task 3: `CommandIssued` message + `IssueCommand` helper

**Files:** Create `crates/vmux_command/src/issued.rs`; Modify `crates/vmux_command/src/lib.rs`, `crates/vmux_command/src/plugin.rs:13`

Add `vmux_command` dep: none new (it already owns `AppCommand`).

- [ ] **Step 1: Failing test** (`issued.rs`)

```rust
use bevy::prelude::*;
use crate::command::AppCommand;

#[derive(Message, Clone)]
pub struct CommandIssued {
    pub caller: Entity,
    pub command: AppCommand,
}

pub trait IssueCommand {
    fn issue(&mut self, caller: Entity, command: AppCommand);
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct CommandIssuer<'w> {
    pub app: MessageWriter<'w, AppCommand>,
    pub issued: MessageWriter<'w, CommandIssued>,
}

impl IssueCommand for CommandIssuer<'_> {
    fn issue(&mut self, caller: Entity, command: AppCommand) {
        self.issued.write(CommandIssued { caller, command: command.clone() });
        self.app.write(command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{AppCommand, TerminalCommand};

    #[test]
    fn issue_writes_both_buses() {
        let mut app = App::new();
        app.add_message::<AppCommand>().add_message::<CommandIssued>();
        let caller = app.world_mut().spawn_empty().id();
        let mut sys = bevy::ecs::system::SystemState::<CommandIssuer>::new(app.world_mut());
        {
            let mut issuer = sys.get_mut(app.world_mut());
            issuer.issue(caller, AppCommand::Terminal(TerminalCommand::Clear));
        }
        sys.apply(app.world_mut());
        let app_msgs = app.world().resource::<Messages<AppCommand>>();
        let issued = app.world().resource::<Messages<CommandIssued>>();
        assert_eq!(app_msgs.len(), 1);
        assert_eq!(issued.len(), 1);
    }
}
```

- [ ] **Step 2: Wire into `lib.rs`** — add `pub mod issued; pub use issued::{CommandIssued, CommandIssuer, IssueCommand};`
- [ ] **Step 3: Register message** in `crates/vmux_command/src/plugin.rs` build (chain): add `.add_message::<CommandIssued>()` next to `.add_message::<AppCommand>()` (line 13).
- [ ] **Step 4: Run** `cargo test -p vmux_command issue_writes_both_buses` → PASS.
- [ ] **Step 5: Commit** `git commit -am "feat(command): add CommandIssued attribution bus + IssueCommand"`

### Task 4: thread agent `sid` to the dispatch site

**Files:** `crates/vmux_service/src/agent_events.rs:6,18`; `crates/vmux_terminal/src/plugin.rs` (~1314); `crates/vmux_agent/src/plugin.rs:288`

- [ ] **Step 1: Add `sid` to `AgentToolCallRequest`** (`agent_events.rs:18`)

```rust
#[derive(Message)]
pub struct AgentToolCallRequest {
    pub request_id: AgentRequestId,
    pub sid: String,
    pub name: String,
    pub args_json: String,
}
```

- [ ] **Step 2: Add `origin` to `AgentCommandRequest`** (`agent_events.rs:6`)

```rust
#[derive(Clone)]
pub enum CommandOrigin {
    User,
    Agent { sid: String },
}

#[derive(Message)]
pub struct AgentCommandRequest {
    pub request_id: AgentRequestId,
    pub origin: CommandOrigin,
    pub command: AgentCommand,
}
```

(Re-export `CommandOrigin` where `AgentCommandRequest` is re-exported: `crates/vmux_agent/src/events.rs:5`.)

- [ ] **Step 3: Stop dropping `sid`** in `crates/vmux_terminal/src/plugin.rs` `ServiceMessage::AgentToolCall` arm (~1314): capture `sid` and pass it into `AgentToolCallRequest { request_id, sid, name, args_json }`.

- [ ] **Step 4: Pass origin in `handle_agent_tool_calls`** (`crates/vmux_agent/src/plugin.rs:288`): build `AgentCommandRequest { request_id: req.request_id, origin: CommandOrigin::Agent { sid: req.sid.clone() }, command }` (and the query branch unchanged). For `forward_history_open_intent` (`plugin.rs:867`) set `origin: CommandOrigin::User`.

- [ ] **Step 5: Fix all `AgentCommandRequest { .. }` constructors** to include `origin` (compiler will list them; the only user-origin one is `forward_history_open_intent`).

- [ ] **Step 6: Build** `cargo build -p vmux_agent -p vmux_terminal -p vmux_service` → compiles.
- [ ] **Step 7: Commit** `git commit -am "feat(agent): thread agent sid + command origin to dispatch"`

### Task 5: emit `CommandIssued` from the agent path

**Files:** `crates/vmux_agent/src/plugin.rs:323` (`handle_agent_commands`); add deps `vmux_team`, ensure `vmux_command::CommandIssued` available.

- [ ] **Step 1: Failing test** (`vmux_agent` tests) — drive an `AgentCommandRequest{ origin: Agent{sid:"s1"} , command: AppCommand-bearing }` through `handle_agent_commands` with a spawned `Profile+Agent{sid:"s1"}` entity, assert a `CommandIssued { caller = that entity }` is emitted.

```rust
#[test]
fn agent_appcommand_attributes_to_agent_profile() {
    use vmux_command::CommandIssued;
    use vmux_team::{Agent, Profile, AvatarSpec};
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<vmux_command::AppCommand>()
        .add_message::<vmux_command::CommandIssued>()
        // + the other messages handle_agent_commands needs (see existing test at plugin.rs:1694 for the full list)
        ;
    let agent = app.world_mut().spawn((
        Profile { name: "Claude".into(), avatar: AvatarSpec::for_agent(vmux_core::agent::AgentKind::Claude) },
        Agent { sid: "s1".into(), kind: vmux_core::agent::AgentKind::Claude },
    )).id();
    // resource mapping sid -> entity (see Step 3)
    // write AgentCommandRequest{ origin: Agent{ sid:"s1" }, command: AppCommand id "terminal_clear" }
    // app.update();
    let issued: Vec<_> = app.world().resource::<Messages<CommandIssued>>().iter_current_update_events().collect();
    assert!(issued.iter().any(|m| m.caller == agent));
}
```

(Use the existing big test at `crates/vmux_agent/src/plugin.rs:1694` as the template for the message-registration boilerplate.)

- [ ] **Step 2: Resolve `sid → Agent` entity.** Add a query `agents: Query<(Entity, &vmux_team::Agent)>` to `handle_agent_commands`; build a lookup or `.iter().find(|(_, a)| a.sid == sid)`. Also fetch the single `User` entity: `user: Query<Entity, With<vmux_team::User>>`.

- [ ] **Step 3: Replace `app_commands.write(command)`** in the `ServiceAgentCommand::AppCommand` arm with attribution:

```rust
let caller = match &request.origin {
    CommandOrigin::Agent { sid } => agents.iter().find(|(_, a)| &a.sid == sid).map(|(e, _)| e),
    CommandOrigin::User => user.single().ok(),
}.unwrap_or(Entity::PLACEHOLDER);
issued.write(CommandIssued { caller, command: command.clone() });
app_commands.write(command);
```

(`issued: MessageWriter<CommandIssued>` added to the system params. Keep `Entity::PLACEHOLDER` fallback only if neither resolves; log a warn.)

- [ ] **Step 4: Run** `cargo test -p vmux_agent agent_appcommand_attributes_to_agent_profile` → PASS.
- [ ] **Step 5: Commit** `git commit -am "feat(agent): attribute agent AppCommands to their profile"`

### Task 6: attach `Profile+Agent` when an agent spawns; revert needs it

**Files:** `crates/vmux_agent/src/plugin.rs` (`handle_spawn_agent_requests` ~1294; `attach_page_agent_to_stack` ~224)

- [ ] **Step 1: Failing test** — spawn a CLI agent (reuse `deep_link_focuses_existing_claude_tab` setup at `plugin.rs:1628`), assert the agent entity has `vmux_team::Agent { sid, kind }` + `Profile`.

- [ ] **Step 2: Implement (CLI)** in `handle_spawn_agent_requests`, after inserting `AgentSession`/`process_id`, also insert when a session id is known:

```rust
let sid = req.session_id.clone().unwrap_or_default();
commands.entity(terminal).insert((
    vmux_team::Profile {
        name: req.kind.display_name().to_string(),
        avatar: vmux_team::AvatarSpec::for_agent(req.kind),
    },
    vmux_team::Agent { sid, kind: req.kind },
));
```

For `PendingAgentSession` (no sid yet), insert `Profile`+`Agent{ sid: String::new(), kind }` and backfill `sid` where `SessionId` is later inserted (`crates/vmux_agent/src/session.rs` `track_session_id_inserts`): when `SessionId` is added, set `Agent.sid`.

- [ ] **Step 3: Implement (Page agent)** in `attach_page_agent_to_stack` add `Profile`+`Agent{ sid, kind }` alongside `components::AgentSession`.

- [ ] **Step 4: Run** the new test → PASS. `cargo test -p vmux_agent` → green.
- [ ] **Step 5: Commit** `git commit -am "feat(agent): give agent entities a Profile identity"`

### Task 7: attributed logging

**Files:** `crates/vmux_command/src/plugin.rs:32`

- [ ] **Step 1: Update test** (`plugin.rs` source-scrape test at :41) to expect reading `CommandIssued` and logging caller.

- [ ] **Step 2: Rewrite `log_app_commands`** to read `CommandIssued`, resolve `caller` → `Profile` (name + `User`/`Agent` role) and log:

```rust
fn log_app_commands(
    mut reader: MessageReader<crate::issued::CommandIssued>,
    profiles: Query<(&vmux_team::Profile, Has<vmux_team::User>)>,
) {
    for ev in reader.read() {
        let who = profiles.get(ev.caller)
            .map(|(p, is_user)| format!("{} ({})", p.name, if is_user { "user" } else { "agent" }))
            .unwrap_or_else(|_| "unknown".into());
        info!(target: "vmux_command::app_command", caller = %who, cmd = ?ev.command, "AppCommand");
    }
}
```

Add `vmux_team` dep to `vmux_command`. Keep the system ordered after `WriteAppCommands` (it now reads `CommandIssued`, also written in that set).

- [ ] **Step 3: Run** `cargo test -p vmux_command` → PASS.
- [ ] **Step 4: Commit** `git commit -am "feat(command): log AppCommand caller (user vs agent)"`

### Task 8: emit `CommandIssued` from user entry points

**Files (one commit per site or grouped):** `crates/vmux_desktop/src/shortcut.rs:85`, `native_keyboard.rs:232`, `os_menu.rs:426,448`; `crates/vmux_layout/src/command_bar/handler.rs:1055`; `crates/vmux_layout/src/tab.rs:301`; `crates/vmux_terminal/src/plugin.rs:1223`; header/side-sheet command observers.

Each currently does `writer.write(AppCommand::…)`. Swap to the helper so attribution is captured.

- [ ] **Step 1:** In each system, replace `MessageWriter<AppCommand>` with `vmux_command::CommandIssuer` and add a `user: Query<Entity, With<vmux_team::User>>` param; resolve `let caller = user.single().unwrap_or(Entity::PLACEHOLDER);`. Replace `writer.write(cmd)` with `issuer.issue(caller, cmd)`.
- [ ] **Step 2:** For observer-style handlers (`On<BinReceive<…>>`) that currently use `Commands`/a writer, obtain the writers via the trigger's world access pattern already used in that file; emit both `CommandIssued` and `AppCommand`.
- [ ] **Step 3:** Build the affected crates; run `cargo test -p vmux_desktop -p vmux_layout -p vmux_terminal` (targeted).
- [ ] **Step 4: Commit** `git commit -am "feat: attribute user-issued AppCommands to the user profile"`

> Note: internal cascade re-dispatches (`pane.rs`, `tab.rs` open re-emits, `browser` context menu) keep raw `AppCommand::write` — they are effects, not calls, and are intentionally not attributed.

---

## PHASE 2 — Per-space active profile

### Task 9: `ActiveProfile` component on `Space`

**Files:** `crates/vmux_layout/src/space.rs`

- [ ] **Step 1: Failing test** — spawn a `Space`, run the ensure-system, assert it has `ActiveProfile(user_entity)`.
- [ ] **Step 2: Implement**

```rust
#[derive(Component, Clone, Copy, Debug)]
pub struct ActiveProfile(pub Entity);
```

Add an `ensure_active_profile` system (in `TeamPlugin`, Update): for each `Space` `Without<ActiveProfile>`, insert `ActiveProfile(user_entity)` (resolve the single `User`). Register `ActiveProfile` for reflection only if it must persist — it must NOT (points at live entities); leave it unregistered/unsaved.
- [ ] **Step 3: Run** test → PASS. **Commit** `git commit -am "feat(team): per-space ActiveProfile defaulting to user"`

### Task 10: update active profile on `CommandIssued`

**Files:** `crates/vmux_team/src/plugin.rs`

- [ ] **Step 1: Failing tests** (per-space isolation + takeover + reclaim):
  - agent MCP `CommandIssued` whose caller is an `Agent` in space A → A's `ActiveProfile` = that agent; space B unchanged.
  - subsequent user `CommandIssued` → active space's `ActiveProfile` = user.
- [ ] **Step 2: Implement `update_active_profile`** reading `CommandIssued`:
  - Resolve the caller's space: for an `Agent` caller, walk `ChildOf` up to the owning `Space` (post-#112 a stack/terminal is under a `Space`); for a `User` caller, use the active space (`ActiveSpaceTag`).
  - Set that `Space`'s `ActiveProfile(caller)`.
- [ ] **Step 3: Run** tests → PASS. **Commit** `git commit -am "feat(team): active profile follows caller per space"`

### Task 11: revert to user on agent exit

**Files:** `crates/vmux_team/src/plugin.rs` (listen to `vmux_agent::AgentSessionExited` or `RemovedComponents<Agent>`)

- [ ] **Step 1: Failing test** — space `ActiveProfile = agent`; despawn/exit that agent; assert reverts to user.
- [ ] **Step 2: Implement** `revert_active_profile_on_agent_exit`: on `AgentSessionExited`/removed `Agent`, any `Space` whose `ActiveProfile` points at the gone entity → reset to the `User` entity.
- [ ] **Step 3: Run** → PASS. **Commit** `git commit -am "feat(team): revert active profile to user when agent exits"`

---

## PHASE 3 — Header facepile

### Task 12: `TeamEvent` / `TeamCommandEvent` shared types

**Files:** Create `crates/vmux_team/src/event.rs`; export from `lib.rs` (no `cfg`, shared host+web).

- [ ] **Step 1: Define** (mirror derives from `crates/vmux_layout/src/event.rs:257` `StacksHostEvent`):

```rust
pub const TEAM_EVENT: &str = "team";

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TeamEvent { pub members: Vec<TeamMemberRow> }

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TeamMemberRow {
    pub id: String,        // entity bits as string, for click round-trip
    pub name: String,
    pub initials: String,
    pub color: String,
    pub is_user: bool,
    pub is_active: bool,
    pub is_running: bool,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize,
         rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TeamCommandEvent { pub command: String, #[serde(default)] pub member_id: Option<String> }
```

- [ ] **Step 2:** `cargo build -p vmux_team` (native + `--target wasm32-unknown-unknown` if available) → compiles.
- [ ] **Step 3: Commit** `git commit -am "feat(team): rkyv host/command event types"`

### Task 13: emit `TeamEvent` to the header

**Files:** `crates/vmux_team/src/plugin.rs` (native). Add deps `bevy_cef`, `vmux_browser` types as needed; mirror the emit at `crates/vmux_browser/src/lib.rs:2074-2086`.

- [ ] **Step 1: Implement `push_team_emit`** (system): resolve the layout CEF entity + `host_emit_ready` (mirror `push_pane_tree_emit` signature at `lib.rs:2098`), build `members` from: the single `User` `Profile`, plus `Agent` `Profile`s whose owning `Space` is the active space; set `is_active` from that space's `ActiveProfile`, `is_running` from `vmux_agent::AgentRunState`. Dedupe via `Local<String>` RON; `commands.trigger(BinHostEmitEvent::from_rkyv(cef_e, TEAM_EVENT, &payload))`.
- [ ] **Step 2:** Register the system in `TeamPlugin` (Update). Build → compiles.
- [ ] **Step 3:** Unit-test the builder: factor membership into a pure fn `build_team_rows(...) -> Vec<TeamMemberRow>` and test active/running/scoping flags without CEF.
- [ ] **Step 4: Commit** `git commit -am "feat(team): emit active-space team to the header"`

### Task 14: receive `TeamCommandEvent` → open `vmux://team`

**Files:** `crates/vmux_team/src/plugin.rs`; register on the "layout" host.

- [ ] **Step 1:** In `TeamPlugin`, `add_plugins(BinEventEmitterPlugin::<(TeamCommandEvent,)>::for_hosts(&["layout"]))` and `add_observer(on_team_command)` — mirror `crates/vmux_layout/src/tab.rs:301` (`On<BinReceive<TabsCommandEvent>>`).
- [ ] **Step 2:** `on_team_command` writes a `PageOpenRequest` for `vmux://team/` (mirror how the spaces page is opened; see `vmux_space` `handle_spaces_page_open`/`SpacesPageSpawnRequest`). For now any click opens the page.
- [ ] **Step 3:** Build → compiles. **Commit** `git commit -am "feat(team): open vmux://team from header"`

### Task 15: render the facepile in the header

**Files:** `crates/vmux_layout/src/page.rs`

- [ ] **Step 1:** Add a listener (mirror `tabs_state` at `page.rs:33`): `use_bin_event_listener::<vmux_team::event::TeamEvent, _>(vmux_team::event::TEAM_EVENT, …)`. (Add `vmux_team` to `vmux_layout` deps.)
- [ ] **Step 2:** In `HeaderView`'s tab row (`page.rs:216`), after the `flex-1` tabs `div`, add `TeamFacepile { members }`.
- [ ] **Step 3:** Implement `TeamFacepile` (Notion/GDrive overlap): a row `class: "flex items-center -space-x-2 pl-2"`; for each member render a circular avatar disc `class: "relative inline-flex size-7 items-center justify-center rounded-full ring-2 ring-background text-[11px] font-medium"` with `style: "background:{color}"`, ring `ring-primary` when `is_active`, `animate-pulse` when `is_running`; show `initials`. Cap at 5; overflow `+k` disc. Whole cluster `onclick` → `try_cef_bin_emit_rkyv(&TeamCommandEvent { command: "open".into(), member_id: None })`.
- [ ] **Step 4:** Source-scrape test (mirror existing `page.rs`/`tests/page_source.rs` pattern, see memory) asserting `-space-x-2` and `TEAM_EVENT` usage present.
- [ ] **Step 5:** `cargo test -p vmux_layout` → PASS (this catches the `include_str!` text asserts). **Commit** `git commit -am "feat(layout): header facepile of the active-space team"`

---

## PHASE 4 — `vmux://team` page

### Task 16: served page scaffold (mirror `vmux_space`)

**Files:** Create `crates/vmux_team/src/page.rs` (wasm32); Modify `crates/vmux_team/src/lib.rs` (add `PAGE_MANIFEST`, `#[cfg(wasm32)] pub mod page`); `crates/vmux_server/src/lib.rs:41`.

- [ ] **Step 1:** Add to `lib.rs`:

```rust
#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "team",
    title: "Team",
    keywords: &["team", "agents", "profile"],
    icon: "users",
    command_bar: true,
};

#[cfg(target_arch = "wasm32")]
pub mod page;
```

- [ ] **Step 2:** `web_pages!` (`crates/vmux_server/src/lib.rs:41`): add `render_team: "team" => vmux_team::page::Page,` and add `vmux_team` to `vmux_server` deps.
- [ ] **Step 3:** `crates/vmux_team/src/page.rs`: a Dioxus `Page` that listens to `TeamEvent` and lists members (reuse `vmux_ui` `Avatar`); each row `onclick` emits `TeamCommandEvent { command: "activate", member_id: Some(id) }`. Mirror `crates/vmux_space/src/page.rs` structure.
- [ ] **Step 4:** `TeamPlugin`: `app.world_mut().spawn(crate::PAGE_MANIFEST);` and a `handle_team_page_open` in `PageOpenSet::HandleKnownPages` + broadcast `TeamEvent` to the "team" host (mirror `vmux_space` `handle_spaces_page_open` + `broadcast_spaces_to_views`).
- [ ] **Step 5:** Build the web bundle path used in dev (`make dev` rebuilds pages) — verify the host compiles via `cargo check -p vmux_server` and the wasm page via the project's page-build step.
- [ ] **Step 6: Commit** `git commit -am "feat(team): vmux://team served roster page"`

### Task 17: activate + jump from the team page

**Files:** `crates/vmux_team/src/plugin.rs` (`on_team_command` extend)

- [ ] **Step 1: Failing test** — `TeamCommandEvent{ command:"activate", member_id: Some(agent_id) }` sets the active space `ActiveProfile` to that entity.
- [ ] **Step 2:** Extend `on_team_command`: parse `member_id` → `Entity`; set active space `ActiveProfile`; if it's an `Agent`, focus its page/stack (reuse `vmux_terminal::pid::focus_pane_entity` / existing focus command); if `User`, focus last user stack.
- [ ] **Step 3:** Run test → PASS. **Commit** `git commit -am "feat(team): activate and jump to a profile from the team page"`

---

## PHASE 5 — Wire-up & verification

### Task 18: register `TeamPlugin` in the app

**Files:** `crates/vmux_desktop/src/lib.rs` (plugin assembly — find where `AgentPlugin`/`SpacePlugin` are added).

- [ ] **Step 1:** Add `vmux_team::TeamPlugin` to the desktop plugin group, after `SpacePlugin`/`AgentPlugin` (so `User`/`Agent` entities + spaces exist). Add `vmux_team` to `vmux_desktop` deps.
- [ ] **Step 2:** `cargo build -p vmux_desktop` → compiles.
- [ ] **Step 3: Commit** `git commit -am "feat(desktop): register TeamPlugin"`

### Task 19: full checks + runtime verification

- [ ] **Step 1:** `cargo fmt --all` ; `cargo clippy --workspace --all-targets -- -D warnings` ; `cargo test --workspace`. Fix failures.
- [ ] **Step 2: Runtime (user-driven, per memory: do not self-launch `make dev` unbounded):** the user runs `make dev` and verifies:
  - Header shows a facepile at the right: `You` + active-space agents; overlap + ring styling.
  - Driving an agent (it calls an MCP tool) flips the active ring to that agent; a running agent pulses; user action reclaims.
  - Switching spaces swaps the roster and restores each space's active profile.
  - Clicking the facepile opens `vmux://team`; activating a member there focuses its page.
  - Logs show `AppCommand caller=You (user)` / `caller=Claude (agent)` lines.
- [ ] **Step 3:** Address review (CodeRabbit + human) before merge (AGENTS.md). **Open PR** via `gh pr create` (per memory: direct, not `-w`).

---

## Self-review notes

- **Spec coverage:** model (T1–2,6), attribution+log (T3–5,7,8), per-space active (T9–11), facepile (T12–15), team page (T16–17), wire-up (T18–19). All spec sections mapped.
- **Type consistency:** `Profile{name,avatar}`, `Agent{sid,kind}`, `User`, `AvatarSpec{initials,color}`, `CommandIssued{caller,command}`, `CommandOrigin::{User,Agent{sid}}`, `ActiveProfile(Entity)`, `TeamEvent{members}`, `TeamMemberRow{id,name,initials,color,is_user,is_active,is_running}`, `TeamCommandEvent{command,member_id}` used consistently across tasks.
- **Open risks to confirm at execution:** exact `vmux_space/Cargo.toml` dep lines (T1); CEF emit-system param list (mirror `push_pane_tree_emit`, T13); wasm page-build wiring for a new host (T16 Step 5); whether `ActiveProfile` space resolution via `ChildOf` matches the #112 hierarchy (T10).
