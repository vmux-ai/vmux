# MCP Integration: the workspace is an API

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
the agent process exits. See **[Background Service](background-service.md)** for how that
daemon is supervised and how the work it owns persists.

Every agent is launched **anchored to its own Space** (`vmux mcp --anchor <id>`).
Tool calls resolve relative to that anchor, so a background agent spawns its pages
and terminals in its own space and can't read or disrupt the one you're looking at.

## MCP methods

Agents discover the full set via `tools/list`. The server is registered under the name
`vmux`, so tool names are bare (no `vmux_` prefix) to avoid `vmux:vmux_*` duplication. The
core methods:

- `browser_navigate` — point the active (or a target) pane at a URL.
- `terminal_send` — send raw text to the active terminal.
- `select_tab` — focus a tab by index.
- `create_space` — create a new space and switch to it.
- `update_settings` — set a single setting by dot-path.
- `open_page` — open a page in a new pane beside the requesting agent's own.
- `run` — run a process in a human-visible terminal: the user watches live and can take
  over the prompt, while the agent gets stdout/stderr and the exit code.
- `read_layout` / `update_layout` — fetch the pane tree (stable ids), mutate it, and
  commit it back; Vmux diffs against the live graph and reconciles **React-style** — add
  panes, move stacks, shift focus, in one atomic transaction.
