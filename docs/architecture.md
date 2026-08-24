# Vmux — Architecture

An agent-first workspace that ships with a browser and an IDE.

Vmux is a native **Rust** host that embeds Chromium through **CEF**. The browser is a
guest surface, not the container. That inversion is the whole thesis: instead of your app
living inside a web sandbox, the web lives inside a native host that reaches straight to
the OS and the GPU.

The host runs on **Bevy**, a data-oriented ECS. Web views composite on the GPU into a
`tmux`-style tiling tree, and agents drive all of it over **MCP** — the workspace is an
API.

---

## Two processes

The window is a client. The work lives somewhere that outlives the window.

```
   ┌── the app you see ──────────┐      ┌── the daemon ───────────────┐
   │  Bevy host                  │      │  vmux_service               │
   │  composites CEF surfaces    │◀────▶│  PTYs                       │
   │  owns the workspace shape   │ unix │  agent + ACP sessions       │
   └─────────────────────────────┘ sock └─────────────────────────────┘
                                          supervised by launchd
```

Close the window and the shell keeps reading, the build keeps building, the agent keeps
streaming. Reopen and the app reconnects, re-subscribes, and replays a snapshot.

launchd relaunches the daemon **on crash, not on exit**, and `RunAtLoad` is false — it is
not a login item. The app starts it on demand.

---

## Three roles

Name the roles, not the devices. Every diagram below holds whether the client is a laptop,
a phone, or a watch.

- **Server** — `vmux_service`. One per host machine. Owns the PTYs and the sessions.
- **Client** — anything that renders the workspace and sends intent back. **Local** shares
  the machine with the server; **remote** does not.
- **Relay** — a rendezvous point, and not optional. A host behind NAT cannot be dialled.
  Its internals live in `vmux-cloud`; what matters here is that it forwards packets it
  cannot read.

```
   ┌─ host machine ───────────────────────┐
   │  local client ──unix socket──▶ server │
   └────────────────────────────────│──────┘
                                    │ QUIC control, held open
                                    ▼
                                  relay          (vmux-cloud)
                                    ▲
                                    │ QUIC, the same UDP port
                           ┌────────┴─────────┐
                           │  remote client   │
                           └──────────────────┘
```

Neither end of the relay listens for the other. Both dial the same UDP port and are told
apart by the hello they open with. The remote client's packets belong to a **second** QUIC
session that terminates on the host, so the relay carries ciphertext — it holds no key and
links no crate that could decode a payload.

Nothing is dialled until Remote is switched on.

### What each link carries

| Link | Transport | Payload |
|---|---|---|
| page ↔ local client | `window.cef` binEmit/binListen | rkyv; page→host adds a `vmux-bin-ipc-v1` envelope |
| local client ↔ server | unix socket | `u32`-length-prefixed rkyv, 64 MiB cap |
| remote client ↔ server | QUIC, one stream per request | rkyv `SharedMessage` / `SharedResponse` |
| server ↔ relay | QUIC control connection | JSON hello, then opaque DATAGRAM frames |

The hellos are JSON and everything after is rkyv, deliberately. rkyv encodes enum variants
**positionally** — a peer one release behind does not fail to decode a reordered variant,
it decodes the *wrong* one. Fine on the unix socket, where both sides ship together. Not
fine for a phone that updates on its own schedule, so the frame that decides whether to
keep talking is JSON.

---

## The ECS model

Bevy splits the atom of a React component into **data** and **behaviour**, with an id
tying them together.

| ECS | What you already know |
|---|---|
| Entity | a primary key, or a `key` prop — a bare id, no fields, no methods |
| Component | plain data bolted onto an id — one `useState` slice, lifted out |
| System | a function over every entity matching a query — `useEffect` / a reducer |
| World | the single store — one in-memory database |
| Message | `dispatch(action)` — systems never call each other directly |

```rust
#[derive(Component)]
pub struct Terminal;

fn focus_active(panes: Query<&Terminal, With<Active>>) {
    for terminal in &panes { }
}
```

Read the query out loud: *for each entity that has a `Terminal` and is `Active`, do this.*

Capabilities **compose** rather than being inherited. A plain web-view entity *becomes* a
shell the moment a `Terminal` component is added — no subclass, no base class. And because
each system declares the data it touches, Bevy runs non-conflicting systems across cores
for you.

### Plugins

One crate, one capability, one `build()`. A plugin bundles its components, systems,
resources, messages and observers and registers them in one call, so a reader sees the
whole feature in one place.

```rust
app.add_plugins((TerminalPlugin, EditorPlugin, ServicePlugin, BrowserPlugin, ...));
```

Plugins do not call each other. Cross-crate behaviour flows through messages and
components. Composition all the way up: components compose an entity, plugins compose the
app.

---

## The layout tree

Ownership is structural — an element's position in the tree *is* its identity.

```
Window
├─ Header                              shared top bar
├─ SideSheet  Left / Right / Bottom    navigator + slide-in panels
└─ Main
   ├─ Space "vmux-ai/vmux" [Active]    visible project workspace
   │  ├─ Tab [Active]
   │  │  └─ PaneSplit
   │  │     ├─ Pane [Active] ─ Stack [Active] ─ Browser
   │  │     └─ Pane          ─ Stack          ─ Browser
   │  └─ Tab
   └─ Space "acme/dashboard"           fully alive, hidden
```

- **Space** — a project container. Exactly one is `Active` and drawn; the rest stay alive.
- **Tab** — a saved pane arrangement.
- **Pane / PaneSplit** — a recursive row/column tree. `tmux`-style tiling.
- **Stack** — several leaves in one pane, cycled like browser tabs.
- **Browser** — the leaf: a web page, a terminal, or an agent.

**The selection invariant: at most one `Active` child per parent.** Focus is found by
walking `Active` down the tree. Small `ensure_active_*` systems check topology every frame,
so if a human or an agent destroys a node the gap heals next frame.

A tab is owned by its Space *entity*, not a loose string key, which sandboxes mutations
inside that Space. Cross-space leaks are impossible by construction — the legacy model
tagged tabs with a detached id and computed selection globally, which let creating a space
corrupt another's panes.

State snapshots to `store.ron` via `moonshine-save`. A schema-version sidecar hard-resets
on an incompatible version rather than loading a broken store.

---

## How it paints

```
   Vmux window
   │
   ├─ content page  ─▶  native windowed CEF view, framed to its pane
   │                    Chrome-parity CPU, no offscreen copy
   │
   └─ the layout    ─▶  one transparent CEF overlay running a Dioxus/WASM app
                        header · URL bar · sidebar · command bar
```

Two backends, swapped at runtime. The everyday macOS path puts a real `NSView` over the
pane, so scrolling costs what Chrome costs. Everywhere else — Linux, the layout overlay,
3D mode — CEF paints **offscreen** into a GPU texture, imported straight into Bevy's `wgpu`
device with no CPU copy. Switching tears the browser down and recreates it.

The UI is Rust all the way down: one source compiles to native Bevy systems and to a
`wasm32` Dioxus app rendered inside CEF. Dioxus is React-shaped — `rsx!` markup, signals
and hooks — styled with Tailwind and shadcn tokens.

"All the way down" is about Vmux's *own* surfaces. The content you open is full Chromium;
any React or Vue app renders exactly as it would in Chrome.

The whole workspace already lives in a Bevy 3D scene, so Player mode tilts your panes into
a spatial view of the same workspace, pages still live. It was never why we reached for a
game engine.

---

## Pages, and the trust boundary

What fills a pane is decided by its URL scheme.

| Scheme | What it is |
|---|---|
| `https://` | the open web — full Chromium |
| `file://` | a local file or directory, in the editor |
| `vmux://` | Vmux's own apps — terminal, history, settings, the layout overlay |

`vmux://` pages are not fetched. They are bundled into the app and served from a registered
custom scheme, addressed by host, so a page loads instantly and offline.

"The workspace is an API" invites the obvious question: can a random website drive it?

```
   vmux://terminal   ─▶  trusted    ─▶  window.cef bridge  ─▶  ECS
   file:///main.rs   ─▶  trusted    ─▶  window.cef bridge  ─▶  ECS
   https://evil.com  ─▶  untrusted  ─▶  dropped
```

A frame is trusted only when Vmux itself served it. Because `vmux://` and `file://` come
straight from disk, no website can ever *be* at one of those URLs — the boundary is
unforgeable rather than merely checked. It is enforced **per frame**, so an `evil.com`
iframe inside a trusted page is still rejected, in the browser process with a
defence-in-depth check in the renderer.

A second layer adds least privilege *among* trusted pages: each message type is bound to
the pages that may emit it, so a compromised Vmux page cannot pivot to another's handlers.
The full Bevy Remote Protocol is locked to the `debug` page alone.

---

## The daemon's registries

Two maps, and "registering" is inserting into one of them.

```
   ProcessManager        ProcessId ──▶ Process        PTY + child + cell grid
   AgentSessionManager   sid       ──▶ SessionHandle  provider stream + history
```

Each exposes the same two surfaces: a **broadcast channel** of updates, and a point-in-time
**snapshot**. Clients hold no state — they subscribe.

**Terminal.** `portable_pty` opens the PTY and spawns the shell; a reader thread pumps its
output through `alacritty_terminal`'s VTE engine into a cell grid. Each poll diffs the grid
by row hash and broadcasts only the **changed lines**. The page applies each patch to
per-row signals, so only those lines repaint. The daemon also owns what should outlive a
frame: OSC 133 command tracking, per-shell integration, copy-mode motions.

**Editor.** `syntect` plus `two-face` — the ~200 grammars from `bat` — highlight line by
line into `StyledSpan`s. The file viewer, the preview and git diffs all emit the same
spans, so code looks identical wherever it appears. No tree-sitter, no LSP. The host ships
only the visible slice of a file and re-slices as you scroll.

---

## Mobile Remote

The desktop is behind NAT, so it dials out and holds the connection open. Three modules own
that on this side; what the relay does in the middle belongs to `vmux-cloud`.

```
   supervisor.rs  ─▶  owns the dialer's lifetime = the Remote switch
   dialer.rs      ─▶  holds the connection open, terminates the inner QUIC session
   dispatch.rs    ─▶  the only place a remote message becomes an action
```

**The switch is a lifetime, not an access check.** Off means no dial, no registration and
nothing to retry. Gating admission alone would leave a desktop registered, retrying
forever, and advertised as one that refuses everyone.

**The dialer** redials on a doubling backoff from one second to thirty. A registration that
stood for a minute restarts the sequence, so a desktop connected for hours reconnects in a
second — and a relay that accepts a registration then tears it down mid-redeploy is not
mistaken for a healthy one. The phone's packets arrive as DATAGRAM frames and are handed to
an inner endpoint that terminates their QUIC session *here*: same certificate, same
`admit()`, same dispatch a phone dialling directly would have reached.

**Dispatch** enforces prompt size, replay dedup and attachment confinement once, rather
than at each of nine handlers.

### Pairing

No CA signs for a Mac, so the host mints its own certificate. The QR deep link carries the
relay endpoint, a 256-bit bearer token, and the certificate's SHA-256. The client pins that
fingerprint and trusts nothing else — narrower than the public root set. A pairing link
without a fingerprint is **refused rather than downgraded**, because there is no unpinned
transport left to fall back to.

Discovery is manual by design. No mDNS, no zeroconf.

### Local and remote are the same client

A page never names a backend. It emits typed payloads under an event id, and a `PageHost`
decides what that physically means.

```
   page (identical code everywhere)
     └─ PageHost::emit / listen
          ├─ CefHost     wasm in a webview  ─▶ crosses a process boundary ─▶ ECS
          └─ MobileHost  native beside one  ─▶ QUIC ─▶ server ─▶ broker ─▶ ECS
```

The asymmetry is not where you would guess. Locally the page is wasm inside an embedded
browser and every message crosses a real process boundary. Remotely the page is native Rust
in the same process as its webview and crosses nothing — the distance is to the *host*, not
to the renderer.

Two current limits: `emit` is one-way on remote clients, and subscriptions are still
polled. `ListAgents` and `ListTeam` are brokered into the local client's ECS rather than
read from the server, so they answer `NoDesktop` until a window is there to ask.

---

## Agents

Every action a person can take is also an **MCP tool**. Vmux ships a stdio MCP server —
line-delimited JSON-RPC in `crates/vmux_mcp` — so any MCP-capable agent drives the
workspace the way a person does.

The server is a thin front end: it forwards each call to the daemon over the unix socket.
The daemon owns the sessions, so work keeps running even if the agent process exits.

Every agent is launched **anchored to its own Space**. Tool calls resolve relative to that
anchor, so a background agent cannot read or disrupt the space you are looking at.

`read_layout` / `update_layout` are the interesting pair: fetch the pane tree with stable
ids, mutate it, commit it back. Vmux diffs against the live graph and reconciles
React-style, in one atomic transaction.

---

## Platforms

| Platform | Role | Status |
|---|---|---|
| macOS | host | primary — packaged, launchd-managed, shipped |
| Linux | host | builds and tests in CI, not packaged |
| Windows | host | not started |
| iOS | remote client | linked in CI, packaged and shipped via `dx` |
| Android | remote client | configured, no platform code yet |

A remote client is strictly a client — the server half of `vmux_service` is compiled out.

### The cfg aliases

The recurring trap was gating a desktop half on `not(target_arch = "wasm32")`, which is
**true on iOS**. A build script emits three aliases so the gate names what it means:

| Alias | Means |
|---|---|
| `web` | wasm32 — the browser bundle |
| `ui` | wasm32 *or* iOS — the pages, no Bevy, no process access |
| `host` | everything else — the desktop app and the daemon |

iOS is native code but is not `host`: it runs the pages, so it is `ui`. Write
`#[cfg(host)]`, never a negation of two target predicates. Cargo's own
`[target.'cfg(...)'.dependencies]` still has to spell the targets out, because dependency
resolution happens before any build script runs.

### Where a crate lives

`crates/app/` is something a user starts. `crates/page/` answers a URL, one per page.
`crates/host/` is runtime with no UI that owns state. Everything else stays flat.

Two traps. `host/` is **not** a layer above `page/` — `page/vmux_agent` depends on
`host/vmux_service`, because those crates cfg-split and a page links only the non-host
half. And the `host` cfg alias is **not** the directory: `vmux_ui` holds host-gated code
while staying flat.
