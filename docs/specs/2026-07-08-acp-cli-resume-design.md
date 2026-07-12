# ACP ↔ CLI Session Resume (`/resume`) — Design

Date: 2026-07-08
Status: Core implementation complete; selector-input refinement approved in chat, pending plan

## Summary

Add a `/resume` affordance that lists an agent's **past on-disk sessions** and lets the
user reopen any of them, and enable **cross-runtime handoff** of a single conversation
between the ACP runtime (`vmux://agent/<id>`) and the CLI PTY runtime
(`vmux://agent/<kind>/cli/<sid>`).

Trigger: a `/` slash-command menu recognized by the ACP agent page composer (Claude Code /
Zed style). Keyboard input from anywhere on the page is routed into the composer when it does
not conflict with a selector action or a modified shortcut.

Core user story: start a session as `agent/claude` (ACP); hit a feature only the CLI has;
run `/cli` to **fall back to the CLI, resuming the same session, in the same page**.

## Goals

- List past sessions on disk for all agent kinds (claude, codex, vibe), recent-first.
- `/resume` in the ACP composer → pick a session → **swap the current page in place** to it.
- Cross-runtime handoff (ACP ↔ CLI) of the **same** session id, same cwd, same page —
  enabled only where session-id sharing is verified.
- Extensible slash mechanism in the composer (`/resume`, `/cli` first; room for more).

## Non-goals (deferred)

- A dedicated `vmux://resume` browser page (backbone is built reusable for it later).
- Surfacing agent-advertised ACP `availableCommands` (currently dropped in the projector).
- Codex/Vibe cross-runtime handoff (list + same-runtime resume only, until id-sharing is
  verified per kind).
- Per-row runtime pick inside the `/resume` list (runtime switch is the separate `/cli`
  command for v1).
- Cross-machine resume (transcripts are local files).

## Background — current state

Resume plumbing already exists on both sides; what's missing is discovery/picker + a bridge.

- **URL grammar** (`crates/vmux_agent/src/url.rs`): `AgentUrl::Cli { kind, sid }` ⇒
  `vmux://agent/<kind>/cli[/<sid>]`; `AgentUrl::Acp { id, sid }` ⇒
  `vmux://agent/<id>[/<sid>]`. `CLI_FRESH_SID = "cli"`.
- **ACP resume**: `AcpSession.resume: Option<String>` drives `session/load`
  (`crates/vmux_service/src/acp/driver.rs`, gated on `agent_capabilities.load_session`,
  graceful fallback to `session/new`). The agent-assigned `acp_session_id` is persisted in
  the pane URL via `apply_acp_session_created` (`crates/vmux_agent/src/client/acp.rs`).
- **CLI resume**: per-kind flags in `build_args` — codex `resume <sid>`, vibe/claude
  `--resume <sid>` (`crates/vmux_agent/src/client/cli/{codex,vibe,claude}.rs`). Entry points:
  opening a resume URL and `handle_restart_agent_pty` (`crates/vmux_agent/src/plugin.rs`).
- **CLI session discovery** (one-shot, matches a live PTY): `CliAgentStrategy::discover_session`
  scrapes `sessions_root` (`~/.claude/projects`, `~/.codex/sessions`, `~/.vibe/logs/session`),
  driven by an fs-watcher (`crates/vmux_agent/src/session.rs`).
- **Persistence**: only `PageMetadata.url` (carrying the sid) + `TerminalLaunch` round-trip.
  Restore re-runs the CLI **with resume** if the URL carried a sid
  (`crates/vmux_desktop/src/persistence.rs`, `rebuild_space_views`). `STORE_SCHEMA_VERSION = 4`.
- **No slash mechanism** anywhere. Composer `do_submit` sends raw text as `ChatSubmit`
  (`crates/vmux_agent/src/chat_page/page.rs`). Command bar uses `>` for commands, `/` for paths.
- **ACP `availableCommands`** are dropped: `projector.rs` `apply()` has `_ => Vec::new()`.
- ACP ids and CLI `AgentKind`s are **separate namespaces** today (string coincidence only).
  A bare `vmux://agent/claude` resolves to ACP (it's in `settings.acp`), never CLI.

## Key verified facts (load-bearing)

- vmux runs **`npx -y @zed-industries/claude-code-acp@latest`** for ACP "claude"
  (`crates/vmux_setting/src/settings.ron`; registry may override via id `claude-acp`).
- **claude-code-acp ≥ 0.12 unifies session ids with the CLI.** The ACP `sessionId` **is**
  the CLI session UUID **is** the `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl` stem.
  Same on-disk store. Verified in adapter source + tests; `@latest` is well past the floor.
  ⇒ The `acp_session_id` vmux already persists is a valid `claude --resume <sid>` target,
  both directions.
- **Hard constraint: same cwd.** Sessions are keyed by encoded cwd. A handoff (or resume)
  from a different cwd silently starts a fresh session. cwd MUST travel with the session.
- Codex/Vibe id-sharing across ACP and CLI is **unverified** — treat as false for now.

## Model — runtime-agnostic session identity

A session is `(kind, sid, cwd)`. **Runtime** (ACP vs CLI) is how you open it, not what it is:

- ACP projection: `vmux://agent/<kind>/<sid>` → `session/load`
- CLI projection: `vmux://agent/<kind>/cli/<sid>` → `--resume`/`resume`

Both projections require the same cwd. `/resume` chooses **which** session; `/cli` chooses
**which runtime** for the current session.

## Architecture

### 1. Backend session lister

New method on `CliAgentStrategy` (`crates/vmux_agent/src/client/cli/strategy.rs`), reusing
each kind's existing `sessions_root` + parsers:

```rust
fn list_sessions(&self) -> Vec<ResumableSession>;
```

```rust
pub struct ResumableSession {
    pub kind: AgentKind,
    pub sid: String,
    pub cwd: PathBuf,
    pub mtime: SystemTime,
    pub title: String,        // first user message / summary; falls back to short sid
    pub cross_runtime: bool,  // ACP↔CLI handoff safe for this kind (claude=true)
}
```

- claude: iterate `~/.claude/projects/<proj>/<uuid>.jsonl` → sid=stem, cwd from jsonl
  (or decoded proj dir), mtime, title from first user turn.
- codex: walk `~/.codex/sessions/**/*.jsonl`, read `session_meta` first line → id, cwd.
- vibe: `~/.vibe/logs/session/session_*/meta.json` → id, cwd, times.

A backend collector unions across registered strategies, sorts by mtime desc, dedups by
`(kind, sid)`. Claude's store is shared by ACP+CLI, so one scan covers both runtimes.
**Scan on demand** (when `/resume` is invoked), not eagerly. `cross_runtime` is a per-kind
capability: claude=true; codex/vibe=false until verified.

### 2. Composer slash mechanism (frontend — ACP agent page)

In `crates/vmux_agent/src/chat_page/page.rs` (Dioxus, WASM). Keep the frontend dumb:
render + emit intents; command/session data comes from the backend.

- Input beginning with `/` enters slash mode. A drop-up menu renders above the textarea,
  filtered by the command token after `/`.
- The first visible row is selected immediately. Arrow Up/Down and Ctrl+P/N navigate with
  wraparound. Enter executes the selected row; users never need to move away from the default
  selection before invoking it. Esc closes the selector.
- Every selected row has a stable DOM id. Selection changes call
  `scrollIntoView({ block: "nearest" })`, including after filtering, so keyboard navigation
  never leaves the selected row outside the selector viewport.
- Command entries are pushed from the backend as a snapshot (`[{name, description}]`).
  v1 commands: `resume`, `cli` (`cli` shown only when the pane's kind has `cross_runtime`).
- Selecting a command emits a typed intent; it does **not** send text to the agent.
- A `/foo` that matches no vmux command and is submitted normally is sent to the agent as a
  plain prompt (agent decides what to do with it). vmux only intercepts its own commands.

The prompt parser has two explicit states:

- `/token` — command selector, filtered by `token`.
- `/resume <query>` — session selector. Entering the whitespace after the exact `resume`
  command opens the picker and requests the session snapshot once. Selecting `/resume` from
  the command selector changes the prompt to `/resume ` and enters the same state.

The page owns a global `keydown` listener for prompt capture:

- Selector handling has priority over prompt editing.
- When a selector is open, Arrow Up/Down and Ctrl+P/N navigate; Enter invokes; Esc closes.
- Otherwise, unmodified printable input, Backspace, and Delete focus and edit the textarea at
  its current selection/caret, even when focus was elsewhere on the page.
- Shift remains available for uppercase/symbol input. Cmd, Ctrl, and Alt modified keys are
  untouched, except Ctrl+P/N while a selector is open.
- Events already targeting the textarea are left to its normal Dioxus handler so each key is
  applied exactly once.

Interactive overlay z-index caveat: the menu must sit above chat rows (see terminal overlay
z-index memory) and capture pointer/keyboard while open.

### 3. `/resume` flow + in-place swap

1. `/resume` selected, or `/resume ` typed directly → prompt becomes/remains `/resume ` and the
   frontend emits `ResumeListRequest` once for that picker entry.
2. Backend resolves the requesting page's current agent kind, runs the lister, retains only
   that kind's sessions, then pushes `ResumableSessionsSnapshot` to the page.
3. The session selector filters incrementally as `<query>` changes. Matching is
   case-insensitive across sid, title, and cwd; an exact or partial sid therefore narrows the
   list naturally while the literal `/resume <query>` stays visible in the prompt.
4. The first matching row is selected whenever the filtered result set changes. Enter emits
   `ResumeSession { kind, sid, cwd }`; the backend resumes it in the current pane's runtime.
5. No matches renders `No matching sessions`. An empty unfiltered history renders
   `No resumable sessions found`. Enter does nothing in either empty state and never submits
   the `/resume` text to the agent.
6. Backend **swaps the current page in place**:
   - Tear down the old session pane on the stack. ACP: removing `AcpSession` already fires
     `close_acp_session_on_remove` → `ClosePageAgent`. CLI: kill `ProcessId`, drop
     `AgentSession`/`SessionId`.
   - Attach the new session on the **same stack** carrying cwd: reuse
     `SpawnAgentInStackRequest { stack, cwd, session_id }` (CLI) or the ACP attach-to-stack
     path (`attach_acp_agent_to_stack`) with `resume = Some(sid)`.

Introduce `SwapStackSession { stack, target_url: String, cwd }`, which performs
teardown-then-attach so both `/resume` and `/cli` share it without adding a
`vmux_agent` dependency to `vmux_core`.

### 4. Cross-runtime handoff (`/cli` — the fallback)

`/cli` in the ACP composer emits `SwapStackSession` with a CLI `target_url`, current sid,
and cwd → CLI PTY resumes the same conversation, same page.

- Gated by `ResumableSession.cross_runtime` / a per-kind capability. Claude only for v1.
- cwd is taken from the live `AcpSession.cwd` (never `default_cwd`).
- Reverse (`/acp` from a CLI pane) is deferred because CLI panes have no vmux composer.

### 5. Cmd+K entry points (deferred)

The Cmd+K session browser and "Continue in CLI/ACP" `AppCommand` leaves are deferred. A
follow-up can route `on_command_bar_action` through the same `SwapStackSession` flow.

## Data flow & messages

Frontend → backend (CEF bin events; keep dumb-frontend contract):
- `SlashCommandsRequest` → backend replies `SlashCommandsSnapshot { commands }`.
- `ResumeListRequest` → backend replies `ResumableSessionsSnapshot { sessions }`.
- `ResumeSession { kind, sid, cwd }` — invoke.

Backend (ECS):
- `SwapStackSession { stack, target_url: String, cwd }` — teardown-then-attach on a stack.
- Reuse: `SpawnAgentInStackRequest` (CLI), ACP attach-to-stack, `ClosePageAgent`.

Register new message/component types in the owning plugin's `build()` (idempotent), not
per-test. `vmux_core::event` compiles for wasm — cfg-gate any Bevy `Message`/`Component` to
`not(wasm32)` (see vmux_core wasm memory).

## Edge cases

- **Already open**: target sid already live in a tab → focus it instead of double-opening.
  CLI via `AgentSessionToEntity`; ACP via routing sid map.
- **Stale sid**: ACP `session/load` already falls back to `session/new`. CLI: surface
  "session not found," keep the pane rather than silently starting fresh.
- **cwd mismatch**: prevented — cwd is carried in `ResumableSession`/`SwapStackSession`.
- **Missing binary**: existing setup-page path (`attach_cli_setup_to_stack`).
- **Empty history / huge lists**: lister caps + relative-time formatting; prompt-driven filter.
- **Filtered empty state**: keep the session selector open and render `No matching sessions`;
  Backspace/global typing can recover without reopening it.
- **Direct argument**: pasting or typing `/resume <sid>` enters the session selector without a
  separate command-selection step. Enter resumes the highlighted match; it does not submit the
  raw command to the agent.
- **Global prompt input**: selector keys win; all unrelated Cmd/Ctrl/Alt shortcuts retain their
  browser/app behavior. Global edits preserve the textarea caret and selection.
- **Persistence**: the swapped session's URL is rewritten (ACP already does this on
  `AcpSessionCreated`; CLI via `format_agent_url`), so a restart resumes the post-swap state.

## Testing

- `list_sessions` parsers per kind against fixture session dirs (sid, cwd, title, mtime).
- URL build/round-trip for both projections and the ACP↔CLI swap
  (`vmux://agent/claude/<sid>` ↔ `vmux://agent/claude/cli/<sid>`).
- In-place swap: message-driven ECS test — send `SwapStackSession`, assert the stack's child
  session pane is replaced and the new session carries the correct cwd + resume sid.
- `cross_runtime` gating hides `/cli` (and "Continue in CLI") for codex/vibe.
- Composer slash parser/filter/navigation helpers: native unit tests for `/token` vs
  `/resume <query>`, current-kind session filtering, case-insensitive sid/title/cwd matching,
  first-row selection, wraparound, and empty results.
- `page.rs` integration/source-scrape assertions for Arrow Up/Down + Ctrl+P/N handling,
  `scrollIntoView` with `Nearest`, selected-row ids, global prompt capture, modifier exclusions,
  and `No matching sessions`.
- Native `cargo test -p vmux_agent`; WASM compile check for the Dioxus page.
- Observable-behavior check: assert the snapshot/broadcast the frontend receives, not just
  internal state (verify-observable-behavior memory). Runtime test is user-driven at the end.

## Scope / phasing

**v1 (this PR):**
- Backend `list_sessions` for all 3 kinds + collector.
- Composer slash menu (`/resume`, `/cli`) in the ACP agent page.
- Prompt-driven `/resume <sid/query>` filtering, default selection, Ctrl+P/N navigation,
  selected-row scrolling, and global prompt keyboard capture.
- `/resume` list → in-place swap; cwd carry.
- `/cli` cross-runtime handoff (Claude), gated by `cross_runtime`.
- `SwapStackSession` message + tests.

**Deferred:** Cmd+K session browser + "Continue in CLI/ACP" commands; `vmux://resume` page;
ACP `availableCommands` surfacing; codex/vibe cross-runtime (post-verification); per-row
runtime pick.

## Risks / open questions

- claude-code-acp version drift: `@latest` is past the id-unify floor, but a future adapter
  change could alter id semantics. Mitigation: `cross_runtime` is a capability flag, easy to
  flip off.
- Title extraction cost for large `~/.claude/projects` — read only the first line/turn per
  file; cap scan; sort by mtime before enriching titles.
- In-place teardown races (daemon session close vs new attach on same stack) — sequence via
  the single `SwapStackSession` handler; assert with the ECS test.
