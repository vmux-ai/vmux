<h1 align="center">Vmux</h1>
<p align="center"><b>One architecture. Every platform. Every process.</b></p>
<p align="center">The same Rust, ECS, plugins, and types from UI to service to agent.</p>

<p align="center">
  <img src="icon.png" alt="Vmux icon" width="256" />
</p>

Most "cross-platform" products are several applications wearing the same design system: Electron
on desktop, React Native on mobile, Next.js on the web, another service behind them, and another
API for agents. They may share TypeScript and domain types, but not state, lifecycle, routing,
permissions, extension points, or architecture.

That is why "add one feature" becomes changing the renderer, preload bridge, IPC handlers,
main-process services, daemon protocol, CLI, mobile app, and automation API.

```text
Typical cross-platform stack = Shared types + Separate applications
Vmux                         = Shared application architecture + Platform adapters

Typical boundary = Domain model → UI DTO → IPC DTO → RPC DTO → tool schema
Vmux boundary    = One Rust type → UI · ECS · IPC · QUIC · CLI · MCP
```

```text
Electron product                  Vmux feature plugin
├── renderer state + UI           ├── state
├── preload API                   ├── systems
├── IPC handlers                  ├── UI
├── main-process service          ├── commands
├── daemon protocol               ├── tools
├── CLI + automation schema       └── wire contracts
└── mobile implementation
```

Vmux gives that feature one home. The feature owns its state, behavior, UI, commands, agent tools,
and protocol together. Desktop, mobile, the background service, the CLI, and the MCP server compose
the parts they need instead of rebuilding the feature behind another interface. For each contract,
one Rust type is the source of truth across every boundary it crosses; transport changes, meaning
does not.

**ECS turns integration from dependencies into data.** Attach a component to add behavior. Install
a plugin to add a capability. Swap an adapter to change platforms. Existing features do not need
to know who extended them or be rewired around every new surface.

> Electron gets you to the first window. Vmux keeps every next surface from becoming another app.

## Why it is different

| | Vmux |
|---|---|
| **One model everywhere** | Desktop, mobile, daemon, CLI, and MCP are Bevy apps composed from the same typed feature plugins. |
| **One feature, every interface** | A feature owns its state, systems, page, commands, tools, and wire contracts together. Each runtime selects what it needs. |
| **One contract end to end** | The same Rust type can be an ECS message, UI event, process payload, network request, CLI operation, or agent tool contract. |
| **Composition over integration** | Attach a component to add behavior, install a plugin to add a capability, or swap an adapter to change platforms. Existing features need no rewiring. |
| **Decoupled by construction** | Features meet in the ECS world through typed data and messages, not inside each other's APIs. |
| **Predictable for agents** | Before code exists, its owner, state, behavior, messages, and composition point already have a known shape. |
| **UI-independent** | Long-running work has stable identity, snapshots, subscriptions, and a reconnect protocol. Every UI is disposable. |
| **Native without losing the web** | Rust is the application. Dioxus, Chromium, terminals, and editors are composable surfaces. |

> Electron is cross-platform UI. Vmux is cross-platform architecture.

## One framework, many applications

The Vmux binary is one distribution of the framework, not its boundary. The same feature plugins
can compose a personal Vmux or a different desktop app, mobile app, CLI, service, MCP server, tool,
game, or future web host.

```text
Vmux framework
├── official Vmux
├── your Vmux       selected plugins + your behavior
└── a new product   different composition, same architecture
```

The intended self-hosting loop makes source-level customization practical: ask an agent inside
Vmux to change Vmux, let it create or replace a plugin, build a personal distribution, and switch
to that build through a custom channel. Edit, type-check, build, try, and roll back — without
waiting for the official application to expose another setting or extension hook.

> Neovim made the editor programmable. Vmux makes the application programmable.

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
