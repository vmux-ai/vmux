<h1 align="center">Vmux</h1>
<p align="center"><b>Applications that outlive their windows.</b></p>
<p align="center">The agent-first browser and IDE with a durable Rust core, composable surfaces, and one typed interface for humans, agents, and every client.</p>

<p align="center">
  <img src="icon.png" alt="Vmux icon" width="256" />
</p>

Electron made websites installable by making the browser the application. Vmux inverts that
model: the application is a native Rust runtime, and the browser is one surface inside it.

```text
Application = Durable Work + Typed Commands + Composable Surfaces
```

## Why it is different

| | Vmux |
|---|---|
| **Durable** | Terminals, builds, and agents keep running after every window closes. |
| **Composable** | Browser, editor, terminal, native UI, and new capabilities assemble as ECS plugins and surfaces. |
| **Predictable** | State, behavior, messages, and feature ownership have fixed typed shapes, so humans and agents know where new code belongs before writing it. |
| **Agent-native** | People and agents use the same commands and operate the same workspace through MCP. |
| **Cross-platform** | Rust shares the application itself across targets; platform-specific code stays in adapter plugins. |

> Electron makes the browser portable. Rust makes the application portable.

Web remains fully supported through embedded Chromium. It is a renderer, not the architecture.

## Product

- **Co-work with agents** — People and agents build side by side in one shared space — from hands-on pairing to full autonomy, you set the balance.
- **Browser simplicity, tmux power** — Looks like the browser you already know; split, stack, and tile panes like tmux underneath.
- **IDE power underneath** — Keyboard-driven workflows and deep environment control — and agents drive the whole workspace over MCP.
- **3D workspace** — Powered by Bevy. Flip your panes into a live, GPU-rendered 3D scene — same workspace, still interactive.

See [the architecture](docs/architecture.md) for the runtime, ECS, plugin, rendering, and
local/remote model.

## Install

```sh
curl -fsSL https://vmux.ai/install | sh
```

Requires macOS 13.0 (Ventura) or later.

## Development

```sh
# Check prerequisites
make doctor

# Run macOS app
make
```

The first build through `make` in a linked worktree automatically seeds its build cache from the main worktree.

See [Makefile](Makefile) for all targets.

## License

Copyright (c) 2024-2025 Junichi Sugiura

Licensed under the [GNU General Public License v3.0 or later](LICENSE).
