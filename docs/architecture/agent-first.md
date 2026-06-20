# Agent-first: the workspace is an API

> Part of the [Vmux Architecture](../architecture.md) overview.

Vmux exposes its runtime to agents through an **MCP** server
(`crates/vmux_mcp/src/tools.rs`), from atomic mutations to declarative reconciliation:

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

Agents are anchored in their own Space, so a background agent can't read or disrupt the
space you're looking at.

## Persistence via a daemon

Commands route through a `launchd`-supervised background daemon (`crates/vmux_service`)
that owns the PTYs and agent sessions. Because it outlives the window, shells, long
builds, and agent routines persist across app restarts.

## A privileged bridge behind a scheme gate

"The workspace is an API" invites the obvious question — can a random website drive it? No.

The host bridge (`window.cef` — the messaging that lets a page read or command the
workspace) only works for **trusted frames**: a page is trusted *iff* its URL is
`vmux://<known-host>/`. `vmux://` is a registered scheme served only from bundled assets, so
no web page can ever *be* at a `vmux://` URL — an unforgeable boundary, checked **per frame**
(an `evil.com` iframe inside a trusted page is still rejected). Anything you browse over
`https://` gets zero bridge access; calls are dropped before they reach the ECS, enforced in
the browser process with a defense-in-depth check in the renderer.

A second layer adds **least privilege** among trusted pages: each message type is bound to
the page that may emit it (`for_hosts(&["history"])`, …), so a compromised vmux page can't
pivot to another's handlers — and the full Bevy Remote Protocol is locked to the `debug`
page alone. The predicate lives in the patched `bevy_cef_core` (`url_is_trusted_embedded_page`),
unit-tested against `https://evil.com`, `about:blank`, and bare `vmux://`.
