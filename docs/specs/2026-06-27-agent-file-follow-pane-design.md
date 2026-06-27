# Agent File Follow-Pane

**Date:** 2026-06-27
**Status:** Design — pending spec review
**Depends on:** `feat/per-profile-active-pane` (`docs/specs/2026-06-27-per-profile-active-pane-design.md`) — `ActivePanes`, `ActivatePane`, `ProfileId::Agent`, per-agent focus rings. Ships as a **separate PR stacked on** that branch.

## Problem

When an agent reads or edits a file, the user can't see which file. The agent's **native** Read/Edit tools emit nothing to vmux — they don't call `mcp__vmux__open_file`. (Observed: agent runs native Grep/Read, no MCP traffic, no preview.) The user wants the file the agent is working on to appear **instantly** in a live `file://` preview beside the agent.

vmux already has every downstream piece: the `vmux_editor` `file://` page, the `OpenBeside`/placement system (treats `file:` as `PageKind::File`, never splits the agent pane), and — on `feat/per-profile-active-pane` — a per-profile active-pane + colored-ring model where an agent's own actions move the agent's pane/ring without touching the human's focus. The only missing link is **detecting the agent's native file touches**.

## Goal

- On an agent **read or edit** of a real file, open or update a single `file://` **follow-pane** beside that agent, scrolled to the touched region.
- The follow-pane **is the agent's own active pane**: emit `ActivatePane{Agent}` (no `LastActivatedAt` stamp), so it lights up in that agent's ring color and the human's focus ring never moves.
- **Multi-agent:** one follow-pane per agent, each ringed in its own color (the per-profile model already scales to N).

## Non-goals

- **Search-hit preview** (Grep/Glob targets). Trigger is reads + edits only.
- **Diff view on edit.** Edit shows the live file scrolled to the change; git `DiffView` is deferred.
- **Codex reads.** Codex has no structured read tool (reads go through the shell); edits only. See Detection.
- **Tab history.** One follow-pane per agent; content swaps in place (no tab buildup).
- **Log-tailing.** Detection is hook-based; no session-log parser is built.

## Architecture / data flow

```
agent Read/Edit tool
  -> CLI hook (Claude PostToolUse | Vibe after_tool | Codex PostToolUse)
       command: vmux notify-file-touch --anchor <id>   (tool JSON on stdin)
  -> service: ClientMessage::AgentCommand { anchor, AgentCommand::FileTouched { path, line, kind } }
  -> ECS message: AgentFileTouch { anchor, path, line, kind }   (vmux_agent)
  -> follow-pane system:
       resolve agent pane from anchor
       open OR reuse the agent's file:// follow-pane (focus: false)
       navigate to file://<path> + scroll to <line>
       emit ActivatePane { Agent(anchor), pane: file_pane }   (no LastActivatedAt)
  -> per-profile ring system draws the agent's colored ring on the follow-pane
```

Everything from `AgentCommand::FileTouched` onward is **agent-agnostic**. Only hook injection differs per agent.

## Components

### 1. Detection — CLI hooks (per-agent injection, one shared notifier)

**Shared notifier — new `vmux` subcommand.** `vmux notify-file-touch --anchor <id>` (new module `crates/vmux_cli/src/commands/notify_file_touch.rs`, mirroring `notify.rs`). Reads the hook's tool JSON from **stdin**, extracts:

- `tool_name` → `kind` (Read vs Edit/Write/MultiEdit/apply_patch).
- `tool_input.file_path` → `path` (absolute; skip if absent or relative).
- `line`: Read → `tool_input.offset` (when present); Edit → locate `old_string`'s first line in the file (best-effort); else `None`.
- `anchor`: from `--anchor`, falling back to `VMUX_ANCHOR` (same pattern as `notify.rs:21-23`).

No-op (exit 0, send nothing) when `file_path` or anchor is missing — so the hook is safe even outside vmux. Sends `ClientMessage::AgentCommand { anchor, AgentCommand::FileTouched { path, line, kind } }` to the service.

**Per-agent injection** (the only agent-specific code; built in each `build_args`/`build_env`):

| Agent | Mechanism | Covers |
|---|---|---|
| **Claude** 2.1.x | `--settings '<inline JSON>'` with `PostToolUse` matcher `Read\|Edit\|Write\|MultiEdit`, command `vmux notify-file-touch --anchor <id>`, `async: true`. Merges with user settings; per-invocation; does not touch `~/.claude/settings.json`. | reads + edits |
| **Vibe** 2.17.x | `VIBE_ENABLE_EXPERIMENTAL_HOOKS=true` in `build_env` **+** a vmux-managed `~/.vibe/hooks.toml` `after_tool` hook (match read/edit tools) → `vmux notify-file-touch`. Hook **no-ops when `VMUX_ANCHOR` is unset**, so manual vibe use is unaffected. | reads + edits |
| **Codex** 0.142.x | `-c features.hooks=true` + hooks config (`PostToolUse` matcher `apply_patch\|Edit\|Write`) → `vmux notify-file-touch --anchor <id>`. | **edits only** |

Codex has no structured read tool — it reads via the shell (`cat`/`rg`/`sed`), which fires `PostToolUse` as a **Bash** event carrying a command string, not a `file_path`. Recovering a path from that is the shell-scraping fragility we rejected, so Codex is edits-only until it gains a read tool. Codex's `apply_patch` PostToolUse was broken pre-v0.118.0 (openai/codex#16732) and is documented as supported now; **verify it fires on 0.142.2 during implementation.**

### 2. Transport — `AgentCommand::FileTouched`

New variant on the existing agent command enum (where `AgentCommand`/`AgentQuery` live in `vmux_core`/`vmux_service::protocol`):

```rust
enum FileTouchKind { Read, Edit }
AgentCommand::FileTouched { path: String, line: Option<usize>, kind: FileTouchKind }
```

The service forwards `{ anchor, command }` into the ECS exactly like the other `AgentCommand`s (e.g. the `Notify`/`OpenBeside` path in `vmux_agent::plugin`), producing the `AgentFileTouch` message.

### 3. Follow-pane — `vmux_agent` system (mirrors `claim_browser_pane`)

New Bevy message `AgentFileTouch { anchor, path, line, kind }`. System `on_agent_file_touch` (modeled on `AgentBrowserResolve::claim_browser_pane`, `plugin.rs:589-604`, and `browser_pane_for`, `plugin.rs:566-584`):

1. Resolve the agent term + parent pane from `anchor` (`agent_terms.find(pid == anchor)` → `ChildOf`).
2. Resolve the agent's existing **file** follow-pane via `file_pane_for(agent_pane)` — a sibling leaf pane under the same split whose stack hosts a `file://` page (`PageKind::File`). Analogous to `browser_pane_for` but filtered to the editor host.
3. **If none:** open beside the agent with `direction: None` so the existing placement decides — `resolve_placement` reuses an existing `file://` leaf or spirals a new one and **never splits the agent pane** (`PageKind::File`). Reuse the same `OpenBeside`/`OpenBesideRequest` path the `open_file` MCP tool already uses (`anchor = agent`, `focus: false`), so a manual `open_file` and the follow-pane share one pane.
4. **If it exists:** navigate it to `file://<path>` (reuse `vmux_editor` `FileOpenEvent` / `on_file_open`, `plugin.rs:675-708`) and scroll to `line` (reuse the `apply_goto`/`LspGoto` jump-to-line path, `plugin.rs:1280-1338`). No new pane.
5. Emit `ActivatePane { profile: ProfileId::Agent(format!("{anchor:?}")), active: ActiveStack { pane: Some(file_pane), .. } }` — **never stamp `LastActivatedAt`** (the per-profile invariant). Use the **same** `format!("{anchor:?}")` keying as `claim_browser_pane` so an agent's browser pane and file pane share one `ProfileId`.

**One follow-pane per agent**, keyed by anchor; content swaps. With N agents there are N follow-panes and N `ActivePanes[Agent]` entries.

### 4. Scroll-to-line

Reuse `vmux_editor`'s existing jump-to-line (`apply_goto` / `LspGoto`, `plugin.rs:1280-1338`) and `FileOpenEvent` (`on_file_open`). Best-effort: Read `offset` → line; Edit → first line of `old_string`; fallback to top.

### 5. Ring + focus — inherited from per-profile model

No new ring/focus code. The follow-pane becomes `ActivePanes[Agent(anchor)]`, so the per-agent windowed/OSR ring (`feat/per-profile-active-pane`, commit `eac17cfc`) renders it in the agent's color. `focus: false` + no `LastActivatedAt` stamp ⇒ the local human's `FocusedStack` / OS keyboard focus / ring are untouched.

### 6. Toggle

`settings.ron` `agent.follow_files: bool`, default **on** (absent ⇒ on, per the no-auto-seed fallback convention). When off: skip hook injection at launch and ignore any `FileTouched` that arrives.

## Error handling

- **Non-blocking by construction.** PostToolUse fires after the tool and cannot block it; Vibe `after_tool` uses `strict = false`. `notify-file-touch` no-ops on missing path/anchor or an unreachable service socket.
- **Success-only.** PostToolUse fires only on tool success, so a failed edit won't preview — acceptable.
- **Stale entities.** If the agent term or pane has despawned, `on_agent_file_touch` resolves `None` and no-ops; `prune_active_panes` (per-profile) drops dead `ActivePanes` entries.
- **Bad path.** Non-absolute / non-`file:` paths are skipped (`path_from_files_url` requires absolute).
- **Vibe `hooks.toml`.** Write is idempotent and confined to a vmux-managed block; never clobber the user's existing hooks.

## Risks / verify during implementation

- Codex `apply_patch` PostToolUse actually fires on 0.142.2 (one manual test).
- Vibe experimental-hooks stability; confirm a managed `~/.vibe/hooks.toml` + env flag is the cleanest injection (no per-invocation flag exists).
- Hook latency: Claude `async: true`; Vibe `after_tool` runs after the tool body — acceptable for a passive preview.
- Edit line-locating accuracy (string search) — fallback to top is fine.
- Rapid successive touches: swap is cheap; coalesce only if flicker is observed.

## Testing

- **Unit (`vmux_cli`):** `notify-file-touch` parses Claude PostToolUse (Read w/ `offset`, Edit w/ `old_string`), Vibe `after_tool`, and Codex `apply_patch` JSON into the correct `{ path, line, kind }`; no-ops on missing path/anchor.
- **Integration (Bevy, `vmux_agent`):** `AgentFileTouch` opens a `file://` pane beside the agent with `focus: false`; `FocusedStack` / `ActivePanes[Local]` unchanged; `ActivePanes[Agent(anchor)] == file_pane`; a second touch swaps the **same** pane (no new pane); a despawned agent ⇒ no-op.
- **Placement:** the file follow-pane never splits the agent pane (extends existing placement guarantees).
- **Multi-agent:** two agents ⇒ two follow-panes, two disjoint `ActivePanes[Agent]` entries.

## Sequencing (one PR, on top of `feat/per-profile-active-pane`)

1. **Shared downstream:** `AgentCommand::FileTouched` + `AgentFileTouch` message + `on_agent_file_touch` (open/reuse + navigate + scroll) + `ActivatePane` emission + `agent.follow_files` toggle.
2. **CLI:** `vmux notify-file-touch` subcommand + stdin parsing.
3. **Claude injection** (`--settings`) — validates the original screenshot case end-to-end.
4. **Vibe injection** (env flag + managed `hooks.toml`).
5. **Codex injection** (`-c features.hooks` + `apply_patch` matcher), edits-only; verify firing.
