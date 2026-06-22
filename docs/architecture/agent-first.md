# Vmux MCP: the workspace is an API

> Part of the [Vmux Architecture](../architecture.md) overview.

Every action you can take in Vmux — open a page, split a pane, run a command,
reshape the layout — is also an **MCP tool**. Vmux ships a standard
[Model Context Protocol](https://modelcontextprotocol.io) server, so any
MCP-capable agent drives the workspace exactly the way a person does. The
flagship client is the **vibe** CLI; Claude, Codex, or anything that speaks MCP
connect the same way.

## How an agent connects

Vmux exposes a stdio MCP server — line-delimited JSON-RPC (`tools/list`,
`tools/call`) in `crates/vmux_mcp`. Point an agent at the `vmux mcp` command and
it gets the full tool surface; that's all the vibe CLI is wired to under the hood.

The server is a thin front end: it connects to the `vmux_service` daemon over its
Unix socket and forwards each call as a typed protocol message. The daemon — not
the agent — owns the PTYs, terminals, and sessions, so work keeps running even if
the agent process exits.

Every agent is launched **anchored to its own Space** (`vmux mcp --anchor <id>`).
Tool calls resolve relative to that anchor, so a background agent spawns its pages
and terminals in its own space and can't read or disrupt the one you're looking at.

## The tool surface

From atomic mutations to declarative reconciliation:

- **Atomic** — `browser_navigate`, `terminal_send`, `select_tab`, `create_space`,
  `update_settings`.
- **Spatial** — `open_page` spawns a page in a new pane beside the requesting agent's own.
- **Interactive shell** — `run` starts a process in a human-visible terminal: the user
  watches live and can take over the prompt, while the agent gets stdout/stderr and the
  exit code.
- **Declarative reconciliation** — `read_layout` / `update_layout`: the agent fetches the
  tree (with stable ids), mutates it, and commits it back; Vmux diffs against the live
  graph and reconciles **React-style** — add panes, move stacks, shift focus, in one
  atomic transaction.

## Persistence via a daemon

Commands route through a `launchd`-supervised background daemon (`crates/vmux_service`)
that owns the PTYs and agent sessions. Because it outlives the window, shells, long
builds, and agent routines persist across app restarts — an agent can kick off a build,
the app can close, and the output is still waiting when it reopens.

## A privileged bridge behind a scheme gate

Agents reach Vmux over MCP. Web pages reach it over a second, tightly gated path — and
"the workspace is an API" invites the obvious question: can a random website drive it? No.

The host bridge (`window.cef` — the messaging that lets a page read or command the
workspace) only works for **trusted frames**: a page is trusted *iff* its URL is
`vmux://<known-host>/`. `vmux://` is a registered scheme served only from bundled assets, so
no web page can ever *be* at a `vmux://` URL — an unforgeable boundary, checked **per frame**
(an `evil.com` iframe inside a trusted page is still rejected). Anything you browse over
`https://` gets zero bridge access; calls are dropped before they reach the ECS, enforced in
the browser process with a defense-in-depth check in the renderer.

A second layer adds **least privilege** among trusted pages: each message type is bound to
the page that may emit it (`for_hosts(&["history"])`, …), so a compromised Vmux page can't
pivot to another's handlers — and the full Bevy Remote Protocol is locked to the `debug`
page alone. The predicate lives in the patched `bevy_cef_core` (`url_is_trusted_embedded_page`),
unit-tested against `https://evil.com`, `about:blank`, and bare `vmux://`.
