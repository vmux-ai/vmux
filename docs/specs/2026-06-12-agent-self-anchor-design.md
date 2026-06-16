# Agent Self-Anchor + Relative Layout

Give each CLI agent (vibe / claude / codex) a stable handle to its **own** pane, so it can place panes relative to itself ("open a terminal beside me") and reason about where it lives. This is the foundation for the broader "vmux as an agentic IDE" direction.

This is spec #4 of that arc. The next spec, #1 (run a command in a terminal next to the agent, with output read back), builds directly on the primitive defined here.

## Revision (v2): `open_page` / `run` / `read_terminal` agent surface

The model is **Live Share with a coworker**: the agent acts in *visible, shared, interactive* terminals the user can take over. The agent's open/run MCP surface consolidates to self-relative verbs (all resolved via the `ProcessId` anchor below):

- **`open_page { url, direction?=right, focus?=true }`** — open a page (terminal `vmux://terminal/`, else browser) in a new pane beside the agent. (Renamed from the original `open_beside_me`; internal protocol/command names still read `OpenBeside`.)
- **`run { command, direction?=right, focus?=false }`** — open a terminal beside the agent and **type `command` into an interactive shell** (so the user watches live and can take over; the shell persists after the command). `focus` defaults false (the agent is driving). Reuses `split_leaf_into_two` + `TerminalStackSpawnRequest { pending_input }`.
- `focus_self` is **removed** — redundant with `update_layout`'s `focused` triple.

**Read-back = "read the visible terminal"** (Live Share): the agent reads scrollback like the user does — no clean stdout/exit capture. Feasible because the service already broadcasts `ProcessOutput` (raw bytes) + `ProcessExited { exit_code }`; future optimization (clean `{output, exit}` for a fresh `run` terminal, or OSC-133 shell-integration markers for existing shells) is purely additive at the read layer.

**Still pending (next increments on this branch):**
1. `read_terminal { terminal }` — return a terminal's scrollback text; answered by the **service** rendering the `Process` `Term` (reuse `snapshot`/`snapshot_text`).
2. `run { terminal }` — run in an existing terminal (by handle), not just a new one.
3. **Terminal handle = `ProcessId`** exposed as `process_id` on terminal `Stack` DTOs in `read_layout`, and returned by `run`, so the agent can target/read specific terminals.
4. Remove the now-superseded `in_pane`, `run_shell`, `new_terminal_tab` MCP tools (keep `browser_navigate` — that's *navigate an existing browser*, a different job).

The sections below describe the original `open_beside_me`/`focus_self` design; the anchor (`ProcessId`) + `is_self` foundation is unchanged and still current.

## Motivation

vmux already injects its MCP server into all three CLIs (vibe via `VIBE_MCP_SERVERS`, claude via `--mcp-config`, codex via `-c mcp_servers.vmux`), and they already have layout tools (`read_layout`, `update_layout`, `open_in_*`, `new_terminal_tab`, `run_shell`). Two gaps block the IDE experience:

1. **No self-reference.** `read_layout` returns a `focused: {tab, pane, stack}` triple, but that is whatever the **human** last clicked — not the calling agent's pane. If the user focuses another pane, the agent loses track of itself. There is no marker that says "this stack is you."
2. **No anchor for relative placement.** Because the agent can't identify its own pane, it cannot reliably "open X beside me." `open_in_pane` and friends act against the focused pane.

The MCP server process also has no identity today: `mcp::resolve` (`crates/vmux_agent/src/mcp.rs:14`) injects only `args: ["mcp"]`.

## Approach

Reuse `ProcessId` (`crates/vmux_core/src/process_id.rs:8`, re-exported as `vmux_core::ProcessId` and `vmux_service::protocol::ProcessId`) as the agent's self-anchor. `ProcessId` is a `[u8; 16]` UUID newtype with `Display`/`FromStr`, and it is vmux's canonical PTY handle.

It satisfies the three things an anchor needs:

- **Known at spawn.** `ProcessId::new()` is minted client-side before `CreateProcess` is sent (`crates/vmux_terminal/src/plugin.rs:595`). We can hand it to the launch builder so it lands in the MCP server's argv.
- **Resolvable.** Terminal entities already carry `&ProcessId` — a direct query, no new lookup map. The `--anchor` string round-trips through the existing `Display`/`FromStr`.
- **Restart-stable for free.** `ProcessId` is the persistent-session re-attach key. On GUI restart, `reattach_terminal_bundle(process_id)` (`crates/vmux_terminal/src/plugin.rs:683`) re-creates the terminal with the **same** id and sends `AttachProcess`. The MCP server — a child of the still-alive CLI/PTY — keeps sending its original `--anchor`; the restored entity carries the same `ProcessId`; they re-match. No moonshine `Reflect`/`Save` is needed (and `ProcessId` is deliberately not `Reflect`).

No new component is introduced. `vmux_core` is untouched.

### Why `ProcessId` and not a new `AnchorToken`

An earlier draft minted a dedicated `AnchorToken(String)` persisted on the stack via moonshine. `ProcessId` is strictly better: it already exists, is already restart-stable by design, already round-trips as a string, and lives on the **content** entity — so "self" follows the agent if the human drags it into a different stack, rather than being pinned to the original stack.

### Why `ProcessId` and not `SessionId`

`SessionId` (the CLI's own session id) is the agent's *durable* identity, but it is the wrong choice for the *anchor*:

- **Not known at spawn.** Fresh (non-resumed) sessions have no sid until vmux discovers it from the CLI's log dir seconds after launch (the `PendingAgentSession` window). The MCP server's `--anchor` is fixed at CLI-launch time, before the sid exists.
- **Doesn't identify a pane instance.** Resuming the same sid in two panes yields two stacks with the same `SessionId` — ambiguous "self". `ProcessId` is unique per running process.
- **Over-scoped.** The anchor only needs to live as long as the MCP server, which shares the process lifetime. A conversation-lifetime id buys nothing here.

`SessionId` still does its own job — it is the durable identity persisted in the stack url and used to resume agents across Mac restart (see Persistence / Restart). The two are complementary, not competing.

## Identity Resolution

Anchor → location walk:

```
ProcessId (--anchor)
  → terminal entity with that ProcessId (the one also carrying AgentSession)
    → parent Stack  (ChildOf)
      → parent Pane (ChildOf)
```

Parent chain confirmed by the spawn paths (`respond_process_stack_spawn` / `handle_spawn_agent_requests`): terminal `ChildOf` stack `ChildOf` pane.

- "self stack" = the Stack ancestor of the matching terminal.
- "self pane" = the Pane ancestor of that stack (the split target for relative placement).

## Injection

The anchor reaches the MCP server as an argv flag, which is uniform across all three CLIs because each serializes `McpServerConfig.args` verbatim (`crates/vmux_agent/src/client/cli/{vibe,claude,codex}.rs`). Env would only work for vibe; argv works for all three.

Changes:

1. `mcp::resolve(cwd)` → `mcp::resolve(cwd, anchor: ProcessId)`; appends `"--anchor", anchor.to_string()` to `args` (`crates/vmux_agent/src/mcp.rs`).
2. `build_agent_launch` (`crates/vmux_agent/src/launch.rs:6`) takes the `ProcessId` and forwards it to `mcp::resolve`.
3. `handle_spawn_agent_requests` (`crates/vmux_agent/src/plugin.rs:867`) reorders:
   - mint `let process_id = ProcessId::new();`
   - call `build_agent_launch(..., process_id)` so `--anchor <uuid>` is baked into the launch args
   - spawn the agent terminal, then **pin** that id: `commands.entity(terminal).insert(process_id);` so the bundle's own freshly-minted id is overwritten and the subsequent `CreateProcess` uses the same id that was injected as `--anchor`.
4. `vmux_cli`'s `mcp` subcommand gains `--anchor <uuid>` (clap), parsed to `Option<ProcessId>` and passed to `run_stdio`.

## MCP Surface

Three additions; the agent never sees or supplies the anchor — the MCP server injects its own `--anchor` value into the outgoing payloads.

### `open_beside_me`

```
open_beside_me { direction: "right" | "left" | "top" | "bottom" (default "right"), url: string }
```

Splits the agent's own pane in `direction` and opens `url` in the new pane. `url` accepts any page url (`vmux://terminal/...` for a terminal, anything else loads as a browser), same rules as `update_layout`.

### `focus_self`

```
focus_self {}
```

Moves focus to the agent's own stack.

### `read_layout` — `is_self`

`read_layout` gains no new arguments at the agent's level, but the MCP server attaches its anchor to the query. The returned tree marks the caller's stack with `is_self: true`. All other stacks omit it (or `false`).

## Protocol Changes

In `crates/vmux_service/src/protocol.rs` (mirrors the existing `AgentShellMode` pattern — a protocol-local enum, mapped on the GUI side):

- `enum AgentPaneDirection { Top, Right, Bottom, Left }`.
- `AgentCommand::OpenBeside { anchor: ProcessId, direction: AgentPaneDirection, url: String }`.
- `AgentCommand::FocusSelf { anchor: ProcessId }`.
- `AgentQuery::ReadLayout` becomes `AgentQuery::ReadLayout { anchor: Option<ProcessId> }` (was a unit variant).
- `open_beside_me` and `focus_self` return `AgentCommandResult::Ok` (not the tree); the agent calls `read_layout` afterward to learn any new ids (which now marks `is_self`). `AgentCommandResult::Layout` stays in use by `update_layout`.
- Extend `validate_agent_command` to accept the new variants.

In `crates/vmux_layout/src/protocol.rs`:

- Add `is_self: bool` (default `false`) to the read/write `Stack` DTO. Read-only: populated on `read_layout`, ignored by the `update_layout` reconciler (like `is_loading`).

## Implementation

### `vmux_mcp` (`crates/vmux_mcp/src/tools.rs`)

- Add `open_beside_me` and `focus_self` as `McpParamTool` variants (parse `direction` string → `AgentPaneDirection`, validate non-empty `url`).
- `run_stdio` holds the parsed `--anchor` (`Option<ProcessId>`); `dispatch_from_tool_call` gains access to it and stamps it onto `OpenBeside` / `FocusSelf` / `ReadLayout`.
- If a self-tool is called with no anchor (non-agent MCP client), return a structured tool error.

### `vmux_agent` (`crates/vmux_agent/src/plugin.rs`)

- `handle_agent_commands`: new arms for `OpenBeside` and `FocusSelf` that resolve the anchor and write `vmux_layout` requests (below). They respond `AgentCommandResult::Layout(..)` (via the existing `forward_layout_apply_responses` path) or `Error`.
- `handle_agent_queries`: thread `anchor` from `ReadLayout` into `LayoutSnapshotRequest`.

### `vmux_layout`

- `OpenBesideRequest { anchor: ProcessId, direction: PaneDirection, url: String, request_id: u64 }` + handler:
  - resolve `anchor` → terminal → stack → pane (return `Error` if unresolved);
  - refactor `handle_open_in_pane` (`crates/vmux_layout/src/pane.rs:811`) to take an explicit source pane; the existing keyboard/menu path passes the focused pane, this path passes the resolved anchor pane. Reuse `direction_to_split` (`pane.rs:759`) and the existing split machinery;
  - spawn a stack in the new pane and write a `PageOpenRequest` for `url`;
  - on completion, emit the resulting `LayoutSnapshot` (reuse the snapshot/apply-response plumbing) keyed by `request_id`.
- `FocusSelfRequest { anchor: ProcessId, request_id: u64 }` + handler: resolve to stack, set `FocusedStack`.
- `LayoutSnapshotRequest` gains `anchor: Option<ProcessId>`; the read walk in `reconcile.rs` (the function around `reconcile.rs:453` that formats node ids from `entity.to_bits()`) sets `is_self: true` on the stack whose descendant terminal matches the anchor.

### `vmux_cli`

- `--anchor <uuid>` on the `mcp` subcommand → `run_stdio(anchor)`.

### Restart edge — keep `--anchor` consistent with `ProcessId`

Agent PTY restart is handled by `handle_restart_agent_pty` (`crates/vmux_agent/src/plugin.rs:1069`). Today it rebuilds the CLI args from a hand-built `McpServerConfig { command: l.command, args: vec![], cwd: None }` and never rebuilds vibe's env — which misconfigures the vmux MCP server after a restart (a pre-existing bug: the agent loses its vmux tools). The fix: mint the new `ProcessId` **first**, then rebuild the launch via `mcp::resolve(cwd, new_id)` → `strategy.build_args` / `strategy.build_env`, so the new `--anchor` (and a correct MCP config) is applied. GUI restart preserves the id via re-attach and needs no change. This is the one correctness detail that must not be missed.

## Data Flow (end to end)

`open_beside_me { direction: "right", url: "vmux://terminal/" }`
→ MCP server stamps its `--anchor` → `AgentCommand::OpenBeside { anchor, direction: Right, url }`
→ service relay → `AgentCommandRequest`
→ `handle_agent_commands` arm → `OpenBesideRequest`
→ resolve `ProcessId` → terminal → stack → pane
→ split that pane to the right, open `url` in the new stack
→ `AgentCommandResult::Ok` → back to the agent (it calls `read_layout` if it needs the new ids).

## Persistence / Restart

Two identities, two lifetimes, each correct for its scope:

| Identity | Lifetime | Persisted | Job |
|---|---|---|---|
| `SessionId` (sid) | the conversation — survives Mac restart | yes, in the stack url `vmux://agent/<kind>/<sid>` | resume the right agent into the right pane on relaunch |
| `ProcessId` | this process | no (deliberately) | bind the currently-running MCP server to its pane |

- No moonshine change. `ProcessId` is not `Reflect` and does not need to be.
- **GUI restart** (daemon keeps PTYs alive): layout restores, `reattach_terminal_bundle` re-creates the agent terminal with the same `ProcessId`, the still-running MCP server keeps its original `--anchor`, resolution holds.
- **PTY restart** (one agent process restarts): new `ProcessId`, new MCP server; the restart path rebuilds `--anchor` (see above).
- **Mac restart / daemon death** (daemon, PTYs, and MCP servers all die): on relaunch the layout restores, each agent stack still carries `vmux://agent/<kind>/<sid>`, and resume runs through the same `SpawnAgentInStackRequest` → `handle_spawn_agent_requests` path (confirmed at `crates/vmux_agent/src/plugin.rs:684`), which mints a fresh `ProcessId` and injects a fresh `--anchor`. Self-anchoring re-establishes automatically. The durable thing that survived is the sid in the stack url, not the anchor — and that is correct, because the anchor only ever needs process lifetime (the MCP server is a child of the CLI process and shares its lifetime).

## Edge Cases

- **No/unparseable anchor** (non-agent MCP client, or a session spawned before this feature): self-tools return a clear error; `read_layout` emits no `is_self`. Pre-existing live agents get anchoring only after a respawn/restart that injects the flag.
- **Anchor resolves to no live terminal** (agent exited or its stack was closed): `OpenBeside` / `FocusSelf` return `Error("self process not found")`.
- **Duplicate anchor:** impossible — `ProcessId` is a v4 UUID.
- **Agent moved to another stack:** resolution follows it (id is on the content entity), so "self" stays correct.

## Testing

`vmux_agent`:
- `mcp::resolve(cwd, id)` appends `--anchor <uuid>`; each strategy's serialized config carries it (extend the existing per-strategy arg tests in `vibe.rs` / `claude.rs` / `codex.rs`).
- `handle_spawn_agent_requests` pins the minted `ProcessId` and the value in `--anchor` equals the pinned id.
- Resolution: with `FocusedStack` pointing at a different pane, the anchor still resolves to the agent's stack.

`vmux_layout`:
- `open_beside_me` splits the **anchor** pane, not the focused pane (focus a different pane in the test, assert the new pane is a sibling of the anchor pane in the requested direction).
- `read_layout` marks exactly one stack `is_self`, and it is the agent's.
- `focus_self` sets `FocusedStack` to the agent's stack.
- Restart simulation: round-trip through `reattach_terminal_bundle` with the same `ProcessId`, assert resolution still holds.

`vmux_mcp` / `vmux_cli`:
- `tool_definitions()` includes `open_beside_me` and `focus_self`.
- self-tool with no anchor → tool error.
- `vmux_cli/tests/mcp_smoke.rs`: `--anchor` round-trips; `open_beside_me` reaches dispatch.

## Crates Touched

`vmux_layout`, `vmux_agent`, `vmux_mcp`, `vmux_service`, `vmux_cli` (+ its tests). `vmux_core` is untouched.

## Out of Scope (follow-on specs)

- **#1 — run a command beside the agent with read-back.** `run_beside_me { command }`: same anchored split, but spawns a terminal that runs `command` and streams stdout + exit code back via `AgentCommandResult`. Reuses this spec's anchor resolution and anchored-split wholesale.
- Workspace presets / named IDE layouts.
- Proactive auto-arrange (steering the agent to set up its workspace unprompted).
- Editor / file panes (vmux has no code editor today).
