# Visible, Viewport-Aware Agent Web Research

**Date:** 2026-06-26
**Status:** Design — approved direction, pending spec review
**Branch:** `feat/visible-agent-browser-research`

## Problem

When an agent (Mistral Vibe CLI, running in a vmux agent pane) researches the
web, it uses its own built-in `web_search` and `web_fetch` tools. Both are
invisible to the user and bypass the user's real browser:

- **`web_fetch`** is a client-side `httpx` GET with a generic User-Agent. No
  session/cookies, bot-blocked on many sites (observed: a reddit fetch returned
  37 chars — Cloudflare wall).
- **`web_search`** is a server-side Mistral Conversations API call
  (`tools=[{"type": "web_search"}]`, model `mistral-vibe-cli-with-tools`). It
  runs a full research agent on Mistral's servers: Brave SERP +
  server-side `open_url` page reads + synthesis, returning a finished answer with
  citations. All reading happens on Mistral's infra, **unauthenticated**
  (public pages only), and **the user never sees any of it** — only summary
  lines like "Searched X (7 sources)".

vmux's value proposition is the opposite: the agent works in the user's real,
logged-in, **visible** browser pane that the user can watch and take over. The
agent already has MCP browser tools (`browser_navigate`, `browser_snapshot`,
etc.) but nothing steers it to prefer them, and the tools are ergonomically
weak for a research loop (navigate and snapshot are two separate round-trips;
no viewport/scroll awareness).

The primary motivation is **observability** — the user wants to see what the
agent is doing — with logged-in session access, JS rendering, and bot-block
resilience as secondary benefits.

## Goals

1. Make the agent perform web research in the user's visible browser pane:
   search, navigate, scroll, and read on screen where the user can watch.
2. Make the browser tools ergonomic enough that act + observe is one MCP call
   (critical: Vibe spawns a fresh subprocess per MCP call with a 60s timeout —
   fewer calls matter).
3. Make the agent viewport-aware so its reading is mirrored on screen rather
   than slurped invisibly from the full DOM.

## Non-goals

- Replacing Mistral's search *quality*. We accept slower, more-manual research
  in exchange for visibility.
- Le Chat webview bridge and Claude/Codex parity (follow-up; see Out of Scope).
- A vmux-hosted search backend. The agent searches by navigating to a search
  engine in the visible pane.

## Background: current architecture

- **MCP tools** live in `crates/vmux_mcp/src/tools.rs`. `browser_navigate` is a
  param tool (`McpParamTool::BrowserNavigate`, tools.rs:22-25) mapped to
  `AgentCommand::BrowserNavigate { url, pane }` (tools.rs:100-104).
  `browser_snapshot` is a hand-built `ToolDefinition`
  (`browser_snapshot_definition`, tools.rs:452) dispatched to
  `AgentQuery::BrowserSnapshot { pane }` (tools.rs:689-699).
- **Commands vs queries** are distinct paths in
  `crates/vmux_service/src/protocol.rs`: `AgentCommand` →
  `AgentCommandResult`; `AgentQuery` → `AgentQueryResult`. Navigate is
  fire-and-forget (returns an ack); snapshot returns data. Results are
  serialized back to MCP text in `crates/vmux_mcp/src/protocol.rs:333,377`.
- **Navigate execution:** `AgentCommand::BrowserNavigate` →
  `vmux_agent/src/plugin.rs:671` writes `vmux_layout::BrowserNavigateRequest`
  (defined `vmux_layout/src/lib.rs:184`) → handled in
  `vmux_browser/src/lib.rs:3506`.
- **Snapshot execution:** `AgentQuery::BrowserSnapshot` →
  `vmux_agent/src/plugin.rs:1237` writes `BrowserSnapshotRequest`
  (`vmux_agent/src/events.rs:77`) → produces `BrowserSnapshotResponse`
  (events.rs:83) → `snapshot_response_to_query_result` (events.rs:88). Each
  element already carries `ref`, `role`, `name`, `value`, `bbox`.
- **Load state exists:** `WebviewLoadingStateReceiver` drained by
  `drain_loading_state` (`vmux_browser/src/lib.rs:2013`) with per-webview
  `is_loading` events (lib.rs:2018, 2545, 2559). This is the load-settle signal
  Part B needs.
- **Vibe launch:** `crates/vmux_agent/src/client/cli/vibe.rs` passes only
  `--trust` (+ `--auto-approve` in tests, `--resume <sid>` when resuming) and
  injects MCP config via the `VIBE_MCP_SERVERS` env var. No tool-disabling, no
  steer prompt.
- **Precedent — Claude:** `crates/vmux_agent/src/client/cli/claude.rs` disables
  native `Bash` (`--disallowedTools Bash,Monitor`), allow-lists vmux tools, and
  injects `RUN_STEER_PROMPT` via `--append-system-prompt`
  ("Run ALL shell commands via the mcp__vmux__run tool, which executes in a
  visible terminal the user can watch and take over"). This design mirrors that
  pattern for browsing.
- **Vibe config:** `~/.vibe/config.toml` has a top-level
  `disabled_tools = ["bash"]` and per-tool `[tools.web_search]` /
  `[tools.web_fetch]` sections. `vibe --help` documents `VIBE_*` env vars
  override any config field, and `--agent NAME` loads `~/.vibe/agents/NAME.toml`.

## Design overview

Three parts, shippable together:

- **A — Steer Vibe to the browser:** disable `web_fetch` + `web_search`, inject
  a browser-first steer prompt at launch.
- **B — Navigation returns snapshot inline:** page-loading nav tools wait for
  load-settle and return the snapshot in the same result.
- **C — Viewport-aware snapshots + visible scroll:** snapshots report viewport
  geometry and per-element in-viewport flags; a new `browser_scroll` tool lets
  the agent scroll the visible pane (and returns a fresh snapshot).

## Part A — Steer Vibe to the browser

### Behavior

At launch, vmux configures the Vibe session so that:

1. `web_fetch` and `web_search` are **disabled** (the agent has no invisible
   web path).
2. A **steer prompt** is present in the system context, e.g.:

   > Your built-in web_search and web_fetch are disabled. Do ALL web research
   > in the user's visible browser via the vmux MCP tools so the user can watch
   > and take over: navigate to a search engine with `browser_navigate`, read
   > the returned snapshot, `browser_scroll` to bring content into view, and
   > open results with `browser_navigate`. Prefer this for everything — it uses
   > the user's logged-in session and is visible to them.

### Mechanism — requires a spike

vmux launches Vibe **interactively** (not `-p`), so `--enabled-tools`
(which only disables-others in programmatic mode) does not apply. Candidate
mechanisms, to be confirmed by a short spike against the installed `vibe`
binary, ranked by cleanliness (per-invocation, non-invasive, supports both
tool-disable and system-prompt):

1. **`VIBE_*` env override** beside the existing `VIBE_MCP_SERVERS` — e.g.
   `VIBE_DISABLED_TOOLS` for the tool list, plus a system-prompt field if one
   exists. Cleanest (no files touched) **if** list-field env override is
   supported and a system-prompt field exists. Spike must confirm the exact
   field name(s) and string format (JSON array vs CSV).
2. **Custom agent TOML** at a vmux-managed path, launched via `--agent vmux`,
   defining disabled tools + system prompt. Clean and self-contained if the
   agent schema supports both.
3. **Custom skill** (mirrors the builtin `vibe` skill which sets
   `disabled_tools`), selected per-invocation.
4. **Merge into `~/.vibe/config.toml`** — most invasive (mutates the user's
   global config); fallback only.

**Open item:** exact tool-name tokens for the disabled list. `config.toml`
uses `web_search`/`web_fetch` section names, but the builtin skill used
`disabled_tools = ["webfetch"]`. The spike must confirm the canonical
registry names.

### Scope

v1 targets the Vibe CLI agent strategy (`vibe.rs`). The steer text is authored
generically so it can later be reused for the Le Chat bridge and (adapted) for
Claude/Codex.

## Part B — Navigation returns snapshot inline

### Behavior

These tools wait for the page to settle, then return the snapshot in the same
result:

- `browser_navigate`, `browser_go_back`, `browser_go_forward`,
  `browser_reload`, `browser_hard_reload`
- `open_page`, `open_in_place`, `open_in_new_tab`, `open_in_new_stack`,
  `open_in_new_space` — **only when the resolved URL is a web page.**

Terminal opens and non-web `vmux://` URLs return the existing plain ack (no DOM
to snapshot). `browser_snapshot` remains as a standalone tool for re-reading
after scroll/click without navigating.

### Wait policy

- After issuing the navigation, await the webview's `is_loading → false`
  transition for that pane (via the existing loading-state signal).
- Cap the wait at ~10s (well under Vibe's 60s tool timeout). On timeout,
  capture whatever is rendered and return it with a `timedOut: true` flag
  rather than erroring.
- Then capture the snapshot for the navigated pane and return it as the result.

### Architecture

Navigate is currently a fire-and-forget command; it must become an operation
that completes only after load-settle + snapshot. Approach:

- Introduce a **pending-navigation tracker** that correlates an agent-initiated
  navigation (by pane id / request id) with its subsequent `is_loading → false`
  event and a snapshot capture. The tracker lives where loading state is
  observable (`vmux_browser` / `vmux_agent` plugin systems), consistent with
  the project's message + system integration convention.
- Extend the navigation result path to carry a snapshot payload — e.g. a new
  `AgentCommandResult` variant (or route the affected nav tools through a
  query-style response) that serializes the same shape `browser_snapshot`
  returns. Reuse `BrowserSnapshotRequest`/`BrowserSnapshotResponse` and
  `snapshot_response_to_query_result` for the capture so there is one snapshot
  implementation.
- Tool descriptions updated: "Returns the page snapshot after load — no
  separate browser_snapshot call needed."

## Part C — Viewport-aware snapshots + visible scroll

### Snapshot schema additions

Every snapshot (standalone and the inline ones from Part B) gains viewport
context (full DOM is still returned — option "full DOM + viewport flags"):

```json
{
  "url": "…",
  "title": "…",
  "viewport": {
    "scrollX": 0,
    "scrollY": 1840,
    "viewportWidth": 1280,
    "viewportHeight": 900,
    "pageWidth": 1280,
    "pageHeight": 7320
  },
  "elements": [
    { "ref": "e12", "role": "link", "name": "…", "value": null,
      "bbox": [x, y, w, h], "inViewport": true }
  ],
  "timedOut": false
}
```

- `viewport.*` comes from the page's `scrollX/Y`,
  `innerWidth/Height`, and `documentElement.scrollWidth/Height`, captured in
  the same JS-eval pass that builds the snapshot.
- `inViewport` per element is derived from its `bbox` against the viewport
  rect. This tells the agent (and explains to the watching user) what is on
  screen right now.

### `browser_scroll` tool

New param tool. Scrolls the active or specified browser pane and **returns a
fresh snapshot** (consistent with Part B):

- `pane: Option<String>` — target pane (defaults to active/agent pane).
- One of:
  - `to: "top" | "bottom"`
  - `delta: <pixels>` (positive = down, negative = up; e.g. one viewport ≈
    `viewportHeight`)
  - `ref: "<element ref>"` — scroll the element into view.
- Returns the post-scroll snapshot (same schema as above).

Scrolling drives the real visible pane, so the user watches the agent move
through the page as it reads.

### Steer addition

The Part A steer prompt instructs the agent to scroll through long content so
the user can follow, and that `inViewport` elements are what the user currently
sees.

## Data flow summary

```
agent → MCP tools/call(browser_navigate)
  → AgentCommand::BrowserNavigate
  → BrowserNavigateRequest (vmux_layout)
  → vmux_browser navigates webview
  → [wait] is_loading: false  (cap 10s → timedOut)
  → BrowserSnapshotRequest → BrowserSnapshotResponse (+ viewport, inViewport)
  → command result carries snapshot
  → MCP text result  → agent
```

`browser_scroll` follows the same act → settle → snapshot shape without a load
wait (scroll settles synchronously; snapshot immediately after).

## Error handling

- **No browser pane / wrong pane type** (e.g. target is a terminal): return a
  clear error string, as the existing tools do.
- **Load timeout:** not an error — return partial snapshot + `timedOut: true`.
- **Snapshot capture failure** (JS eval error): return the navigation outcome
  (url/title if available) plus an error note, so the agent still learns the
  navigation happened.
- **Non-web open** (terminal/`vmux://`): unchanged ack; no snapshot field.

## Testing

- **Unit (`vmux_mcp`):** dispatch tests for `browser_scroll` (args → query /
  command), schema presence in `tool_definitions()`, and that nav tools’
  descriptions/return contracts are updated. Mirror existing tests like
  `browser_snapshot_dispatches_to_query_with_pane`.
- **Schema:** serde round-trip for the extended snapshot
  (`viewport`, `inViewport`, `timedOut`).
- **Integration (Bevy, `vmux_agent`/`vmux_browser`):** drive the message flow —
  send a navigate request, simulate `is_loading` true→false, assert a snapshot
  response is produced and carried in the command result; assert the timeout
  path yields `timedOut: true`. Follow the project convention of registering
  message types + systems and asserting on resulting ECS state/messages.
- **Part A:** unit-test that `vibe.rs` `build_args`/`build_env` include the
  disable + steer config once the mechanism is chosen (mirror
  `build_args_disables_native_bash_and_steers_to_run` in `claude.rs`).
- **Manual (end, single pass):** run vmux, give Vibe a research task, confirm it
  searches/opens/scrolls in the visible pane, snapshots arrive inline, and no
  `web_search`/`web_fetch` calls occur.

## Open questions / spikes

1. **Part A mechanism** (the only real unknown): confirm against the installed
   `vibe` binary how to, per-invocation, disable tools **and** inject a system
   prompt for an interactive session without mutating global config. Resolve to
   one of the ranked mechanisms above.
2. **Canonical tool-name tokens** for the disabled list
   (`web_search`/`webfetch` vs `web_search`/`web_fetch`).
3. **Snapshot size:** the full DOM + viewport flags can be large; confirm the
   existing snapshot already bounds element count, and add a cap if needed to
   stay within Vibe's per-call limits.

## Out of scope (follow-ups)

- Le Chat webview bridge parity (`crates/vmux_desktop/src/lechat_bridge.rs`
  bypasses MCP `initialize`; steering there needs a different channel).
- Claude/Codex browser-first steering (they already disable native shells; a
  browsing steer could be added later).
- A vmux-native search tool / SERP abstraction.
