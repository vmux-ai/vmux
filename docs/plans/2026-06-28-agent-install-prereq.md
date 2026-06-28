# Agent Install Prerequisite (Homebrew auto-setup) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a user installs a Homebrew-based agent (claude/codex) and Homebrew is missing, install Homebrew first transparently, then the agent — one button press — and surface a Retry on failure instead of spinning forever.

**Architecture:** Native (Bevy) owns brew detection + execution; the WASM Dioxus setup page is a dumb renderer that subscribes to native-pushed state over the rkyv bin-event bridge. The install runs as one cross-shell-safe `bash -c '…'` chain in a terminal pane. Completion is detected from the OSC-133 `CommandLifecycle` event the service already emits (currently dropped by the desktop).

**Tech Stack:** Rust, Bevy ECS, bevy_cef (CEF IPC bridge), Dioxus (WASM), rkyv.

**Worktree:** `.worktrees/agent-install-prereq` (branch `feat/agent-install-prereq`). All paths below are relative to that worktree root. Spec: `docs/specs/2026-06-28-agent-install-prereq-homebrew-design.md`.

**Build note:** CEF builds are heavy. Keep a warm `target/` and use `cargo check`/targeted `cargo test` during the loop. Do NOT subagent-drive the native build steps. Defer the runtime/manual test to the single pass in Task 9.

---

### Task 1: `vmux_core::agent_setup` — prerequisite-aware command builder

**Files:**
- Modify: `crates/vmux_core/src/agent_setup.rs`

- [ ] **Step 1: Write the failing tests**

Append these tests inside the existing `mod tests` block in `crates/vmux_core/src/agent_setup.rs` (before the closing `}`):

```rust
    #[test]
    fn requires_homebrew_only_for_cask_agents() {
        assert!(requires_homebrew("claude"));
        assert!(requires_homebrew("codex"));
        assert!(!requires_homebrew("vibe"));
        assert!(!requires_homebrew("nope"));
    }

    #[test]
    fn chained_command_prepends_homebrew_when_absent() {
        assert_eq!(
            install_command_chained("claude", false).as_deref(),
            Some(
                "bash -c 'NONINTERACTIVE=1 /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\" && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && brew install --cask claude-code'"
            )
        );
        assert_eq!(
            install_command_chained("codex", false).as_deref(),
            Some(
                "bash -c 'NONINTERACTIVE=1 /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\" && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && brew install --cask codex'"
            )
        );
    }

    #[test]
    fn chained_command_plain_when_brew_present() {
        assert_eq!(
            install_command_chained("claude", true).as_deref(),
            Some("brew install --cask claude-code")
        );
    }

    #[test]
    fn chained_command_never_wraps_vibe() {
        let absent = install_command_chained("vibe", false);
        let present = install_command_chained("vibe", true);
        assert_eq!(absent, present);
        assert_eq!(
            absent.as_deref(),
            Some("curl -LsSf https://mistral.ai/vibe/install.sh | bash")
        );
    }

    #[test]
    fn chained_command_unknown_is_none() {
        assert_eq!(install_command_chained("nope", false), None);
        assert_eq!(install_command_chained("nope", true), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p vmux_core agent_setup`
Expected: FAIL — `cannot find function requires_homebrew` / `install_command_chained` in this scope.

- [ ] **Step 3: Implement the functions**

Add to `crates/vmux_core/src/agent_setup.rs` after `install_command` (before `#[cfg(test)]`):

```rust
/// True for agents installed via Homebrew casks (`claude`, `codex`).
pub fn requires_homebrew(segment: &str) -> bool {
    matches!(segment, "claude" | "codex")
}

/// The official Homebrew installer one-liner, run non-interactively.
///
/// `NONINTERACTIVE=1` skips the installer's "Press RETURN" prompt; `sudo` still
/// prompts for a password once on a fresh machine.
pub fn homebrew_install_command() -> &'static str {
    "NONINTERACTIVE=1 /bin/bash -c \"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
}

/// The command vmux runs in the terminal to install `segment`.
///
/// When the agent needs Homebrew (`claude`/`codex`) and it is absent
/// (`brew_present == false`), the command first installs Homebrew, loads it onto
/// `PATH` for the session, then installs the agent — wrapped in `bash -c '…'` so
/// it runs verbatim under nushell, zsh, or bash. Otherwise the plain per-agent
/// command is returned unchanged. Returns `None` for unknown segments.
pub fn install_command_chained(segment: &str, brew_present: bool) -> Option<String> {
    let base = install_command(segment)?;
    if requires_homebrew(segment) && !brew_present {
        Some(format!(
            "bash -c '{} && eval \"$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)\" && {base}'",
            homebrew_install_command()
        ))
    } else {
        Some(base.to_string())
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p vmux_core agent_setup`
Expected: PASS (all tests incl. the existing `known_segments_resolve`, `unknown_segment_is_none`).

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_core/src/agent_setup.rs
git commit -m "feat(agent-setup): prerequisite-aware install command builder"
```

---

### Task 2: Bridge event types for the setup page

**Files:**
- Modify: `crates/vmux_agent/src/vibe/setup/event.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/vmux_agent/src/vibe/setup/event.rs`:

```rust
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn prereq_status_rkyv_roundtrip() {
        let v = AgentSetupPrereqStatus {
            needs_homebrew: true,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<AgentSetupPrereqStatus, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(back.needs_homebrew);
    }

    #[test]
    fn result_rkyv_roundtrip() {
        let v = AgentSetupResult { ok: false };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&v).unwrap();
        let back = rkyv::from_bytes::<AgentSetupResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(!back.ok);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent setup::event`
Expected: FAIL — `cannot find type AgentSetupPrereqStatus` / `AgentSetupResult`.

- [ ] **Step 3: Implement the types**

Add to `crates/vmux_agent/src/vibe/setup/event.rs` after `AgentInstallRunRequest`:

```rust
/// Bin-event id for native → page prerequisite status pushes.
pub const AGENT_SETUP_PREREQ_EVENT: &str = "agent_setup_prereq";

/// Bin-event id for native → page install-result pushes.
pub const AGENT_SETUP_RESULT_EVENT: &str = "agent_setup_result";

/// Page → native: asks whether `agent` needs a prerequisite installed first.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentSetupPrereqRequest {
    pub agent: String,
}

/// Native → page: whether Homebrew must be installed before the agent.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentSetupPrereqStatus {
    pub needs_homebrew: bool,
}

/// Native → page: terminal install finished. `ok == false` drives the Retry state.
#[derive(
    Clone,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentSetupResult {
    pub ok: bool,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p vmux_agent setup::event`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_agent/src/vibe/setup/event.rs
git commit -m "feat(agent-setup): bridge event types for prereq + result"
```

---

### Task 3: Forward OSC-133 command completion to the desktop

The service already emits `ServiceMessage::CommandLifecycle`; `poll_service_messages` drops it at `_ => {}`. Add a typed desktop message and forward it.

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs`

- [ ] **Step 1: Add the message type**

In `crates/vmux_terminal/src/plugin.rs`, after `ProcessExitedEvent` (currently ends at line ~3146), add:

```rust
#[derive(Message, Debug, Clone)]
pub struct CommandLifecycleEvent {
    pub process_id: ProcessId,
    pub kind: vmux_service::protocol::CommandLifecycleKind,
}
```

- [ ] **Step 2: Register the message**

In `add_terminal_update_systems` (line ~360), add the `.add_message` call to the existing chain:

```rust
fn add_terminal_update_systems(app: &mut App) -> &mut App {
    app.add_message::<ProcessExitedEvent>()
        .add_message::<CommandLifecycleEvent>()
        .add_message::<OscTitleChanged>()
```

- [ ] **Step 3: Add the writer to `PollServiceWriters`**

In `struct PollServiceWriters<'w>` (line ~1034), add after `process_exited`:

```rust
    process_exited: MessageWriter<'w, ProcessExitedEvent>,
    command_lifecycle: MessageWriter<'w, CommandLifecycleEvent>,
```

- [ ] **Step 4: Fill the dropped match arm**

In `poll_service_messages`, replace the final `_ => {}` (line ~1509) with:

```rust
            ServiceMessage::CommandLifecycle { process_id, kind } => {
                writers
                    .command_lifecycle
                    .write(CommandLifecycleEvent { process_id, kind });
            }
            _ => {}
```

- [ ] **Step 5: Verify it compiles (no unit test — pure forwarding arm with no logic, mirrors the adjacent `ProcessExited` arm; behavior is exercised by Task 7's test which writes `CommandLifecycleEvent` directly)**

Run: `cargo check -p vmux_terminal`
Expected: compiles clean.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): forward OSC-133 CommandLifecycle to desktop"
```

---

### Task 4: Terminal re-input request (for Retry)

Lets native code re-type a command into an existing live terminal by `ProcessId`, reusing the `PendingTerminalInput` flush path.

**Files:**
- Modify: `crates/vmux_terminal/src/plugin.rs`

- [ ] **Step 1: Add the message type**

After `CommandLifecycleEvent` (from Task 3) add:

```rust
#[derive(Message, Debug, Clone)]
pub struct TerminalReinputRequest {
    pub process_id: ProcessId,
    pub data: Vec<u8>,
}
```

- [ ] **Step 2: Add the handler system**

Add near `flush_pending_terminal_input` (after it, ~line 1536):

```rust
fn handle_terminal_reinput_requests(
    mut requests: MessageReader<TerminalReinputRequest>,
    terminals: Query<(Entity, &ProcessId), With<Terminal>>,
    mut commands: Commands,
) {
    for req in requests.read() {
        for (entity, pid) in &terminals {
            if *pid == req.process_id {
                commands.entity(entity).insert(PendingTerminalInput {
                    data: req.data.clone(),
                });
            }
        }
    }
}
```

- [ ] **Step 3: Register message + system**

In `add_terminal_update_systems`, add `.add_message::<TerminalReinputRequest>()` to the message chain, and register the system after `poll_service_messages` (so the freshly-inserted `PendingTerminalInput` is picked up by `flush_pending_terminal_input` the same or next frame). Add to the `.add_systems(Update, …)` group:

```rust
        .add_systems(
            Update,
            handle_terminal_reinput_requests.after(poll_service_messages),
        )
```

- [ ] **Step 4: Verify it compiles (no unit test — the system is a `ProcessId`-match + component insert; its effect is verified end-to-end by the manual Retry test in Task 9)**

Run: `cargo check -p vmux_terminal`
Expected: compiles clean.

- [ ] **Step 5: Commit**

```bash
git add crates/vmux_terminal/src/plugin.rs
git commit -m "feat(terminal): re-input request to retype into a live terminal"
```

---

### Task 5: Prereq detection observer (native)

**Files:**
- Modify: `crates/vmux_agent/src/vibe/setup.rs`

- [ ] **Step 1: Write the failing test**

Add a `tests` module at the end of `crates/vmux_agent/src/vibe/setup.rs` (this file is `#[cfg(not(target_arch = "wasm32"))]` for the native items; the test is native):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prereq_needs_homebrew_logic() {
        if cfg!(target_os = "macos") {
            assert!(prereq_needs_homebrew("claude", false));
            assert!(prereq_needs_homebrew("codex", false));
            assert!(!prereq_needs_homebrew("claude", true));
        } else {
            assert!(!prereq_needs_homebrew("claude", false));
        }
        assert!(!prereq_needs_homebrew("vibe", false));
        assert!(!prereq_needs_homebrew("nope", false));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent setup::tests::prereq_needs_homebrew_logic`
Expected: FAIL — `cannot find function prereq_needs_homebrew`.

- [ ] **Step 3: Implement the helper + observer**

In `crates/vmux_agent/src/vibe/setup.rs`, update the imports. Replace:

```rust
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};
```

with:

```rust
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};
```

and replace:

```rust
use crate::vibe::setup::event::AgentInstallRunRequest;
```

with:

```rust
use crate::vibe::setup::event::{
    AgentInstallRunRequest, AgentSetupPrereqRequest, AgentSetupPrereqStatus, AgentSetupResult,
    AGENT_SETUP_PREREQ_EVENT, AGENT_SETUP_RESULT_EVENT,
};
```

Add the helper + observer (anywhere among the `#[cfg(not(target_arch = "wasm32"))]` fns):

```rust
/// Homebrew is needed first only on macOS, only for cask agents, and only when
/// `brew` is not already resolvable.
#[cfg(not(target_arch = "wasm32"))]
fn prereq_needs_homebrew(segment: &str, brew_present: bool) -> bool {
    cfg!(target_os = "macos")
        && vmux_core::agent_setup::requires_homebrew(segment)
        && !brew_present
}

#[cfg(not(target_arch = "wasm32"))]
fn on_agent_setup_prereq_request(
    trigger: On<BinReceive<AgentSetupPrereqRequest>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let segment = &trigger.event().payload.agent;
    let brew_present = crate::exec::find_executable("brew").is_some();
    let needs_homebrew = prereq_needs_homebrew(segment, brew_present);
    if browsers.has_browser(webview) && browsers.host_emit_ready(&webview) {
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            AGENT_SETUP_PREREQ_EVENT,
            &AgentSetupPrereqStatus { needs_homebrew },
        ));
    }
}
```

> Note: `AgentSetupResult` and `AGENT_SETUP_RESULT_EVENT` are imported here but first used in Task 7 — keep the import; the unused-import warning disappears once Task 7 lands. If you implement strictly task-by-task and CI denies warnings, temporarily add only `AgentSetupPrereqRequest, AgentSetupPrereqStatus, AGENT_SETUP_PREREQ_EVENT` now and extend the import in Task 7.

- [ ] **Step 4: Register the observer + emitter type**

In `AgentSetupPlugin::build`, change the plugin/observer registration to:

```rust
        app.add_plugins(
            BinEventEmitterPlugin::<(AgentInstallRunRequest, AgentSetupPrereqRequest)>::for_hosts(
                &["agent"],
            ),
        )
        .add_observer(on_agent_install_run)
        .add_observer(on_agent_setup_prereq_request)
        .add_systems(Update, auto_redirect_agent_setup_when_installed);
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_agent setup::tests::prereq_needs_homebrew_logic`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/vmux_agent/src/vibe/setup.rs
git commit -m "feat(agent-setup): detect missing Homebrew and push prereq status"
```

---

### Task 6: Chained install command + install-pane state + Retry reuse

**Files:**
- Modify: `crates/vmux_agent/src/vibe/setup.rs`

- [ ] **Step 1: Extend `AgentInstallPane`**

Replace the existing component:

```rust
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct AgentInstallPane {
    setup_stack: Entity,
}
```

with:

```rust
#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct AgentInstallPane {
    setup_stack: Entity,
    setup_webview: Entity,
    agent: vmux_core::agent::AgentKind,
    process_id: vmux_service::protocol::ProcessId,
    armed: bool,
}
```

- [ ] **Step 2: Rewrite `on_agent_install_run` (chained command + retry reuse)**

Replace the whole `on_agent_install_run` fn with:

```rust
#[cfg(not(target_arch = "wasm32"))]
fn on_agent_install_run(
    trigger: On<BinReceive<AgentInstallRunRequest>>,
    focus: Res<vmux_layout::stack::FocusedStack>,
    ctx: vmux_layout::pane::PlacementCtx,
    mut install_panes: Query<&mut AgentInstallPane>,
    mut commands: Commands,
    mut spawn: MessageWriter<vmux_terminal::TerminalStackSpawnRequest>,
    mut run: MessageWriter<vmux_terminal::RunShellRequest>,
    mut reinput: MessageWriter<vmux_terminal::TerminalReinputRequest>,
) {
    let webview = trigger.event().webview;
    let segment = &trigger.event().payload.agent;
    let Some(kind) = vmux_core::agent::AgentKind::from_url_segment(segment) else {
        warn!("agent install run: unknown agent segment '{segment}'");
        return;
    };
    let brew_present = crate::exec::find_executable("brew").is_some();
    let Some(command) = vmux_core::agent_setup::install_command_chained(segment, brew_present) else {
        warn!("agent install run: unknown agent segment '{segment}'");
        return;
    };
    let input = vmux_terminal::shell_input::shell_command_input(&command);

    // Retry: an install pane for this page already exists — re-type into it.
    for mut pane in &mut install_panes {
        if pane.setup_webview == webview {
            reinput.write(vmux_terminal::TerminalReinputRequest {
                process_id: pane.process_id,
                data: input.clone(),
            });
            pane.armed = false;
            return;
        }
    }

    let (Some(pane), Some(setup_stack)) = (focus.pane, focus.stack) else {
        run_install_in_new_tab(&mut run, &command);
        return;
    };
    if !ctx.leaf_panes.contains(pane) {
        run_install_in_new_tab(&mut run, &command);
        return;
    }
    let existing_tabs: Vec<Entity> = ctx
        .pane_children
        .get(pane)
        .map(|c| c.iter().filter(|&e| ctx.tab_filter.contains(e)).collect())
        .unwrap_or_default();
    let already_split = ctx.split_dir_q.contains(pane);
    let install_pane = vmux_layout::pane::split_or_extend(
        &mut commands,
        pane,
        vmux_layout::pane::PaneSplitDirection::Row,
        &existing_tabs,
        true,
        already_split,
    );
    let process_id = vmux_service::protocol::ProcessId::new();
    commands.entity(install_pane).insert(AgentInstallPane {
        setup_stack,
        setup_webview: webview,
        agent: kind,
        process_id,
        armed: false,
    });
    spawn.write(vmux_terminal::TerminalStackSpawnRequest {
        pane: install_pane,
        cwd: None,
        pending_input: Some(input),
        process_id: Some(process_id),
        activate: true,
    });
}
```

Also update `run_install_in_new_tab` to take `&str` (it already does) — call sites now pass `&command`; no signature change needed.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p vmux_agent`
Expected: compiles clean (note: `on_agent_install_run` is registered in Task 5; the new `TerminalReinputRequest` import resolves via `vmux_terminal::`).

- [ ] **Step 4: Commit**

```bash
git add crates/vmux_agent/src/vibe/setup.rs
git commit -m "feat(agent-setup): chained Homebrew+agent install with retry reuse"
```

---

### Task 7: Install-outcome handler (success vs Retry)

**Files:**
- Modify: `crates/vmux_agent/src/vibe/setup.rs`

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` in `crates/vmux_agent/src/vibe/setup.rs`:

```rust
    #[test]
    fn install_outcome_gates_on_armed_and_presence() {
        // Not armed: ignore (spurious pre-command prompt completion).
        assert_eq!(install_outcome(false, true), None);
        assert_eq!(install_outcome(false, false), None);
        // Armed + binary present => success.
        assert_eq!(install_outcome(true, true), Some(true));
        // Armed + binary absent => failure.
        assert_eq!(install_outcome(true, false), Some(false));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p vmux_agent setup::tests::install_outcome_gates_on_armed_and_presence`
Expected: FAIL — `cannot find function install_outcome`.

- [ ] **Step 3: Implement helper + system**

Add to `crates/vmux_agent/src/vibe/setup.rs`:

```rust
/// Decide an install pane's outcome from a completed command.
///
/// `None` while not yet `armed` (ignores the shell's spurious pre-command
/// completion). Once armed: `Some(true)` when the agent binary is present
/// (success), `Some(false)` when still absent (failure → Retry).
#[cfg(not(target_arch = "wasm32"))]
fn install_outcome(armed: bool, installed: bool) -> Option<bool> {
    if !armed {
        return None;
    }
    Some(installed)
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_agent_install_outcome(
    mut events: MessageReader<vmux_terminal::CommandLifecycleEvent>,
    mut install_panes: Query<&mut AgentInstallPane>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    use vmux_service::protocol::CommandLifecycleKind;
    for ev in events.read() {
        for mut pane in &mut install_panes {
            if pane.process_id != ev.process_id {
                continue;
            }
            match ev.kind {
                CommandLifecycleKind::Started => pane.armed = true,
                CommandLifecycleKind::Ended { .. } => {
                    let installed = crate::exec::find_executable(pane.agent.executable()).is_some();
                    match install_outcome(pane.armed, installed) {
                        Some(false) => {
                            if browsers.has_browser(pane.setup_webview)
                                && browsers.host_emit_ready(&pane.setup_webview)
                            {
                                commands.trigger(BinHostEmitEvent::from_rkyv(
                                    pane.setup_webview,
                                    AGENT_SETUP_RESULT_EVENT,
                                    &AgentSetupResult { ok: false },
                                ));
                            }
                            pane.armed = false;
                        }
                        // Success is handled by `auto_redirect_agent_setup_when_installed`.
                        Some(true) | None => {}
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: Register the system**

In `AgentSetupPlugin::build`, extend the chain to add the system:

```rust
        .add_observer(on_agent_install_run)
        .add_observer(on_agent_setup_prereq_request)
        .add_systems(Update, auto_redirect_agent_setup_when_installed)
        .add_systems(Update, detect_agent_install_outcome);
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p vmux_agent setup::tests::install_outcome_gates_on_armed_and_presence`
Expected: PASS.

- [ ] **Step 6: Verify the crate compiles**

Run: `cargo check -p vmux_agent`
Expected: compiles clean — all imports from Task 5 (`AgentSetupResult`, `AGENT_SETUP_RESULT_EVENT`, `BinHostEmitEvent`, `Browsers`) are now used.

- [ ] **Step 7: Commit**

```bash
git add crates/vmux_agent/src/vibe/setup.rs
git commit -m "feat(agent-setup): detect install completion, surface Retry on failure"
```

---

### Task 8: Setup page — prereq + Retry rendering

**Files:**
- Modify: `crates/vmux_agent/src/vibe/setup/page.rs`

- [ ] **Step 1: Replace the page module**

Overwrite `crates/vmux_agent/src/vibe/setup/page.rs` with:

```rust
#![allow(non_snake_case)]

use crate::vibe::setup::event::{
    AgentInstallRunRequest, AgentSetupPrereqRequest, AgentSetupPrereqStatus, AgentSetupResult,
    AGENT_SETUP_PREREQ_EVENT, AGENT_SETUP_RESULT_EVENT,
};
use dioxus::prelude::*;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

fn current_agent_segment() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .and_then(|path| path.split('/').find(|s| !s.is_empty()).map(str::to_string))
        .filter(|seg| vmux_core::agent_setup::display_name(seg).is_some())
        .unwrap_or_else(|| "vibe".to_string())
}

fn tagline(segment: &str) -> &'static str {
    match segment {
        "claude" => "Anthropic's coding agent, in vmux",
        "codex" => "OpenAI's coding agent, in vmux",
        _ => "Mistral's coding agent, in vmux",
    }
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let segment = current_agent_segment();
    let name = vmux_core::agent_setup::display_name(&segment).unwrap_or("Vibe");
    let command = vmux_core::agent_setup::install_command(&segment).unwrap_or_default();
    let brew_command = vmux_core::agent_setup::homebrew_install_command();
    let tagline = tagline(&segment);
    let accent = agent_accent(&segment);

    let mut installing = use_signal(|| false);
    let mut needs_homebrew = use_signal(|| false);
    let mut failed = use_signal(|| false);

    let _prereq =
        use_bin_event_listener::<AgentSetupPrereqStatus, _>(AGENT_SETUP_PREREQ_EVENT, move |s| {
            needs_homebrew.set(s.needs_homebrew);
        });
    let _result =
        use_bin_event_listener::<AgentSetupResult, _>(AGENT_SETUP_RESULT_EVENT, move |r| {
            if !r.ok {
                installing.set(false);
                failed.set(true);
            }
        });

    {
        let seg = segment.clone();
        use_effect(move || {
            let _ = try_cef_bin_emit_rkyv(&AgentSetupPrereqRequest { agent: seg.clone() });
        });
    }

    let prompt_class = format!("select-none font-mono text-sm {}", accent.accent_text);
    let cta_base = format!(
        "group inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-br {} px-4 py-2.5 text-sm font-medium text-white {} transition-all hover:brightness-110 active:scale-[0.99]",
        accent.grad, accent.cta_shadow
    );
    let cta_full = if installing() {
        format!("{cta_base} pointer-events-none opacity-70")
    } else {
        cta_base
    };

    let emit_segment = segment.clone();
    rsx! {
        main { class: "relative flex min-h-screen items-center justify-center overflow-hidden bg-background p-10 text-foreground",
            div { class: "{accent.glow_top}" }
            div { class: "{accent.glow_bottom}" }

            section { class: "relative w-full max-w-lg rounded-3xl bg-white/[0.04] p-8 ring-1 ring-inset ring-white/10 backdrop-blur-2xl shadow-[0_24px_80px_-24px_rgba(0,0,0,0.7)]",
                div { class: "mb-6 flex items-center gap-4",
                    div { class: "flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-white/[0.06] ring-1 ring-inset ring-white/10",
                        Favicon {
                            favicon_url: "".to_string(),
                            url: format!("vmux://agent/{segment}/cli/"),
                            class: "h-7 w-7 shrink-0 rounded-lg object-contain".to_string(),
                            globe_class: "h-7 w-7 text-muted-foreground".to_string(),
                        }
                    }
                    div { class: "min-w-0",
                        h1 { class: "text-xl font-semibold leading-tight tracking-tight", "Install {name} CLI" }
                        p { class: "text-sm text-muted-foreground", "{tagline}" }
                    }
                }

                if needs_homebrew() {
                    p { class: "mb-5 text-sm leading-relaxed text-muted-foreground",
                        "Homebrew is required to install "
                        code { class: "rounded bg-white/10 px-1.5 py-0.5 font-mono text-[0.8em] text-foreground", "{segment}" }
                        " and isn't set up yet. vmux will install Homebrew first, then {name}."
                    }
                    div { class: "mb-2 flex items-center gap-3 rounded-xl bg-black/40 p-4 ring-1 ring-inset ring-white/10",
                        span { class: "{prompt_class}", "1" }
                        code { class: "min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-sm text-foreground", "{brew_command}" }
                    }
                    div { class: "mb-3 flex items-center gap-3 rounded-xl bg-black/40 p-4 ring-1 ring-inset ring-white/10",
                        span { class: "{prompt_class}", "2" }
                        code { class: "min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-sm text-foreground", "{command}" }
                    }
                    p { class: "mb-5 text-xs text-muted-foreground/70",
                        "You'll be asked for your Mac password once during the Homebrew install."
                    }
                } else {
                    p { class: "mb-5 text-sm leading-relaxed text-muted-foreground",
                        "vmux opened this page because the local "
                        code { class: "rounded bg-white/10 px-1.5 py-0.5 font-mono text-[0.8em] text-foreground", "{segment}" }
                        " command isn't installed yet. Run the command below to get it."
                    }
                    div { class: "mb-5 flex items-center gap-3 rounded-xl bg-black/40 p-4 ring-1 ring-inset ring-white/10",
                        span { class: "{prompt_class}", "$" }
                        code { class: "min-w-0 flex-1 overflow-x-auto whitespace-nowrap font-mono text-sm text-foreground", "{command}" }
                    }
                }

                if failed() {
                    p { class: "mb-3 rounded-xl bg-red-500/10 px-4 py-3 text-sm text-red-300 ring-1 ring-inset ring-red-500/20",
                        "Install didn't finish. Check the terminal for details, then retry."
                    }
                }

                button {
                    class: "{cta_full}",
                    disabled: installing(),
                    onclick: move |_| {
                        installing.set(true);
                        failed.set(false);
                        let _ = try_cef_bin_emit_rkyv(&AgentInstallRunRequest { agent: emit_segment.clone() });
                    },
                    if installing() {
                        span { class: "h-4 w-4 shrink-0 animate-spin rounded-full border-2 border-white/40 border-t-white" }
                        "Installing…"
                    } else if failed() {
                        Icon { class: "h-4 w-4",
                            path { d: "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" }
                            path { d: "M3 3v5h5" }
                        }
                        "Retry"
                    } else if needs_homebrew() {
                        Icon { class: "h-4 w-4",
                            path { d: "M5 12h14" }
                            path { d: "m12 5 7 7-7 7" }
                        }
                        "Install Homebrew + {name}"
                    } else {
                        Icon { class: "h-4 w-4",
                            path { d: "M5 12h14" }
                            path { d: "m12 5 7 7-7 7" }
                        }
                        "Run install command"
                    }
                }

                p { class: "mt-3 text-center text-xs text-muted-foreground/70",
                    "vmux runs it in a terminal and reloads when "
                    code { class: "font-mono", "{segment}" }
                    " is ready."
                }
            }
        }
    }
}
```

- [ ] **Step 2: Typecheck the WASM page**

Run: `cargo check --target wasm32-unknown-unknown -p vmux_agent`
Expected: compiles clean.

> If the target is missing: `rustup target add wasm32-unknown-unknown`.

- [ ] **Step 3: Commit**

```bash
git add crates/vmux_agent/src/vibe/setup/page.rs
git commit -m "feat(agent-setup): transparent Homebrew prereq + Retry on the setup page"
```

---

### Task 9: Full verification + manual runtime pass

**Files:** none (verification only)

- [ ] **Step 1: Workspace build + lint + tests**

Run:
```bash
cargo fmt --all
git checkout -- patches/   # cargo fmt also reformats vendored patches; keep only crates/ changes
cargo clippy --workspace --all-targets
cargo test --workspace
```
Expected: fmt clean (after restoring `patches/`), clippy clean, all tests pass. Fix any failure before proceeding. (If `cargo test --workspace` mis-authors later commits, see the repo's known pre-push notes; commit any fmt fixups now.)

- [ ] **Step 2: WASM page typecheck**

Run: `cargo check --target wasm32-unknown-unknown -p vmux_agent`
Expected: clean.

- [ ] **Step 3: Manual runtime test (single pass — user-run)**

On a Mac **without** Homebrew (or temporarily shadow `brew` off PATH):
1. Open the claude agent (command bar → "Claude", or `vmux://agent/claude/`).
2. Setup page shows "Homebrew is required…", two numbered steps, and the password note. Button reads "Install Homebrew + Claude".
3. Click → terminal pane splits and runs the chain; enter the Mac password once when prompted.
4. On success: page auto-redirects to the Claude CLI; install pane closes.
5. Failure path: re-open setup, click install, cancel the `sudo` prompt (Ctrl-C) → page shows the red "Install didn't finish" banner and a "Retry" button; the failed terminal stays visible. Click Retry → the command re-runs in the same terminal.
6. Regression: `vibe` setup page is unchanged (single `$` command, "Run install command"); installing vibe still works.

- [ ] **Step 4: Delete the plan file (per AGENTS.md: remove once fully implemented)**

```bash
git rm docs/plans/2026-06-28-agent-install-prereq.md
git commit -m "chore: remove implemented agent-install-prereq plan"
```

- [ ] **Step 5: Open the PR**

Use the open-new-pr flow (`gh pr create`, return the URL).

---

## Self-Review

**Spec coverage:**
- Prereq model (`requires_homebrew`, `homebrew_install_command`, `install_command_chained`) → Task 1. ✓
- Bridge events (`AgentSetupPrereqRequest/Status/Result` + ids) → Task 2. ✓
- `CommandLifecycleEvent` + filled dropped arm → Task 3. ✓
- Retry re-input mechanism → Task 4 (terminal) + Task 6 (caller). ✓
- Native prereq detection + push → Task 5. ✓
- Chained command in `on_agent_install_run` + `AgentInstallPane` fields + pinned `ProcessId` → Task 6. ✓
- Completion → success-redirect (reuses `auto_redirect…`) or `AgentSetupResult{ok:false}` + armed gate → Task 7. ✓
- Page: listeners, mount request, transparent prereq variant, Retry, vibe unchanged → Task 8. ✓
- macOS-only gate → `prereq_needs_homebrew` (`cfg!(target_os = "macos")`) Task 5; cask install inherently macOS. ✓
- Testing (core unit tests, outcome/prereq pure-fn tests, workspace + wasm checks, manual pass) → Tasks 1,2,5,7,9. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code; commands have expected output. Tasks 3/4 have no unit test by design (pure forwarding/insert with no branching logic) — stated explicitly with the rationale and the compile gate, not a hidden gap.

**Type consistency:** `AgentInstallPane { setup_stack, setup_webview, agent, process_id, armed }` defined in Task 6, consumed in Task 7. `install_command_chained(segment, brew_present)` defined Task 1, used Task 6. `CommandLifecycleEvent { process_id, kind }` defined Task 3, read Task 7. `TerminalReinputRequest { process_id, data }` defined Task 4, written Task 6. `AgentSetupPrereqStatus { needs_homebrew }` / `AgentSetupResult { ok }` defined Task 2, emitted Tasks 5/7, consumed Task 8. Event-id consts `AGENT_SETUP_PREREQ_EVENT` / `AGENT_SETUP_RESULT_EVENT` consistent across Tasks 2/5/7/8. `install_outcome(armed, installed)` defined+tested Task 7. `prereq_needs_homebrew(segment, brew_present)` defined+tested Task 5. ✓
