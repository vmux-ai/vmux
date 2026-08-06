# Topology: desktop, phone, and what sits between them

Where each process runs, what talks to what, and which of those links carry the same payloads.

The relay is drawn as a boundary only. Its internals live in the `vmux-cloud` repository; what
matters here is the shape of the two endpoints each side uses.

## The three deployment units

```mermaid
flowchart LR
    subgraph mac["Mac"]
        direction TB
        desktop["vmux_desktop<br/>Bevy ECS · window · GPU compositing"]
        daemon["vmux_service<br/>launchd daemon · PTYs · agent sessions"]
        desktop <-->|"unix socket<br/>ServiceClient"| daemon
    end

    subgraph phone["iPhone"]
        mobile["vmux_mobile<br/>native Dioxus · WKWebView renders"]
    end

    relay(["relay<br/>lives in vmux-cloud"])

    daemon -->|"long-poll /desktop/…/commands<br/>POST /desktop/…/responses"| relay
    mobile -->|"/r/…/api/…"| relay
    mobile -.->|"direct: /api/… on loopback or LAN"| daemon

    style relay stroke-dasharray: 4 4
```

Neither end listens for the other. The desktop polls out and the phone calls in, so a phone reaches
a Mac behind NAT without either opening a port. The dotted path is the direct one — loopback from
the Simulator, or a LAN address — and needs no relay at all.

The relay is **not** a generic proxy. It carries a typed `DesktopCommandKind`, so every endpoint the
phone can reach exists as an explicit variant on both sides.

## Inside the Mac

Two processes, deliberately. The daemon outlives the window, so closing the app does not kill a
running agent or a PTY.

```mermaid
flowchart TB
    subgraph proc1["vmux_desktop — Bevy ECS"]
        ecs["ECS world<br/>Space → Tab → Pane → Stack"]
        plugins["plugins: layout, agent, team,<br/>space, history, terminal, editor"]
        cef["embedded CEF<br/>one browser per pane"]
        ecs --- plugins
        plugins --- cef
    end

    subgraph browsers["inside every CEF browser"]
        wasm["vmux_server.wasm — one bundle<br/>routes on window.location"]
        pages["18 pages<br/>vmux://… · file: · https:"]
        wasm --- pages
    end

    subgraph proc2["vmux_service — daemon"]
        registries["AgentSessionManager<br/>AcpSessionManager<br/>ProcessManager"]
        api["axum API on loopback"]
        registries --- api
    end

    cef <-->|"rkyv over window.cef<br/>binEmit / binListen"| wasm
    ecs <-->|"unix socket"| registries
```

One wasm bundle serves every page; there is no per-page build. `vmux_server::App` reads
`window.location` and picks from a manifest table, so `vmux://team` and `file:` land in the same
binary. Each pane being its own browser means that bundle is parsed per pane, not once.

## One page, two hosts

The point of the shared-page work: a page never names a backend. It emits typed payloads under an
event id and subscribes to ids, and `PageHost` decides what that physically means.

```mermaid
flowchart TB
    page["a page — e.g. vmux_team::page::Page<br/>identical code on both hosts"]
    hooks["vmux_ui::hooks<br/>use_bin_event_listener · try_cef_bin_emit_rkyv"]
    trait["PageHost::emit / listen"]

    page --> hooks --> trait

    trait --> cefhost["CefHost<br/>window.cef.binEmit/binListen"]
    trait --> mobilehost["MobileHost<br/>poll HTTP, re-encode as rkyv"]

    cefhost -->|"crosses a process boundary"| bevy["Bevy ECS"]
    mobilehost -->|"HTTP + SSE"| daemon2["daemon API"]
    daemon2 -->|"broker round-trip"| bevy
```

The broker hop is not incidental. The daemon answers HTTP but the ECS holds the state, and they are
separate processes — so anything ECS-derived costs a request into Bevy and back, which is why
`/api/agents` and `/api/team` are broker commands rather than reads of local state.

## What each link carries

| Link | Transport | Payload |
| --- | --- | --- |
| page ↔ Bevy (desktop) | `window.cef` binEmit/binListen | rkyv; page→host adds a `vmux-bin-ipc-v1` envelope |
| Bevy ↔ daemon | unix socket | rkyv-framed `ClientMessage` / `ServiceMessage` |
| phone ↔ desktop | HTTP + SSE | JSON, re-encoded to rkyv by `MobileHost` before the page sees it |
| desktop ↔ relay | HTTPS long-poll | typed `DesktopCommandKind` / `DesktopResponse` |

The phone never chooses between the relay and a direct connection. It appends paths to whatever
`base_url` pairing gave it, and pairing sets that to `{relay}/r/{device_id}` when a relay is
configured — which it is by default. Direct is the exception now: loopback for the Simulator, or a
LAN address.

That makes `DesktopCommandKind` the real API surface for a phone off the network. Every route the
relay carries exists as a variant on both sides, so an endpoint added to the daemon is reachable
directly but invisible through the relay until a matching variant lands with it.

The asymmetry in the last two rows is the open question: rkyv everywhere would remove a re-encode,
but the phone ships separately from the desktop and rkyv's archived layout is not
forward-compatible, whereas JSON tolerates a field the other side has not heard of.

## Shared crates

What both hosts compile, and what only one does.

- **Both** — `vmux_wire` (plain serde/rkyv models, no Bevy), `vmux_ui` (components, hooks,
  transport), `vmux_chat`, `vmux_start`, and the pages lifted onto the transport so far:
  `vmux_history`, `vmux_service`, `vmux_team`, `vmux_space`.
- **Desktop only** — anything that pulls Bevy: the plugins, the compositor, `vmux_browser`.
- **Not shared by choice** — `vmux_editor`, `vmux_terminal` and `vmux_layout`, which are built on
  DOM measurement or on tiling that a phone has no use for.

The recurring trap when adding to the first list: crates gate their desktop half on
`not(target_arch = "wasm32")`, which is **true on iOS**. The gate has to read
`not(any(target_arch = "wasm32", target_os = "ios"))`, and because Cargo resolves dependencies
before any `cfg` in `src` applies, the split has to exist in `Cargo.toml` too.
