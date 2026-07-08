# ACP ↔ CLI Session Resume (`/resume`) — Design

Date: 2026-07-08
Status: Approved (brainstorm), pending implementation plan

## Summary

Add a `/resume` affordance that lists an agent's **past on-disk sessions** and lets the
user reopen any of them, and enable **cross-runtime handoff** of a single conversation
between the ACP runtime (`vmux://agent/<id>`) and the CLI PTY runtime
(`vmux://agent/<kind>/cli/<sid>`).

Primary trigger: a `/` slash-command menu recognized **in the ACP agent page prompt
textarea** (Claude Code / Zed style). Secondary trigger: the Cmd+K command bar (for CLI
panes and global invocation).

Core user story: start a session as `agent/claude` (ACP); hit a feature only the CLI has;
run `/cli` to **fall back to the CLI, resuming the same session, in the same page**.

## Goals

- List past sessions on disk for all agent kinds (claude, codex, vibe), recent-first.
- `/resume` in the ACP composer → pick a session → **swap the current page in place** to it.
- Cross-runtime handoff (ACP ↔ CLI) of the **same** session id, same cwd, same page —
  enabled only where session-id sharing is verified.
- Extensible slash mechanism in the composer (`/resume`, `/cli` first; room for more).
- Cmd+K "Resume session" as a secondary entry point.

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

Both projections require the same cwd. `/resume` chooses **which** session; `/cli` (or the
Cmd+K reverse) chooses **which runtime** for the current session.

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
  filtered by the text after `/`. Arrow keys navigate, Enter selects, Esc closes.
- Command entries are pushed from the backend as a snapshot (`[{name, description}]`).
  v1 commands: `resume`, `cli` (`cli` shown only when the pane's kind has `cross_runtime`).
- Selecting a command emits a typed intent; it does **not** send text to the agent.
- A `/foo` that matches no vmux command and is submitted normally is sent to the agent as a
  plain prompt (agent decides what to do with it). vmux only intercepts its own commands.

Interactive overlay z-index caveat: the menu must sit above chat rows (see terminal overlay
z-index memory) and capture pointer/keyboard while open.

### 3. `/resume` flow + in-place swap

1. `/resume` selected → frontend emits `ResumeListRequest`.
2. Backend runs the lister → pushes `ResumableSessionsSnapshot` to the page.
3. Menu becomes session rows: `title · relative-time · cwd`, filterable.
4. Enter on a row → emits `ResumeSession { kind, sid, runtime }` (backend already holds cwd
   from the snapshot; runtime defaults to the current pane's runtime).
5. Backend **swaps the current page in place**:
   - Tear down the old session pane on the stack. ACP: removing `AcpSession` already fires
     `close_acp_session_on_remove` → `ClosePageAgent`. CLI: kill `ProcessId`, drop
     `AgentSession`/`SessionId`.
   - Attach the new session on the **same stack** carrying cwd: reuse
     `SpawnAgentInStackRequest { stack, cwd, session_id }` (CLI) or the ACP attach-to-stack
     path (`attach_acp_agent_to_stack`) with `resume = Some(sid)`.

Introduce one message, e.g. `SwapStackSession { stack, target: AgentUrl, cwd }`, that
performs teardown-then-attach so both `/resume` and `/cli` share it.

### 4. Cross-runtime handoff (`/cli` — the fallback)

`/cli` in the ACP composer → `SwapStackSession { stack, target: Cli{kind,sid}, cwd }` with
the current session's sid + cwd → CLI PTY resumes the same conversation, same page.

- Gated by `ResumableSession.cross_runtime` / a per-kind capability. Claude only for v1.
- cwd is taken from the live `AcpSession.cwd` (never `default_cwd`).
- Reverse (`/acp` from a CLI pane) is offered via Cmd+K only — CLI panes have no vmux
  composer. Same `SwapStackSession` with `target: Acp{...}`.

### 5. Cmd+K secondary entry

- New `AppCommand` leaf: "Resume session" (`crates/vmux_command/src/command.rs`). Auto-appears
  in the `>` list via the `CommandBar` derive.
- Handled in `crates/vmux_layout/src/command_bar/handler.rs` (`on_command_bar_action`): opens
  the palette in a new "sessions" group fed by the same backend lister (mirrors the existing
  tabs/spaces/pages/work_dirs sections in `command_bar/event.rs` + `results.rs`).
- Selecting a session issues the same `SwapStackSession` (targeting the focused/served stack;
  if none, spawn into a new/focused stack).
- For a live agent pane, "Continue in CLI" / "Continue in ACP" commands trigger the runtime
  handoff (the Cmd+K equivalent of `/cli`/`/acp`).

## Data flow & messages

Frontend → backend (CEF bin events; keep dumb-frontend contract):
- `SlashCommandsRequest` → backend replies `SlashCommandsSnapshot { commands }`.
- `ResumeListRequest` → backend replies `ResumableSessionsSnapshot { sessions }`.
- `ResumeSession { kind, sid, runtime }` — invoke.

Backend (ECS):
- `SwapStackSession { stack, target: AgentUrl, cwd }` — teardown-then-attach on a stack.
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
- **Empty history / huge lists**: lister caps + relative-time formatting; filter box.
- **Persistence**: the swapped session's URL is rewritten (ACP already does this on
  `AcpSessionCreated`; CLI via `format_agent_url`), so a restart resumes the post-swap state.

## Testing

- `list_sessions` parsers per kind against fixture session dirs (sid, cwd, title, mtime).
- URL build/round-trip for both projections and the ACP↔CLI swap
  (`vmux://agent/claude/<sid>` ↔ `vmux://agent/claude/cli/<sid>`).
- In-place swap: message-driven ECS test — send `SwapStackSession`, assert the stack's child
  session pane is replaced and the new session carries the correct cwd + resume sid.
- `cross_runtime` gating hides `/cli` (and "Continue in CLI") for codex/vibe.
- Composer slash menu: `page.rs` source-scrape tests (see page.rs source-scrape memory);
  native `cargo test -p vmux_layout`/`vmux_agent`.
- Observable-behavior check: assert the snapshot/broadcast the frontend receives, not just
  internal state (verify-observable-behavior memory). Runtime test is user-driven at the end.

## Scope / phasing

**v1 (this PR):**
- Backend `list_sessions` for all 3 kinds + collector.
- Composer slash menu (`/resume`, `/cli`) in the ACP agent page.
- `/resume` list → in-place swap; cwd carry.
- `/cli` cross-runtime handoff (Claude), gated by `cross_runtime`.
- Cmd+K "Resume session" + "Continue in CLI/ACP".
- `SwapStackSession` message + tests.

**Deferred:** `vmux://resume` page; ACP `availableCommands` surfacing; codex/vibe
cross-runtime (post-verification); per-row runtime pick.

## Risks / open questions

- claude-code-acp version drift: `@latest` is past the id-unify floor, but a future adapter
  change could alter id semantics. Mitigation: `cross_runtime` is a capability flag, easy to
  flip off.
- Title extraction cost for large `~/.claude/projects` — read only the first line/turn per
  file; cap scan; sort by mtime before enriching titles.
- In-place teardown races (daemon session close vs new attach on same stack) — sequence via
  the single `SwapStackSession` handler; assert with the ECS test.
