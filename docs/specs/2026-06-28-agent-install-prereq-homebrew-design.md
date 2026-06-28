# Agent Install Prerequisite Handling (Homebrew auto-setup)

Date: 2026-06-28
Status: Approved (design)
Scope: macOS only

## Problem

When a user opens an agent that isn't installed, vmux shows the setup page
(`vmux://agent/<seg>/setup`) and, on click, types the agent's install command into
a terminal pane (`crates/vmux_agent/src/vibe/setup.rs::on_agent_install_run`).

- `vibe` → `curl -LsSf https://mistral.ai/vibe/install.sh | bash` (installs `uv`, then
  `uv tool install mistral-vibe`). Needs only `curl` + `bash` — always present on macOS.
- `claude` → `brew install --cask claude-code` — **requires Homebrew**.
- `codex` → `brew install --cask codex` — **requires Homebrew**.

There is **no prerequisite check**. If Homebrew is absent, the shell prints
`command not found: brew` inside the terminal pane and
`auto_redirect_agent_setup_when_installed` polls forever for a binary that will never
appear. The user is stuck on the setup page with no error surfaced. This is friction at
the exact moment a new user is trying to get into an agent.

## Goal

Eliminate that friction: when an agent requires Homebrew and Homebrew is missing, install
Homebrew first (transparently), then the agent, in one button press — and if the install
fails or is cancelled, show an actionable error instead of spinning forever.

## Non-goals

- Installing `curl`/`bash` for `vibe` (always present on macOS; can't bootstrap `curl`
  without a package manager anyway — chicken/egg). `vibe` behavior is unchanged.
- Linux. Homebrew casks are macOS-only; the brew prereq path is macOS-gated. Linux keeps
  current behavior.
- Fully silent Homebrew install. The official installer requires a `sudo` password (and on
  a fresh Mac, may install the Xcode Command Line Tools). We run it in the visible terminal
  pane so the user can press Return and enter their password. We do NOT set
  `NONINTERACTIVE=1` — that mode refuses to prompt for `sudo` and aborts with "Need sudo
  access" when credentials aren't already cached (confirmed in runtime testing).

## User experience (Transparent step)

For `claude`/`codex` when Homebrew is missing:

1. Setup page renders a prereq variant: header "Homebrew required — we'll install it
   first, then {Name}", the two steps shown, and a note "you'll be asked for your Mac
   password once". Button label: "Install Homebrew + {Name}".
2. On click, a terminal pane runs: Homebrew installer → `brew shellenv` (PATH for this
   session) → `brew install --cask <pkg>`.
3. On success, the existing auto-redirect navigates the stack to the agent and closes the
   install pane.
4. On failure/cancel, the page flips to "Install didn't finish — Retry"; the failed
   terminal stays visible so the user can read the error. Retry re-runs the install in the
   same pane.

When Homebrew is already present (or for `vibe`), the page and flow are unchanged from
today.

## Architecture

Native owns detection and execution (it has process + filesystem access); the WASM page is
a dumb renderer of state pushed over the bin-event bridge. Mirrors the LSP/Mason manager
page wiring (`crates/vmux_editor/src/lsp_page.rs` ↔ `lsp/manager_page.rs`).

```text
page (WASM)                         native (Bevy)
-----------                         -------------
on mount ── AgentSetupPrereqRequest ──▶ detect brew (exec::find_executable)
        ◀── AgentSetupPrereqStatus ──── { needs_homebrew }
render prereq variant

click ──── AgentInstallRunRequest ────▶ on_agent_install_run:
                                          cmd = install_command_chained(seg, brew_present)
                                          spawn terminal pane (pinned ProcessId,
                                                               tag AgentInstallPane)
                                        terminal runs cmd
                                          OSC133 D;<exit> ─▶ CommandLifecycleEvent
                                        on Ended (matching pid):
                                          find_executable(exe)?
                                            present ─▶ existing success redirect
                                            absent ─▶ AgentSetupResult{ok:false}
        ◀── AgentSetupResult{ok:false} ─
render "Retry"
click Retry ─ AgentInstallRunRequest ─▶ (reuse install pane)
```

## Components

### 1. `vmux_core/src/agent_setup.rs` — pure command builder (both targets, unit-tested)

`vmux_core` compiles to native and WASM, so both the page (display) and native
(execution) call the same code — no drift between what's shown and what's run.

```rust
/// True for agents installed via Homebrew (claude, codex).
pub fn requires_homebrew(segment: &str) -> bool;

/// The official Homebrew installer one-liner (interactive — prompts for sudo on the TTY).
pub fn homebrew_install_command() -> &'static str;
//  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

/// Full command to run in the terminal. When the agent needs Homebrew and it is
/// absent, prepend the installer + `brew shellenv`, wrapped in `bash -c '…'` so it
/// runs verbatim under nushell/zsh/bash. Otherwise returns today's plain command.
pub fn install_command_chained(segment: &str, brew_present: bool) -> Option<String>;
```

Chained form (claude, brew absent):

```bash
bash -c '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" && eval "$(/opt/homebrew/bin/brew shellenv 2>/dev/null || /usr/local/bin/brew shellenv)" && brew install --cask claude-code'
```

Notes:
- Outer `bash -c '…'` with **only double-quotes inside** → the user's shell (nushell
  included) passes the single-quoted literal straight to `bash`; all bash-isms
  (`&&`, `$( )`, `eval`) execute under bash, not the login shell.
- `brew shellenv` covers both Apple-Silicon (`/opt/homebrew`) and Intel (`/usr/local`)
  prefixes, putting `brew` on PATH for the cask step in the same session.
- `vibe` and the brew-present cases return the existing plain string (no wrapper).

Tests: assert the chained string for claude/codex when `brew_present=false`, the plain
string when `true`, and that `vibe` is identical in both cases.

### 2. Bridge events — `crates/vmux_agent/src/vibe/setup/event.rs` (shared, no cfg gate)

```rust
// page → native (added to the BinEventEmitterPlugin tuple)
pub struct AgentSetupPrereqRequest { pub agent: String }

// native → page (BinHostEmitEvent::from_rkyv)
pub struct AgentSetupPrereqStatus { pub needs_homebrew: bool }
pub struct AgentSetupResult { pub ok: bool }
```

Plus event-id constants for the two incoming (native→page) events, e.g.
`AGENT_SETUP_PREREQ_EVENT` / `AGENT_SETUP_RESULT_EVENT`. All derive the rkyv +
serde set already used by `AgentInstallRunRequest`.

### 3. Terminal completion event — `crates/vmux_terminal/src/plugin.rs`

OSC 133 shell integration is already injected (`vmux_service/src/shell_integration.rs`)
and the service already emits `ServiceMessage::CommandLifecycle { process_id, kind }` on
every prompt command (`vmux_service/src/process.rs`). Today `poll_service_messages` drops
it at `_ => {}`.

- Add a typed message `CommandLifecycleEvent { process_id, kind }` (mirror of the existing
  `ProcessExitedEvent`), register it, add it to `PollServiceWriters`.
- Fill the dropped arm in `poll_service_messages` to forward both `Started` and `Ended`.

Forwarding `Started` too lets the consumer arm itself and ignore the spurious initial
`D;0` a shell emits from its first `precmd`/prompt before our command runs.

### 4. Native setup logic — `crates/vmux_agent/src/vibe/setup.rs`

- New observer on `AgentSetupPrereqRequest`: compute
  `needs_homebrew = requires_homebrew(seg) && exec::find_executable("brew").is_none()`
  and reply to `trigger.event().webview` with `AgentSetupPrereqStatus` via
  `BinHostEmitEvent::from_rkyv`, guarded by `has_browser` / `host_emit_ready`.
  (macOS-gated; on non-macOS reply `needs_homebrew: false`.)
- `on_agent_install_run`: build the command with
  `install_command_chained(seg, exec::find_executable("brew").is_some())` instead of
  `install_command(seg)`. Store the pinned `ProcessId` on `AgentInstallPane` (the id is
  already minted at spawn) and an `armed: bool` flag.
- New observer on `CommandLifecycleEvent`: match `process_id` to an `AgentInstallPane`.
  On `Started` → `armed = true`. On `Ended` while `armed`:
  - `exec::find_executable(kind.executable())` present → reuse the existing success path
    (redirect + `ForcePaneClose`).
  - absent → emit `AgentSetupResult { ok: false }` to the setup
    page webview; leave the failed terminal pane open.
- Retry: the page re-emits `AgentInstallRunRequest`. `on_agent_install_run` detects an
  existing `AgentInstallPane` for the stack and reuses it (re-send `pending_input`,
  re-arm) rather than splitting a new pane.

`auto_redirect_agent_setup_when_installed` stays as the success path/no change.

### 5. Page — `crates/vmux_agent/src/vibe/setup/page.rs` (dumb renderer)

- `use_bin_event_listener::<AgentSetupPrereqStatus,_>(AGENT_SETUP_PREREQ_EVENT, …)` and
  `use_bin_event_listener::<AgentSetupResult,_>(AGENT_SETUP_RESULT_EVENT, …)`, plus a
  `use_effect` on mount that emits `AgentSetupPrereqRequest`.
- Signals: `needs_homebrew`, `installing`, `failed`.
- Render branches:
  - `failed` → "Install didn't finish" + "Retry" button (re-emits `AgentInstallRunRequest`,
    sets `installing=true`, `failed=false`).
  - `needs_homebrew` → "Homebrew required — we'll install it first, then {Name}", show
    Homebrew installer + cask steps, password note, button "Install Homebrew + {Name}".
  - else → today's UI unchanged.
- All copy/derived strings come from `vmux_core::agent_setup` where shared.

## Error handling

- Completion is detected from the shell's real `$?` at prompt return (OSC 133), not a
  timer — no false-positive on slow `brew install` (which can take minutes).
- Failure branch triggers on the agent binary still being absent after the command
  finishes, so a cancelled `sudo`, a network failure, or a brew error all surface as Retry.
- Page pushes are guarded by `has_browser` / `host_emit_ready`; if the page isn't ready the
  request/response handshake retries on the page side (the listener fires page-ready once
  attached).

## Testing

- `vmux_core`: unit tests for `requires_homebrew`, `install_command_chained` (chained vs
  plain vs vibe-unchanged), exact string assertions.
- `vmux_terminal`: a test that a `CommandLifecycle::Ended` service message produces a
  `CommandLifecycleEvent` (send the service message / run the poll system, assert the
  Bevy message), following the existing `ProcessExitedEvent` test pattern.
- `vmux_agent`: system/message tests — given an `AgentInstallPane` with a pinned
  `ProcessId`, a `CommandLifecycleEvent { Ended }` with the binary absent emits
  `AgentSetupResult { ok: false }`; with the binary present it takes the success path.
  Prereq request → status reply for claude (brew absent) vs vibe. Register written
  message types in the plugin `build()` (idempotent) so `cargo test --workspace` passes.
- Manual (single pass at the end): on a Mac without Homebrew, open claude setup → verify
  prereq copy, one password prompt, brew + cask install, auto-redirect; cancel sudo →
  verify Retry; verify vibe unchanged.

## Files

- `crates/vmux_core/src/agent_setup.rs` — new pure fns + tests.
- `crates/vmux_agent/src/vibe/setup/event.rs` — new event types + ids.
- `crates/vmux_agent/src/vibe/setup.rs` — prereq observer, chained command, lifecycle
  observer, retry/reuse, `AgentInstallPane` fields.
- `crates/vmux_agent/src/vibe/setup/page.rs` — listeners, prereq/failed render branches.
- `crates/vmux_terminal/src/plugin.rs` — `CommandLifecycleEvent` + fill the dropped
  `CommandLifecycle` arm in `poll_service_messages`.
- `crates/vmux_agent/src/plugin.rs` — register `AgentSetupPrereqRequest` in the emitter
  tuple (if not added in setup.rs).
