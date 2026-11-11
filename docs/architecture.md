# Vmux architecture

> Flexible by composition. Predictable by design.

Vmux is a composition architecture for cross-platform applications. Features are typed plugins
installed into an ECS world, so a browser, IDE, agent harness, service, and mobile client can
share one model without becoming one tightly coupled program.

This document is a map. Code, tests, typed contracts, and `feature.ron` are the source of
truth. `AGENTS.md` owns repository workflow and coding rules.

## Why Vmux architecture?

### Rich applications, fragmented architecture

Products that span browser content, native UI, files, processes, agents, services, and mobile
clients tend to grow a separate registry, callback layer, state model, and integration path for
each new capability. Flexibility increases, but so does the number of architectures a developer
must understand.

Vmux narrows feature design to one composition model and eight building blocks. New behavior
extends the graph instead of adding another switchboard beside it.

## Applications as plugin graphs

A **plugin** installs one capability: its components, messages, systems, schedule, and any
pages, commands, tools, persistence, or boundary adapters it owns. The **ECS world** is where
those plugins integrate. Systems find typed data and react to typed messages instead of calling
into another feature's internals.

A runtime is a selected plugin graph. Desktop, mobile, the background service, CLI, and MCP
server use the same composition model and differ only in the features and platform adapters they
install.

```mermaid
flowchart LR
    profile["runtime profile"] --> features["feature plugins"]
    profile --> adapters["platform adapters"]
    features -->|install| world["ECS world<br/>state · lifecycle · schedules"]
    adapters -->|install| world
    surfaces["pages · commands · ACP agents"] -->|typed requests| world
    world -->|state · results · effects| surfaces
```

```text
feature = state + systems + UI + commands + tools + contracts
runtime = feature plugins + platform adapters
boundary = typed value + transport adapter
```

`vmux_app` is the platform-neutral composition root. A runtime selects a profile and installs
the feature plugins that profile needs.

```rust
app.add_plugins(
    VmuxPlugin::builder()
        .desktop()
        .git(false)
        .simulator(false)
        .build(),
);
```

Cargo features decide what is compiled. Builder options decide which compiled plugins are
installed. Platform crates then add only their adapters:

| Runtime | Adds |
|---|---|
| `vmux_desktop` | windows, menus, native input, packaging, desktop credentials |
| `vmux_mobile` | mobile lifecycle, pairing, remote transport |
| `vmux_service` | local server, durable processes and sessions |
| `vmux_cli` | argument parsing and one-shot execution |
| `vmux_mcp` | JSON-RPC transport and tool publication |

A feature plugin never installs a sibling feature. Shared composition belongs in `vmux_app`.
Plugin boundaries represent independently configurable, reusable, platform-gated, or
lifecycle-owned capabilities, not wrappers around a few systems. Platform-specific behavior
enters through a platform plugin or adapter at the composition edge.

### Out of the box

| Property | What it means |
|---|---|
| **Runtime composition** | Desktop, mobile, service, CLI, and MCP select plugins and adapters instead of rebuilding features. |
| **Feature ownership** | One crate owns a user capability from UI and state through persistence and published contracts. |
| **Typed boundaries** | Events, messages, and shared values replace callback registries and mirror DTOs. |
| **Observable lifecycle** | Long work has identity and state that systems, UI, tests, and agents can inspect. |
| **Host-authoritative UI** | ECS owns product decisions; pages render projections and emit typed intent. |
| **Predictable changes** | The same small vocabulary tells a human or agent where a change belongs. |

### Built for the agent era

When agents write more code, the bottleneck moves from producing a diff to trusting its design.
Vmux makes ownership, state, operations, UI flow, and runtime composition explicit before the
first line is written. Humans and agents get the same constraints and should produce PRs that
fit the system without inventing a surprising second architecture.

## Learning path

Read this document in order once, then use it as a change map.

1. [Understand Plugin + ECS composition](#applications-as-plugin-graphs).
2. [Learn the eight design primitives](#design-vocabulary).
3. [Trace the default feature flow](#feature-design-flow).
4. [Learn runtime and state boundaries](#system-map).
5. [Apply the durable rules](#invariants).
6. [Route a concrete change](#change-routing).

## Design vocabulary

These eight building blocks are enough to design most Vmux features.

| Building block | Use it for | Rule |
|---|---|---|
| **Crate** | Ownership | One user-facing capability gets one feature crate. It owns the capability from UI to persistence and external contracts. |
| **Plugin** | Composition | The crate's entry point. It installs the capability's systems, messages, observers, child plugins, manifest, and adapters into an app. |
| **Entity** | Identity and lifecycle | Make something an entity when it must be addressed, related, observed, persisted, or removed independently: a page, pane, file, process, or agent session. |
| **Component** | State or capability | Attach small typed facts to an entity. Compose existing components before creating an aggregate that repeats the same data. |
| **System** | Behavior and internal API | Its typed parameters declare the inputs, dependencies, state access, and outputs of one operation. Keep it private and let the owning plugin register it. |
| **Event** | Transient intent or fact inside ECS | Send an event when behavior should happen once without the sender calling the receiver. Target the entity when the behavior belongs to one identity. Use a queued message when ordered stream processing is required. |
| **UiEvent** | UI → host intent | A typed, permission-checked request from a native page. It asks the host to act; it never becomes the authoritative state. |
| **UiState** | Host → UI projection | A typed snapshot or patch derived from ECS. The page renders it and keeps only immediate DOM-local state. |

Treat a system like an internal API endpoint. An event or message is its request; queries and
resources declare its dependencies; component mutations, emitted events, and `UiState` are its
outputs. The function signature is the interface. Other features call that interface through
typed data, not by making the system public or invoking it directly.

### Feature design flow

The default feature flow is:

```mermaid
flowchart LR
    plugin["Plugin"] -. registers .-> system["System<br/>typed internal API"]
    page["native page"] -->|UiEvent| system
    caller["other feature · command · agent"] -->|Event or Message| system
    system -->|query · add · update · remove| entity["Entity + Components"]
    entity -->|derive snapshot or patch| state["UiState"]
    state -->|render| page
```

For a new capability:

1. Choose the crate that owns the user-visible behavior; create a feature crate only when no
   existing owner fits.
2. Make its root plugin the complete composition entry point.
3. Identify the entities whose identity or lifecycle must survive individual interactions.
4. Represent their state and capabilities as components.
5. Design each system as one typed operation over that data.
6. Use events or messages to call the operation without coupling senders to its implementation.
7. Add a `UiEvent` only when intent crosses from a native page into the host.
8. Add `UiState` only for the host-owned state the page must render.

## System map

```mermaid
flowchart LR
    person["person"] --> desktop["desktop app"]
    agent["ACP agent"] --> desktop
    external["external agent"] --> mcp["MCP server"]
    cli["CLI"] --> service["background service"]
    desktop --> service
    mcp --> service
    phone["mobile app"] --> relay["opaque relay"]
    relay --> desktop
    desktop --> host["browser · files · processes · credentials"]
    service --> host
```

- **Desktop app** owns windows, layout, input, native integration, and the visible host ECS.
- **Background service** owns work that must survive a window: processes, agent sessions,
  authorization, and local IPC.
- **Pages** render feature state and emit typed intent. They do not own durable product state.
- **Mobile** runs supported pages as a remote client of the desktop.
- **CLI and MCP** are headless compositions of the same feature behavior.
- **Relay** connects a mobile client to a desktop behind NAT. The inner session terminates on
  the desktop, so the relay cannot read workspace traffic.

## Invariants

These rules matter more than the current module tree.

1. **A feature has one owner.** State, behavior, UI, commands, tools, persistence, and
   contracts live in the feature crate that gives them meaning.
2. **The host is authoritative.** Bevy ECS owns product state and decisions. Dioxus renders
   projections and keeps only immediate DOM state such as uncommitted text drafts, focus, scrolling, and
   pointer geometry.
   Controlled-field edits cross `UiEvent`; rendered values come from `UiState`.
3. **One contract crosses a boundary.** A Rust type may be an ECS message, UI event, IPC
   payload, QUIC payload, CLI operation, or MCP input. Do not create mirror DTOs for each hop.
4. **Runtimes compose features.** Application crates choose plugins and add platform adapters.
   They do not reimplement feature behavior or maintain feature-specific registries.
5. **Features meet through data.** Cross-feature behavior uses typed components, messages,
   entities, and explicit schedule ordering rather than callbacks into another feature's
   internals.
6. **Long work has identity.** Processes, agent sessions, requests, and asynchronous operations
   are entities or components with observable lifecycle, not detached work hidden behind a UI.
7. **Trust ends at the host.** Web content is untrusted. Native pages, tools, remote clients,
   and extensions receive only explicitly registered capabilities.

## Runtime composition

Name the roles, not the devices. Every diagram below holds whether the client is a laptop,
a phone, or a watch.

- **Server** — `vmux_service`. One per host machine. Owns the PTYs and the sessions.
- **Client** — anything that renders the workspace and sends intent back. **Local** shares
  the machine with the server; **remote** does not.
- **Relay** — a rendezvous point, and not optional. A host behind NAT cannot be dialled.
  Its internals live in `vmux-cloud`; what matters here is that it forwards packets it
  cannot read.

```mermaid
flowchart LR
    subgraph host["host machine"]
        local["local client"] <-->|unix socket| server["vmux_service"]
    end
    server <-->|QUIC control, held open| relay(["relay — vmux-cloud"])
    relay <-->|QUIC, the same UDP port| remote["remote client"]
```

Neither end of the relay listens for the other. Both dial the same UDP port and are told
apart by the hello they open with. The remote client's packets belong to a **second** QUIC
session that terminates on the host, so the relay carries ciphertext — it holds no key and
links no crate that could decode a payload.

Nothing is dialled until Remote is switched on.

### Safe storage

On macOS, the desktop process is the Keychain boundary. It owns one non-synchronizing
generic-password item labelled `Vmux Safe Storage`, under service `ai.vmux.safe-storage`
and account `root`. The versioned 256-bit root is never passed to a child process. HKDF-SHA256
derives fixed, versioned domains for the browser, MCP OAuth storage, and each Vault:

```text
Vmux Safe Storage root
├── vmux-safe-storage-browser-v1
├── vmux-safe-storage-mcp-v1
└── vmux-safe-storage-vault-wrap-v1 + vault_id
```

MCP credentials live in encrypted Application Support files authenticated to their account
name. Vault keys live there as envelopes authenticated to the Vault ID and encrypted by the
per-Vault wrapping key. Pre-Safe-Storage browser sessions, MCP credentials, and Vault keys are
treated as absent. The CLI asks the desktop for Vault status through the authenticated service
connection and has no credential or Vault-key broker commands.

The macOS CEF patch pinned by `cef/manifest.toml` exports an embedder-only OSCrypt setter. The
desktop supplies the 128-bit browser-derived key in memory before CEF initialization. During
initialization CEF receives only the derived key and new ciphertext uses a distinct `vm1` prefix.
Chromium's existing raw-key IPC carries only this browser-scoped material to sandboxed browser
helpers, never the root or another subsystem's key.

`cef/manifest.toml` pins the CEF, Chromium, and depot_tools source revisions, patch revision,
release asset, and SHA-256 checksum. `.github/workflows/build-cef.yml` builds the macOS ARM64
artifact on the dedicated CEF runner, publishes source metadata and provenance, and
`scripts/install-cef.sh` verifies both the checksum and the custom export before packaging consumes
the framework.

### What each link carries

```mermaid
flowchart LR
    page["a page"] -->|wry IPC · rkyv| localc["local client"]
    localc -->|unix socket · rkyv| server["vmux_service"]
    remote["remote client"] -->|QUIC · one stream per request| server
    server -->|QUIC control · JSON hello, then DATAGRAM| relay(["relay"])
```

A page reaches its local client over wry's IPC and the `vmux://` protocol, carrying binary
DOM edits one way and rkyv events the other. The unix socket runs `u32`-length-prefixed
rkyv with a 64 MiB cap. A remote client opens one bidirectional QUIC stream per request,
carrying an rkyv `SharedMessage` and `SharedResponse`. The relay's control connection is
the odd one: a JSON hello, then opaque DATAGRAM frames it cannot read.

Remote agent operations are flat `SharedMessage` variants rather than a nested request enum.
The QUIC application protocol is `vmux/11`; changing the positional rkyv wire contract requires
another ALPN version.

Agent commands and queries use the same routed `AgentRequest` envelope. Each operation is a
separate `#[vmux_api::agent]` payload identified by its typed protocol ID; there is no aggregate
query enum. Feature crates define those payloads beside their handlers and return the generic
`AgentQueryResult` transport envelope. The service and MCP runtime route that envelope without
knowing which browser, simulator, or other feature produced it.

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

The entity's creator does not need to predict every behavior it may gain. The system adding that
behavior does not need a reference back to the creator. Components are the contract between them,
so either side can be reused, replaced, or omitted independently. Composition changes by changing
data and plugins, not by editing a growing call graph.

That vocabulary also narrows the implementation search space. A new feature cannot hide
state in an arbitrary object graph or invent a private integration mechanism: its data,
behavior, messages, ownership, and composition point have known forms. Code remains
navigable as the number of features and authors grows, including when the author is an
agent generating the implementation.

A custom `SystemParam` names a coherent capability over the world. Use one when related
queries, commands, or message access would exceed Bevy's system-parameter arity or when it
makes a system materially easier to read and follow, especially when lookup and validation
rules are shared by several systems. Its methods may synchronously inspect or update the ECS
data it contains. The calling system still owns request iteration,
lifecycle transitions, asynchronous task boundaries, cross-feature dispatch, and schedule
ordering. A miscellaneous parameter bundle or a large hidden `run`/`dispatch` workflow is not
a capability and stays as ordinary system logic.

Asynchronous work remains explicit ECS state. A finite filesystem, process, or compute operation
is a task component on the entity whose lifecycle it advances; one system starts it and another
consumes its result. Long-lived runtime-specific I/O, streams, retries, and timers belong to a
transport actor that exchanges typed inputs and outputs with ECS. Coroutine runtimes with ambient
world access are not the default because they hide scheduling and lifecycle state from queries.
Use one only for isolated sequential UI choreography where the sequence itself is the behavior,
never for blocking work, transport ownership, or ordinary domain transitions.
The optional `jobs` capability exposes finite blocking work without Profile or a renderer.
`JobsPlugin` installs bounded `BlockingRuntime` lanes; `JobPlugin<J>` schedules typed immutable
`JobRequest<J>` components on operation entities. Submission captures the `ChildOf` owner even
when a full lane delays execution; retries keep that original ownership. Accepted work
publishes `JobResult<J>` and a payload-free `JobReady<J>` message, and leaves the operation
running. The immutable `JobOwner<J>` snapshot survives publication. Consumers check its `accepts`
guard against the current `ChildOf` alongside operation state and domain revisions, including
ownership changes during the apply frame. The consumer applies the result in `JobSet::Apply`, then triggers
`OperationCompleted` after its domain commit. A worker return alone never means domain success.
Reparenting discards accepted work and uncommitted results; removing an owner drops its children.
Terminal operations release job payloads while retaining their shared identity and outcome.
Full lanes retain requests for retry; unavailable lanes and panics publish typed failures.
SDK `try_spawn` registers the host Waker when a queue is full and wakes it when dequeueing frees
capacity, including when existing work has no completion Waker. Full detection and registration
are synchronized with capacity notification. Hosts reuse a stable Waker across retries so waits
coalesce; native `spawn*` adapters retain their completion-only notification behavior.
Low-level adapters can use `vmux_ecs::blocking` directly: bounded worker pools return entity-owned
`BlockingTask` handles, isolate panics, and invoke a standard `Waker` after publishing the result.
Notification panics are isolated from the worker and its queued work. Pools require positive
worker counts and queue capacities; unsupported configurations report unavailable.
The optional native `WakePlugin` binds the event-loop proxy to `JobWake`; another host can supply
its own waker without linking Winit. Short, long, and process lanes remain independently configurable.
Dropping a handle cancels queued work and discards late results; running adapters cooperate with
the cancellation context. Git keeps separate interactive, background, and single-worker mutation
pools and retains shared-read cancellation and rollback scopes. ACP catalog/history reads and
agent installation use independent shared lanes, retaining requests when a queue is full.
Process submissions distinguish a full queue from an unavailable pool; ACP reports an
unavailable installer pool as an error instead of leaving launch pending.

`vmux_task` supplies entity-owned `AsyncTask<T>` components over Bevy's existing `Task` and
task pools. Inserting a task replaces its token and cancels its predecessor. `TaskPlugin<T>`
collects completion and failure into `TaskResult<T>`; feature systems adopt results through a
guarded command that verifies and claims the current token at application time. Cancellation,
owner removal, replacement, and duplicate adoption therefore cannot apply an obsolete result.
Tool operations retain their domain output and Operation lifecycle; Breadcrumb reads also verify
the current document scope. Installing or binding a task waker requests the first collection pass;
completion then wakes the reactive loop after the result is ready. Tool adapters and inventory
scans hold a shared store lease until their synchronous worker returns, including after cancellation.
Stores for the same canonical path share the lease. MCP manifest transactions and rollback await
that lease without blocking the task pool; OAuth interaction does not hold it. Releasing the lease
wakes its registered event loop, and inserting a replacement task wakes an already-bound consumer.
Canceled work cannot start an adapter or revive its operation state. Running synchronous side effects still require
their adapter's own cancellation and timeout behavior; discarding a task does not roll them back.


System names use their module as context. Prefer `open`, `refresh`, `bind`, `project`, or
`request_definition` inside the owning module over names that repeat the feature, module,
transport, and request type. Keep a longer name only when two systems in the same module would
otherwise be ambiguous.

Not every Rust value is ECS state. Use a component when a value has identity, lifecycle,
independent mutation, observation, or scheduling significance. Use a message when ordering or
cross-system delivery matters. Keep immutable specifications, parsed manifests, serialization
adapters, validated values, and deterministic transformations as plain Rust types. Promote one
to ECS only when the world must address, observe, replace, or schedule it independently; wrapping
a temporary function input in an entity adds indirection without composition.

Keep those non-ECS values in the owning feature's noun module, and put external side effects in a
named boundary module such as `acp/install.rs` or `transport.rs`. Do not create a generic `util`
drawer. `host.rs` and its root plugin remain the composition entrypoint; private systems call the
boundary adapter and project its result back onto ECS entities.

### Plugins

One crate, one capability, one `build()`. A plugin bundles its components, systems,
resources, messages and observers and registers them in one call, so a reader sees the
whole feature in one place.

```rust
app.add_plugins((TerminalPlugin, EditorPlugin, ServicePlugin, BrowserPlugin, ...));
```

Plugins do not call each other. Cross-crate behaviour flows through messages and
components. Composition all the way up: components compose an entity, plugins compose the
app. Large capabilities follow the same rule internally: their root plugin is a table of
contents that composes lifecycle, interaction, presentation, and persistence plugins. A
system stays private beside the plugin that schedules it, so ordering and run conditions
cannot be bypassed by another module.

`vmux_app` is the platform-neutral composition boundary. Applications select a profile and
override feature availability through one builder; individual feature plugins remain usable
without the facade.

`vmux_ecs` defaults to the renderer-free component, identity, operation, and activation core.
The `contracts` feature adds shared wire values; `store` adds schema registration and store
state; `jobs` adds owned blocking work. These capabilities do not require Profile or CEF.
Blocking pools report unavailable on web targets until a web worker adapter exists.
The native `host` bundle is explicit in Vmux consumers.
`VmuxEcsPlugin` installs only primitives and operation lifecycle. The optional `flex` selection
installs the renderer-independent `vmux_flex::FlexPlugin`; the product runtime explicitly requires
that core. `VmuxRuntimePlugin` installs the Vmux Profile, page runtime, and optional
`WindowViewportPlugin` adapter; desktop feature dependencies select that composition through
the application builder.

`vmux_flex` defaults to ECS, math, and Taffy without Window, Winit, or Profile. Existing `Node`,
`ChildOf`, `ComputedNode`, and `Visibility` components remain the geometry contract.
`LayoutContext` is the viewport input component: physical size and DPI. A root uses an explicit
`FlexViewport` reference, its own context, or the unique `PrimaryViewport` fallback. Roots under
non-layout lifecycle owners require explicit input; a missing, ambiguous, or invalid viewport
pauses computation. Paused roots invalidate their cache so resuming adopts changes made while
the viewport was unavailable. The optional `window` adapter projects Window resolution and primary state in
`LayoutSystems::Viewport` before `LayoutSystems::Layout`; consumers run after Layout.
Reparenting refreshes inherited DPI through the subtree and releases obsolete root viewports.
Adding `Node` to a lifecycle owner also refreshes existing descendants as they join its
layout context.
Computed geometry remains physical, with `to_logical` applying inverse DPI. Pane, tab, workspace,
and navigation behavior remains owned by the product layout feature until separately extracted.

Core clocks use `web-time` so activation and operation retention also work on
`wasm32-unknown-unknown` without a native runtime.

Reflection-only declarations use `#[vmux_ecs::types(...)]` on the owning plugin or a unit schema
and `app.register_types::<Schema>()` in its normal build method. This registers reflected types
without a store domain, Save requirements, tracking, or hidden lifecycle systems. Store schemas
reuse the declaration parser and explicitly add their own domain and optional tracking.

Shared data components do not imply persistence. Feature-owned markers and store plugins choose
what is saved: Workspace storage registers its shared fields and includes pages owned by Stacks,
while History explicitly marks its URL and visit records. The same `Cwd`, timestamps, and ordering
components can therefore participate in another store or remain transient without removing a
framework-imposed marker. Identity indexes also expose owner-scoped lookup for direct children;
the same ID in different roots remains valid, while a duplicate inside one root refuses lookup.

`examples/framework-consumer` is an independent Cargo workspace using only public APIs and no
repository patch configuration. Its tests exercise typed requests, scoped identity, operation
completion, optional storage, owned jobs, and Project mutations in a Bevy App without Profile or UI dependencies. CI runs
the consumer and checks its resolved dependency graph along with the minimal core configurations.

`vmux_project` owns renderer-independent Project entities and mutation lifecycles. Its default
plugin consumes typed load/write completion messages and uses shared `Name`, `Cwd`, `Revision`,
identity indexing, and operation lifecycle. The optional `filesystem` plugin implements atomic
snapshot IO; another application can supply its own adapter. The optional `contracts` feature
provides the Project page's read and input contracts without an Editor dependency. The application
builder selects Project explicitly, and Editor requires that core in the composition. Editor owns
the Profile-root binding, folder selection, UI feedback, Git provisioning, filesystem discovery,
and Session contributions. Store tickets bind every result to its root, directory, and generation.
Filesystem jobs queue by canonical directory. Unstarted jobs are discarded when their owner
changes; started atomic IO finishes before a replacement root reads that directory. Backend IO
tasks therefore own their finite lifetime rather than being despawned with a requesting root.

Every product feature appears exactly once in that graph and exposes one crate-level plugin.
That root plugin installs only its own manifest, components, systems, and private subplugins. It
never installs a sibling feature plugin. Cross-feature requirements belong to the `vmux_app`
builder. `vmux_desktop` and `vmux_mobile` add only platform adapters and select a shared profile;
they do not become alternate feature registries.

```rust
app.add_plugins(
    VmuxPlugin::builder()
        .desktop()
        .git(false)
        .simulator(false)
        .build(),
);
```

Cargo features decide what is compiled. Builder options decide which compiled plugins are
installed. Platform crates then add only their adapters:

| Runtime | Adds |
|---|---|
| `vmux_desktop` | windows, menus, native input, packaging, desktop credentials |
| `vmux_mobile` | mobile lifecycle, pairing, remote transport |
| `vmux_service` | local server, durable processes and sessions |
| `vmux_cli` | argument parsing and one-shot execution |
| `vmux_mcp` | JSON-RPC transport and tool publication |

Feature declarations are collected by their typed owner during `Plugin::build`. Repeated
registration is idempotent. `RegistryPlugin::finish` rejects conflicting page, command, alias,
tool, provider, search, CLI, and MCP identifiers, then replaces the declarations with an immutable
`Registry`. Runtime catalogs consume the same feature-owned manifest projections. Registration
after freeze is refused.
The daemon runner finishes and cleans up plugins before its first update, so headless catalogs
observe the same frozen registry as desktop and mobile.

Page manifests are static plugin registration data. Their registration entities exist during
plugin construction so the browser can build the complete embedded-host allowlist before CEF
initializes. Runtime page state and behavior still initialize through ECS schedules.

## Vmux Framework

Vmux Framework is the reusable application architecture that powers Vmux. It is not required
to use Vmux IDE or ACP agents with an existing React, Rust, or other project.

The current framework foundation is:

- typed feature plugins;
- ECS-owned state and lifecycle;
- one contract across UI, IPC, QUIC, CLI, and agent boundaries;
- feature-owned manifests;
- runtime composition profiles;
- platform adapters at the edge.

Vmux is the first complete composition built from that foundation.

Read contracts belong to a capability rather than a page. A client subscribes to typed resource
selectors; the host derives and publishes only that selection, and multiple pages share the
underlying read. The transport carries the resulting projection, not the entire ECS world.
Immediate presentation choices remain local to the client.

Command palette queries cross `UiEvent`, while filtered rows, completion text, prompt targets,
and mode are a host-owned `UiState` projection on the page entity. Dioxus renders that projection;
uncommitted drafts, focus, caret, and IME composition remain local to the DOM.

Tools contributes category navigation rows from registered `tool_providers` metadata through
`CommandBarContribution` children of the querying palette. Shared routes appear once, registered
default-page rows are retained, and activation uses the ordinary `OpenRequest` with its open
target. Category discovery does not register another hosted page or duplicate the sidebar's
provider definitions in the command feature.

A public framework harness, plugin viewer, and composition tooling are direction, not current
API. When they ship, they should inspect and operate the same typed plugin graph rather than
introducing another model beside it.

## Feature ownership

A user-facing capability lives under `crates/feature/`. Its root plugin is the public entry
point and a table of contents for the feature.

```text
crates/feature/vmux_example/
├── src/lib.rs          public plugin
├── src/host.rs         ECS composition
├── src/host/           host-owned behavior
├── src/ui.rs           Dioxus entry point
├── src/ui/             UI-only behavior
├── src/state.rs        host-to-UI state
├── src/event.rs        transient boundary operations
└── src/feature.ron     pages · commands · tools · policy
```

Use only the files a feature needs. Do not create empty layers to match the example.

Split large features by behavior, not by item kind. An approval module should own its
components, messages, systems, and tests together. The root plugin composes those slices.

The macOS desktop bundle is a regular application, with Dock and app-switcher presence;
opening it from another app's fullscreen Space returns to its own window's desktop Space.
Only the embedded Service bundle uses `LSUIElement` for background operation, including in
debug builds.

With native Glass installed, only its backdrop window casts the focus shadow; the transparent
content window disables its own shadow to avoid a doubled outline. Systems without native Glass
retain the normal window shadow.

Native page frames keep their top edge fixed in their parent view's coordinate system.
Flipped AppKit parents use a flexible maximum-Y margin, so restoring the window's height cannot
shift a page down by the size difference before ECS observes another resize.

UI input carries its originating surface. The layout resolves that origin to one native window,
its current Space, and a concrete Stack before applying the request; later systems do not retarget
it through global focus. Menu, shortcut, and agent commands without a surface intentionally use
`FocusedWindow`. Creating another window captures the source Space before focus moves, so startup
state and side-sheet state cannot be read from the empty destination tree.

Space IDs are global identities. Every window may attach its own `Space` view for the same ID;
selecting a space that is not attached creates a fresh local tab and pane tree. Names, project
settings, and history are shared by ID, while tabs, panes, focus, expansion, side sheets, and
other layout state remain local to each window. Only the primary window's views persist.

Each feature has one `feature.ron`. It is the static source of truth for the interfaces that
feature publishes:

Each Stack owns three page components with one reducer. `PageNavigation` is the requested page
identity; projected `PageMetadata` updates immediately for the URL, breadcrumb, tab, and side sheet.
`Current` is the committed surface and its metadata. `Transition` exists only while one incoming
surface advances through loading, paint, presentation, animation, and commit acknowledgements.
Readiness stays inside `Transition` so acknowledgements cannot form invalid cross-request component
combinations, while
paint updates do not mark `PageNavigation` or `Current` changed and therefore do not invalidate
unrelated layout projections. Metadata projection, surface removal, and handoff settlement run only
when their source components change; transition deadlines are checked at a bounded cadence rather
than polling every surface on every frame. A failed or superseded request discards the incoming
surface and leaves `Current` untouched. Request IDs reject acknowledgements from stale surfaces.
Layout projections observe `Current` changes even when the requested metadata stays the same, so
committing a presented surface clears the side sheet's pending-page indication immediately.
Scroll and scroll-end events use asynchronous native delivery, coalesced to the latest position per
element and event kind within one animation frame. Keyboard and pointer events retain synchronous
replies for cancellation; queued scroll events are discarded when their document or interpreter
changes, so native round trips cannot pause scrolling or affect a replaced page.
The selected tab's favicon spinner follows the page after 150 ms of continued loading, avoiding
brief flashes on fast local reloads without delaying the page. Reload then becomes a stop button;
stopping cancels browser loading and rolls an uncommitted handoff back to its retained page.
Inactive restored pages do not animate the selected page's controls. A selected Start page requests
composer focus again when its document becomes ready, including a reused prewarmed page.
Loading feedback includes the reload revision and page URL in its identity, so a completed delay
cannot light up the next reload of the same stack. External-page address rows keep their origin
pill and text; their favicon remains in the tab.
Renderer-only `PrewarmPage` pools prepare Code and Terminal surfaces without intercepting their
page requests. `RendererPool` leases ready surfaces once within the target window; feature plugins
seed minimal context and still own file selection and PTY creation. Claiming a surface clears its
old presentation acknowledgements before the new page's first usable frame appears.
Dormant stack entries keep their page icon until a visible load is requested. Keyboard commands
from the user target the focused page; page-origin commands retain their window scope. Hosted-page
reloads publish loading immediately and clear it on the matching ready handshake or cancellation.
The favicon loader rotates a growing and shrinking arc. Diff loading surfaces retain their full
geometry while showing a short, sparse preview with thinner bars and blank rows; known file headers
remain visible while their bodies load.
Native pages keep the committed surface until the incoming layout has painted and native placement
is ready, then commit immediately without a timed fade. Content pages use the same presentation
lifecycle. Feature page-open handlers create or retarget the incoming surface;
they never despawn the committed surface themselves. Only the page reducer retires it after
presentation commits.

Stack and Tab replacement uses one presentation transition. Selection changes immediately, while
the projection keeps the outgoing branch visible until a ready presented surface exists under the
incoming branch. The same projection drives Bevy layout, CEF visibility, OSR focus, and native
window frames. Settlement removes the transition and retires the outgoing branch in one frame, so
navigation, close, restore, and reuse cannot expose the window background through separate paths.

Page intent commits to ECS immediately: the selected Stack, optimistic URL metadata, breadcrumb,
and side-sheet projection change in the input frame. Surface creation and data refresh continue
behind that committed intent. `UiStatePlugin` retains a small route-keyed snapshot cache for every
typed page state. Reopening a route replays that snapshot before a background refresh, so cached
content remains usable and loading placeholders describe only data that has never existed. Page
readiness is deferred until the current render has registered its state listeners. Incoming pages
remain input-inert but may exchange typed initialization events. The document carries its initial
DOM frame; the interpreter applies it as soon as the root exists, and its first poll acknowledges
that frame. Presentation uses the first usable layout, including reserved placeholders, while
later data arrives as incremental frames. This gives every vmux page one stale-while-revalidate lifecycle
instead of feature-specific loading gates.

Live UI-state frames apply patches in place. Unconsumed increments remain queued across later
frames until their component processes them; constructing a retained cache snapshot is a separate
operation. A controller or catalog update therefore cannot discard diff chunks awaiting a render.
Project catalog workers retain their result until the matching initial detail state exists, so a
fast first-frame completion cannot discard the breadcrumb's project choices.

DOM delivery and presentation are separate acknowledgements. Delivery releases the next edit
batch. Layout, selection, focus and routing respond in the input frame; background data, fonts
and stability timers never hold a layout transition. The incoming page presents its first usable
layout, including reserved loading geometry, at the first paint opportunity. The outgoing surface
covers the handoff until that paint. Initial transcript placement runs before messages become
visible, and later updates preserve the visible row and scroll position. Rehydration invalidates
the old paint acknowledgement.

Native interaction-state updates are repeatable: each update changes the IPC epoch and the
document's inert state without adding persistent JavaScript bindings.
Page-local navigation uses `PageLocationRequest` and `PageLocationUiState`, with the fragment
stored in `PageMetadata`. Fragment-only changes update the hosted URL and the committed Stack
without rebuilding the document or changing its data-cache identity. Pages resolve that shared
location to their own file or section and use the common scroll transport.
Settings, Git file selection, and the editor Outline share `SectionNavigation`: clicks request smooth
scrolling, scrolling publishes the current section, and echoed location updates do not restart
navigation. Notes resolve `#L<line>` to a rendered block and measure the heading at the viewport
edge in one DOM request. Outline following preserves the editor caret. Page routes are declared once in `feature.ron`.
`PageManifest` supplies the default breadcrumb title, translation key, icon, and destination;
the registered manifests supply the page-switch menu. Pages publish only typed `PageAddress`
data through `AddressContribution` or `PageAddressPublish`: the current route, project and session
choices, directories, symbols, and their intents. `vmux_layout` renders the active surface with
one `AddressTrail`. No page component, callback, context, or mounted slot crosses into Layout.
Hosted subtree registration also extends its route metadata's ownership. During navigation,
Layout reads the incoming surface and existing breadcrumb choices supply the selected label and
icon immediately, before the page publishes its enriched address.
The URL row contains routing breadcrumbs and page-type icons. Project and branch selection stays
in the Space card; Code and Session trails contain their viewed file or Session without repeating
that workspace context. Code and Git use `file://` and `git://`; their root
bookmarks resolve the working directory selected in the owning Space card through shared
`page_context::Context`. Git view and commit or stash selections use query parameters. Its
checkout follows the owning workspace, and old `vmux://git/` routes normalize before rendering.
Search, filters, Details, Add, Create,
Resume, Start, and Checkout render inside the page through `PageToolbar`, with local callbacks.
Settings and manager pages share `WorkspaceLayout`: navigation opens by default on desktop,
and its toggle animates the sidebar and icon. The content-column toolbar matches Editor and Note:
the common `PageSidebarToggle` sits on the left and fills its left segment while open.
Sidebar search uses the same top-left `ManagerSearchRow` and `ManagerSearch` as Editor and Git.
Settings keeps that layout mounted while loading or showing no search results, so the query
remains editable and clearing it restores the sections without replacing the search control.
The Settings shortcut uses the shared stack open request and a settings reuse group, focusing
the existing page with its current section instead of opening another stack.
Settings section icons and the active viewport section update the address trail and URL fragment;
command-bar search providers resolve their favicons through the common favicon source.
Git's document, styles and UI transport share the `git://` origin. Code uses `file://` for
navigation and `vmux://files/` for the native renderer: WKWebView reserves the built-in file
scheme and rejects a custom handler. Page routing and renderer asset origins are separate
metadata; an asset origin never becomes a bookmark destination.
The command bar displays decoded URL path segments, including embedded file paths. Edits retain
their original segment boundaries; navigation and whole-URL copies use the canonical encoded URL.
Plain URL drafts retain their exact spelling while typing and deleting; presentation never adds
a root slash or normalizes an unfinished authority.
Ordinary bookmark clicks reuse an existing Stack or open one. Cmd-click, with or without Shift,
opens a new Tab after resolving any project context.
Fragment updates retain the page's breadcrumbs instead of replacing them with the generic route.
`PageAddressRequest` validates the source window, active surface, revision, and URL before sending
`AddressIntentRequest` to the page. Navigation opens in the owning Stack; page-local selection
uses a versioned `AddressEffect` consumed by `PageBreadcrumbs`. Effects bind to the source URL,
owner, and address revision and are removed from retained snapshots to prevent replay on reload.
Layout overlays distinguish DOM popovers from panels. Popovers keep native pointer capture,
while their DOM decides outside-click dismissal; static header and sidebar rectangles cannot
close a popover before an item below them receives its click.
Popup state belongs to the Layout that renders the breadcrumb and uses its existing overlay
capture until dismissal or disposal. Hiding the window header leaves the full page working area.
Space-card Project and Branch selectors share hover opening. Their menus align with the row
on its right, falling back to the left at the viewport edge. A first click after hover keeps
the menu open; the shared leave delay allows the pointer to cross the gap. Each opening advances
an activation revision, so delayed closes and listeners from an earlier opening cannot dismiss it.
Path overflow collapses whole directories using measured widths and keeps the filename intact.
Git's branch-file filter sits above its file tree, with page-top spacing shared by the tree and
diff. Side-sheet cards share their outer width; the Space card keeps an inner right inset.
Long titles and branch labels fade at the edge,
and paths retain their filename with a leading fade. Hover exposes the full label.
Session tabs and side-sheet rows share their Session identicon, with the agent logo in a small
corner badge. Address identicons use the same size as the adjacent navigation icons.
Agent thumbnails contain their logo inside the circular avatar surface, including its border.
Session breadcrumb controls fit their text, leaving the remaining address surface available for
URL editing. A hover control copies the active URL. Session selection requests the full page-open
lifecycle in the current Stack so its Session target and runtime follow the selected URL.
Code bookmarks resolve their directory from the active worktree, owning project, or cwd and open
its README or entry file; directory favicons identify the Code page. Layout and address state flush before native
presentation in the same frame; the final flush retains updates arriving later.
Project rows carry an identity separate from their checkout path. Space cards, composer chips,
project pages and Session metadata use that identity for identicons; branch icons use the branch
name consistently. Moving a Project between worktrees preserves its icon.
Spaces use their immutable Space ID for the same identicon in lists, menus and cards. Renaming
changes only the display name, preserving tab membership and the icon. Commit selections use the
full object ID for identicons in both the URL trail and its menu. An unselected Commits view picks
the latest commit after its initial catalog arrives and writes that selection into the URL;
explicit object IDs remain selected independently of the current branch's history.
Project detail trails reuse the asynchronous Project catalog for their switch menu. Cached choices
remain available across project navigation, and completing a catalog refresh preserves the detail
view rather than replacing it with the Project list.
Search and page controls share `page::PageToolbarRow`, a 32 px row. Collection sidebars use the
same default width as Code's file tree; their search and navigation icons share a 12 px left inset.
`manager::ManagerSearch` fills that row with 13 px text and a subtle focus surface, retaining its
width and height during input. Editor tabs remain beside the sidebar search. The sidebar's scroll
region starts below it. Filter menus use the shared popover layer and may extend beyond the
sidebar without changing the search row, editor tabs, or content positions.
Git's initial skeleton and loaded dashboard share the same split layout, sidebar width and
toolbar rows; loading does not add a second breadcrumb header inside the page.
Initial page subscriptions use `DeferredSend`: blocked surfaces retain the latest request by
event ID and replay it only for the current navigation and source epoch. Navigation changes
discard old requests; effects observe the navigation revision and reissue their current data.
Ordinary input remains blocked until the surface is interactive.
Tab presses and Stack card clicks select immediately; host projection reconciles their local
selection. Earlier acknowledgements cannot replace a newer pending selection.
UI motion is feedback rather than a completion condition. Selection, URL, focus, and the target
layout commit in the input frame; no request waits for an animation, and fresh input supersedes
an in-flight transition from its current visual state. Native layout and shared UI animations
use `vmux_ui::motion::Motion`: 80 ms for feedback, 100 ms for content, and 120 ms for layout, with
a 160 ms ceiling enforced at DOM animation transport boundaries. Reduced motion uses the same
state changes without visual interpolation. Same-document navigation and input-epoch changes
retain in-flight edits and existing component scopes. The current document acknowledges a frame
by its unique sequence after identity resynchronization, while old input and DOM requests remain
rejected. Actual document replacement or failed edits still rebuild the document. Page replacement
does not animate the whole surface or translate a shared composer; shared sidebar rows stay in
place when page selection changes. Session detail-panel visibility belongs to the page shell,
and its geometry remains reserved while the selected session loads.
Session viewports remember their scroll position and loaded-history start on the owning Session.
A floating composer overlaps the transcript's lower edge, with an input-inert surface gradient.
Transcript end padding keeps the latest message clear of the composer and its fade.
A revisit restores that history window and reader position; the first visit follows the tail.
Initial tail following remains active until user scroll input. Placeholder and zero-height scroll
events neither finish placement nor replace the remembered reader position.
Viewport updates validate the originating page URL and Session ID; where a canonical Session ID
is available, transcript readiness is bound to it. These viewport records are transient.
Discrete tabs and search widths change immediately. Removing a tab retains an inert, identifier-free
copy of that specific tab for an 80 ms fade. Remaining tabs move from their previous rectangles
within the 160 ms budget, while selection and the actual layout update immediately. Reduced motion
skips both effects. A cached Stack returning from a hidden,
zero-size frame restores its final rectangle immediately. Collapsible content may interpolate height
within the shared layout budget. The editor sidebar keeps one panel icon in both states.
Its width, short slide, and opacity use the
120 ms layout budget with a fixed inner width; pointer resizing and reduced motion skip width
interpolation, and opening never waits for page data. Cached pages remain usable during refresh;
new data occupies a reserved skeleton region, and page replacement keeps the outgoing surface
until the incoming snapshot paints. Tests exercise fresh input during motion, stale paint rejection,
and retained-page handoffs instead of asserting animation class names. URL-bar background and
padding stay independently clickable and expose the URL on hover; breadcrumb controls keep their own hit targets and events.
Address-path measurements live in an inert, invisible row outside normal layout, so hiding a
directory never removes its width measurement or changes the available inline space. Zero-size
notifications from detached surfaces and subpixel noise retain the previous layout.
Dropdown, Select, and context-menu state belongs to their respective controls; their popup
presentation shares `vmux_ui::components::popup::Surface`. The page interpreter raises, positions,
and focuses new popovers in the mutation frame, before mounted callbacks. Menus fade in from
transparent with a small vertical movement in 100 ms; closing releases interaction immediately and retains
an inert, identifier-free snapshot for an 80 ms fade. The snapshot retains typography and scroll
offsets. New input can discard it, and reduced motion skips interpolation.
Selected dropdown entries are revealed inside their own menu before its first paint. Mounted setup
and later renders preserve that position; fresh pointer, scroll and text input own it thereafter.
Command bars, menus, selects, tooltips and dialogs share `popup::Dismissal`. Document deactivation
closes them through their owning state; moving focus between fields leaves them open. The macOS
application resignation notification dispatches the same lifecycle to every hosted page, so native
focus transfers cannot leave a popup behind. Command-bar deactivation never restores keyboard
focus to a background window.
Collection and Code search reuse `manager::ManagerSearch` at the top of their sidebars. Code keeps
its file tabs and view controls in the first page row; search never reserves an empty row above it.
Search has no surrounding field frame and fills its reserved sidebar row. Code keeps regex and
matching options in a menu, and root-folder headings own the Explorer hover controls. Markdown
outlines publish after document metadata, so a navigation reset cannot discard newly parsed headings. File-list
placeholders retain the real 22 px row height with file outlines and short name bars. Rendered-note
carets share a thin, font-sized style without a surrounding shadow.
Note editing keeps the rendered block in normal flow and centers the source overlay inside that
reserved area, so extra source lines occupy the space above and below without moving adjacent blocks.
Session view and category controls live in the page toolbar. Section headings support right-click
and overflow menus for bulk archive, restore, and folding.
Activation stamps preserve operation order across shared ancestors even within one clock tick;
stack navigation consumes a batch against one advancing cursor and excludes retiring targets.
Sidebar choices and stack shortcuts share that activation path, including nested pane ancestors.
After layout and native placement, a completed surface with matching generation, navigation,
bounds, and full native opacity can finalize selection, input ownership, and stacking in the same
frame. New handoffs retain their existing readiness and animation conditions.
Session rows use memoized repository lookup and group metadata; rendered Wry frames join the
native pointer registry so native scrolling does not generate unnecessary layout wakes.
History swipes use a shared horizontal-wheel recognizer. Horizontal scrollable ancestors consume
movement until their boundary; zero-delta starts and small initial axis noise do not cancel a swipe.
A transient arrow at the corresponding viewport edge tracks gesture progress and fades on commit
or cancellation. Its isolated, input-inert layer does not participate in page layout or delay navigation.
Source-bound requests reuse host history before browser history. The dedicated CEF gesture bridge
keeps general-page IPC permissions unchanged; both backends require one matching, recent native
wheel claim for the current surface, window, frame, and direction before moving history.
Accepted lifecycle batches also carry the current document revision into paint state; an old
mount notification discarded during an interaction-epoch change cannot leave the first paint
waiting on revision zero.
Dirty pages flush against the native document's own parked poll; asynchronously mirrored frame
telemetry does not gate delivery. Start keeps prepared replacement surfaces so closing the last
view can reuse a loaded page while its workspace context refreshes.
Rebuilding the same document preserves its guarded host-state listeners. Only replacing the
page component clears those bindings, so an input-epoch change or frame retry cannot leave a
mounted page subscribed to a registry that no longer exists.
Full document recovery replaces the VirtualDom and reapplies its supplied contexts. Rebuilding
an already-mounted VirtualDom leaves prior component scopes alive, so their queued updates must
not be allowed to address nodes in the replacement document.
Before replacing those scopes, the page captures their retained typed UI state. The replacement
document starts from that state, so an interaction change with an unacknowledged frame cannot flash
loaded content back to skeletons. Capturing happens only during recovery; replacing the page creates
a fresh state store.
The requirement to replace the document is latched outside the transient DOM request queue. A later
navigation or input-epoch update may clear transient requests, but the full rebuild still carries one
remount stamped with its current identity. Full rebuild edits never append into the previous tree.

Custom-scheme asset requests enter the response queue in the same update that receives them.
Pending responses wake the reactive host loop and keep the CEF pump responsive until delivery;
after delivery, both return to their normal idle policy.

Start's branch lookups share one pending read per project across their requesting surfaces.
They run on the bounded Git background workers, so synchronous Git subprocesses cannot occupy
the asset loader's shared IO task pool during startup.

```mermaid
stateDiagram-v2
    [*] --> Stable
    Stable --> Loading: open
    Loading --> Painted: loaded + painted
    Painted --> Presented: native surface visible
    Presented --> Transitioning: animated surface
    Presented --> Committed: immediate surface
    Transitioning --> Committed: animation finished
    Committed --> Stable: atomic commit
    Loading --> Stable: failed or superseded
    Painted --> Stable: failed or superseded
    Presented --> Stable: failed or superseded
```

**The selection invariant: at most one `Active` child per parent.** Focus is found by
walking `Active` down the tree. `LastActivatedAt` records MRU order and selection intent; one
layout reducer projects it into `Active`. Consumers use that projection and consult MRU only while
repairing an incomplete restored tree. Native adapters never infer selection independently. Small
`ensure_active_*` systems run when selection or topology changes, so if a human or an agent destroys
a node the gap heals in the same update without scanning the tree on unrelated frames.

The shared manifest plugin parses the file once and registers typed metadata in ECS. Runtime
catalogs query that data. Application crates do not enumerate feature-owned commands or tools.

State snapshots to `store.ron` via `moonshine-save`. A schema-version sidecar rejects newer
versions unchanged; older, unversioned, or incompatible stores are copied to unique backups
before starting a new workspace. Backup failures refuse startup rather than lose the original.
Both outcomes show a localized warning with the preserved path. Exit waits for pending
autosaves and writes the final snapshot synchronously before releasing the data lease.

World persistence uses named type-registry domains. A persisted component or resource registers
once with its domain; that metadata produces the component and resource allowlists used by saves.
The workspace domain tracks component changes only on entities marked `Save`, including removals
and despawns. Its hierarchy tracker compares names and relationships by value so reinserting an
unchanged hierarchy cannot restart autosave. Dirty state saves after 500 ms of quiet, with a
60-second deadline for continuous changes; coincident deadlines write once, and clean workspaces
do not write periodic snapshots. Specialized domains such as bookmarks keep their own entity
scope, dirty policy, path, migration, and seeding lifecycle.
Automatic saves capture an immutable world on the main thread, then serialize and atomically
replace the file on the I/O pool. An in-flight save keeps later edits dirty; completion wakes the
app and never clears newer changes. Explicit saves finish an earlier write to the same path first.
Failed writes retain the changes and retry on the 60-second deadline; schema sidecars follow
successful writes. Restored session children containing only a timestamp and parent reference are
discarded after backing up the store; conversation content and live children remain intact.
`vmux_ecs::store` shares type-registry domains and entity-owned `State` across workspace and
bookmark stores. A store schema declares its component and resource types with `#[vmux_ecs::store(...)]`;
`domain = WorkspaceStore, track; ...` contributes to the existing workspace domain and installs
deduplicated component change tracking. Other schemas keep their own domains and opt into tracking
explicitly.
`SchemaPlugin` registers the declared types and provides a shared, cached save filter. Loaded, dirty,
and saving revisions stay distinct: a completion acknowledges only its captured revision, and failure
returns that revision to the dirty set. Restore progresses
through materialization and resolution; workspace saves wait for `LayoutRestoreCommitted`.
Store paths, snapshot formats, schema sidecars, validation, and recovery remain feature-owned.
`vmux_ecs::revision` owns the revision value and committed-record metadata shared by Session
mutations and store state. Session payloads remain feature-owned; record bundles compose their
operation UUID, base and committed revisions, timestamp, and owning entity. Existing reflected
Session type paths and record contents remain compatible. Durable mutation increments fail on
overflow; store counters retain their saturating behavior and acknowledge only captured saves.


Workspace loads validate Space, Pane, and Session IDs and the ownership graph before replacing
live entities. Space roots may reference the runtime window outside the snapshot. Failed loads
discard partial entities, preserve the store and schema sidecar in a recovery backup, and let
startup create a usable workspace. Saves cannot overwrite the failed store until that backup exists.

Identity indexes reuse existing ID components within a marker scope. ID or marker changes and
removals rebuild the index; duplicate IDs refuse lookup until the ambiguity is removed. Session
conversation and composer lookup share one index, refreshed after deferred creation and service
mutations before either consumes the Session. These indexes are runtime state, not stored identities.
External identities index the existing provider and provider-ID components together within a
marker scope. Provider stage and archive overrides share this lookup, reject ambiguous identities,
and reconcile component changes and removals. IDs from different providers remain independent;
stored provider components and canonical Session identity keep their existing representation.


Finite product work uses `Operation::bundle` for a stable UUID, kind, and initial state. Project
selection, tool operations, Git changes, and extension installation share the same lifecycle and
terminal cleanup. Feature tasks execute the work; `OperationCompleted` records its result. Results
remain inspectable as `Succeeded`, `Failed` with an error, or `Canceled` for five minutes. Completed
feature payloads are released without removing the operation identity, and late completion cannot
revive canceled work. Reads and long-lived transport actors keep their own task lifecycles.
Project selections additionally reference their Tab and capture `OperationTargetRevision`;
`accepts` rejects a result after selection changes. Cleanup guards dispose of discarded checkouts.
Project creation, cloning, import, and deletion and Explorer file mutations use the same lifecycle.
Their task entities carry `OperationTarget` and belong to the requesting page. Feature systems
apply results and remove task state, while `OperationCompleted` retains success or failure for
inspection. Canceled work cannot apply a late result; project publication still checks its captured
page revision before replacing the current view.


The normal page path is one direction around a loop.

```mermaid
sequenceDiagram
    participant UI as Dioxus page
    participant Host as host ECS
    participant Boundary as service or platform boundary

    UI->>Host: typed input request
    Host->>Host: validate and update entities
    Host->>Boundary: typed operation when needed
    Boundary->>Host: result or state change
    Host->>UI: UiState snapshot or patch
    UI->>UI: render and perform DOM-local effects
```

Everything the page loads arrives over a `vmux://` custom protocol registered on **that
webview**: the document, the rendered frames, the verdict on an event it is blocked on, and
every asset any of those reference. Native pages handle the same reload requests as CEF pages;
the native adapter reloads its document, and the document-load handshake advances its revision
and restores the current host state before accepting page input. Hard reload uses WebKit's origin
reload on macOS to bypass its cache. `Route` enumerates that list, so adding one is a variant
rather than another branch in a chain of string tests.

Current values use `UiState`. One-shot notifications use typed events. Versioned effects
cover DOM actions such as focus or scroll when they must be requested by the host.

## ECS lifecycle

Components compose capabilities. A system depends on the data it queries, not on the code that
created that data. This lets a feature extend an existing entity without importing or rewriting
its owner.

Systems stay private beside the plugin that schedules them. Tests install the plugin rather
than registering private systems under a different schedule.

Asynchronous work remains explicit:

- finite work is represented by a task component and a consuming system;
- long-lived I/O belongs to a named boundary actor;
- requests, results, retries, and completion remain observable ECS state;
- blocking filesystem, process, and network work stays outside the schedule.

## Pages and trust

Vmux renders two kinds of page:

- **Web pages** are arbitrary Chromium content.
- **Native pages** are Vmux features rendered through the page host.

Native page URLs and permissions come from feature manifests. The browser accepts native
capabilities only for registered page hosts. A page cannot choose another feature's event or
state channel by constructing a string.

The host validates page identity before decoding a payload. Browser extensions and remote
clients enter through similarly narrow adapters. General browser content never receives the
Bevy world or unrestricted native APIs.

## Layout ownership

Position in the tree defines ownership. Each native window owns a complete tree. Spaces
identify projects; tabs preserve arrangements; splits tile panes; stacks hold page history.
Global actions resolve through the focused window and then through active descendants.

`vmux_pane` owns the canonical Pane, split, size, identity, and spawn-sequence components without
Window, renderer, or Profile dependencies. Existing reflected paths under
`vmux_desktop::layout::pane` remain the persistence contract. Its read-only Hierarchy capability
resolves leaves and neighbors within an explicit root; SelectRequest refuses split targets,
missing targets, and stale ownership before shared activation and SelectionChanged publication.
Native directional focus forwards its selected target into PaneSet::Select inside the existing
layout request set, so focus projections observe the result in the same update.
Geometry/bundle helpers and wire-direction conversion are optional capabilities. Pane components
do not require Save by default; the optional pane StorePlugin declares its schema and the native
Tree/Identity/Resize adapters explicitly retain Workspace save requirements. Native layout keeps
ID allocation, sequencing, geometry synchronization, pointer/cursor behavior, and close policy.

Only test a window's drag region with coordinates from that window. Never compare child-window
coordinates with the owning window's region.

---

## Pages, and the trust boundary

What fills a pane is decided by its URL scheme.

| Scheme | What it is |
|---|---|
| `https://` | the open web — full Chromium |
| `file://` | a local file or directory, in the editor |
| `vmux://` | Vmux's own pages, served from bundled assets |

`vmux://` pages are not fetched from a server. They are answered from embedded assets by a
custom protocol handler, so a page loads instantly and offline.

`vmux_api::VmuxRoute` is the single parser and canonicalizer for those URLs. Browser dispatch,
history, layout placement, native-page ownership, and agent routing compare its host and path
boundaries instead of string prefixes. Legacy aliases are accepted at ingress and rewritten to
their canonical route before they enter page state.

"The workspace is an API" invites the obvious question: can a random website drive it?

```mermaid
flowchart LR
    v["vmux:// page"] -->|trusted| bridge["host bridge"]
    f["file:// document"] -->|trusted| bridge
    e["https://evil.com"] -->|untrusted| drop["dropped"]
    bridge --> ecs["ECS"]
```

A frame is trusted only when Vmux itself served it. Because `vmux://` and `file://` come
straight from disk, no website can ever *be* at one of those URLs — the boundary is
unforgeable rather than merely checked. On the remaining CEF surfaces it is enforced **per
frame**, so an `evil.com` iframe inside a trusted page is still rejected, in the browser
process with a defence-in-depth check in the renderer.

Native pages get the boundary for free in a different way: their components run in the host
process and their webview is handed only its own `vmux://` protocol, so there is no bridge
for an arbitrary URL to reach in the first place.

A second layer adds least privilege *among* trusted pages: each message type is bound to
the pages that may emit it, so a compromised page cannot pivot to another's handlers. The
full Bevy Remote Protocol is locked to the `debug` page alone.

### Browser extensions

`vmux_extension` owns package metadata, installation, permissions, enabled state, catalog ECS,
runtime preparation, bridge transport, service workers, browser API projection, extension
popups, web-store injection, conformance tooling, and the manager page. `vmux_browser` exposes
only generic CEF primitives and lifecycle boundaries used by browser-backed features; it neither
depends on nor installs extension behavior.

An installed extension is never handed to CEF as it shipped. Vmux copies the package into a
generated runtime directory and patches the manifest so the service worker becomes a stable
`vmux_sw.js` that imports Vmux's bridge ahead of the extension's own worker. The package
itself stays immutable, so re-installing an extension or changing the bridge is a fresh
generation rather than a patch stacked on a patch.

That stable entry point has one consequence worth knowing. Chromium decides whether to
re-fetch a worker by comparing the bytes of its script, and `vmux_sw.js` is three
`importScripts` lines that never change — so a profile can keep serving the *previous*
package's worker from its script cache after the extension underneath it has been replaced,
and that worker then fetches chunks which no longer exist on disk. Vmux therefore
fingerprints the prepared runtimes and clears the profile's service-worker script cache
itself whenever that fingerprint moves.

Extension API requests cross from CEF to Bevy on socket threads while the app's event loop
is reactive. Enqueuing one wakes winit so a `tabs` or `windows` response never waits for the
next mouse or keyboard event. Web-page tab IDs are learned from content-script senders and
translated at the shim boundary; extension pages keep the model IDs the bridge owns.
The prepared extension catalog and bridge server belong to separate entities. Bridge-page
reconciliation reads them independently to create the hidden transport pages used by extensions.
On macOS, extension-owned authentication popups use the owning Layout's existing extension modal. Their
virtual window IDs remain stable for extension APIs while the underlying browser tab stays selected
for matching and autofill. Reopening from the toolbar reuses the current authentication document;
Escape and window removal retire the matching popup. Native popup input stays in its native window
instead of publishing popup-local coordinates to the workspace.
The modal measures its toolbar anchor and adapts its bounds to the content within the viewport.
It remains discoverable as an inactive extension tab in its own virtual window, so authentication
completion can locate and remove that window without changing the active content tab. Direct close
requests also target the owning popup entity.
Popup identity follows its extension across document navigation. Batched opens apply each
transition before the next request, and close messages carry the current virtual window ID.
Native popup documents are excluded from Stack ownership; genuinely routed extra tabs remain owned.
Other platforms retain Stack routing until their native popup adapter supports the modal.

---

## The daemon's registries

Two maps, and "registering" is inserting into one of them.

```mermaid
flowchart LR
    pm["ProcessManager"] -->|ProcessId| proc["Process<br/>PTY + child + cell grid"]
    am["AgentSessionManager"] -->|sid| sess["SessionHandle<br/>provider stream + history"]
```

Each exposes the same two surfaces: a **broadcast channel** of updates, and a point-in-time
**snapshot**. Clients hold no state — they subscribe.

**Terminal.** `portable_pty` opens the PTY and spawns the shell; a reader thread pumps its
output through `alacritty_terminal`'s VTE engine into a cell grid. Each poll diffs the grid
by row hash and broadcasts only the **changed lines**. The page applies each patch to
per-row signals, so only those lines repaint. The daemon also owns what should outlive a
frame: OSC 133 command tracking, per-shell integration, copy-mode motions.
OSC 7 reports the shell's current directory through the same process subscription used by local
and remote terminals, independently of the launch directory. The Terminal breadcrumb shows that
directory and lists alternatives through the daemon. Choosing a directory sends a shell-escaped
`cd` only after the daemon verifies an empty prompt, unchanged directory and no intervening input;
running commands and input in progress reject the request instead of queuing it for later.
Host input is one ordered ECS entity per write, related to its terminal until the process is
ready. Multiple producers cannot overwrite one another through a singleton mailbox.

**Editor.** `syntect` plus `two-face` — the ~200 grammars from `bat` — highlight line by
line into `StyledSpan`s. The file viewer, the preview and git diffs all emit the same
spans, so code looks identical wherever it appears. No tree-sitter, no LSP. The host ships
only the visible slice of a file and re-slices as you scroll.

**Git.** The Git page and automatic Git-backed projections for the editor, Projects, Sessions and
ACP enqueue work and only project completed results. Their repository discovery, status
aggregation, directory reads and path resolution use one bounded worker runtime with separate
interactive and mutating-operation lanes. Repository refreshes are single-flight per checkout,
inactive views release their watches and tasks, and watcher bursts coalesce into invalidations; the
watcher never runs a Git command or filters paths synchronously. Each Git child process drains
stdin, stdout and stderr concurrently, has bounded output and a timeout, and is terminated when its
operation entity is discarded. Short metadata commands avoid a fixed wait; process polling backs
off only while a command remains running. Catalogs and diffs page at their source, so opening a page cannot
schedule work in proportion to the total repository history or block the ECS schedule. Branch
review advances one file and one bounded line window at a time; viewport demand requests another
window. A large lockfile therefore cannot produce an unbounded first render, and an idle review
cannot silently materialize the rest of the branch in the page DOM.
Visible placeholders request a path-specific seek without changing selection or history. Pending
seeks and their continuation windows take priority over background prefetch while retaining the
older batch for later. Prefetch proceeds through files beside the requested path before resuming
older batches. Demand uses the actual visible range rather than the larger render window;
generation checks discard requests from replaced sources while allowing background revisions
to advance without invalidating a path-specific request.
File, staged-only, dirty-buffer and submodule comparisons share the disk-spooled review source.
Syntax state and already generated lines are retained, while colors are generated only through
the requested viewport and cancellation stops further work. Collapsed unchanged ranges use the
same full-width hunk row and line-number gutter as the diff, with separate upward and downward
expansion controls and a whole-range reveal target.
The rendered diff subscribes to its scroll window independently of host data. Moving into a
loaded region replaces virtual spacers with its rows even when no new data arrives.
When earlier content changes height, the file and offset at the viewport define the next render
window. The DOM interpreter restores that anchor within the edit transaction, including scroll
clamping, before another input can arrive. Loaded content therefore stays in the rendered window.
File selection uses smooth scrolling and a stable `#diff-<SHA-256 of path>` anchor, matching
GitHub's pull-request file view. Scroll requests carry their own revision so repeated selection
of one file works and data refreshes cannot restart a completed scroll.
Changing the selected branch, commit, stash or file clears the previous source's viewport;
refreshing the same source retains its loaded rows. Owners keep the last diff generation across
discarded and restored queries, so old viewport demand cannot target a new source. Breadcrumb
menus toggle immediately on click while retaining their hover delay.
Viewed toggles and fold controls recompute the file header's offset after changing the collapsed
sections and move the render window to that offset. Native height transitions compensate the scroll
offset for a header already pinned to the viewport, and reveal a header elsewhere only when needed.
Their height transition captures the rendered body before the DOM edit and keeps an inert copy
only while closing. Native height animation lasts 160 ms, cancels cleanly when reversed, and is
limited to the visible viewport so long diffs still visibly open. Completion removes the animation's
filled height, allowing a later expansion to use its natural size. Reduced motion keeps header
alignment without animation; the model and its row virtualization commit immediately.
Reopening a repository retains the cached file list and diff across repeated restores while
refreshing them in the background. Repository and branch-file reads keep the diff surface in
its loading state until their own first result arrives. The file tree reserves skeleton rows
only while its first files are pending; cached trees and later pages retain their rows without
loading placeholders beneath them.

The host counts columns in **display cells** — `DisplayCells`, one per narrow character and
two per wide one — because wrapping and selection are grid problems and the host has no
font. Only the page can turn a cell into a pixel, so it does: it measures a narrow advance
and a wide one, and `ColumnRuler` walks the line's own text to place a caret, a selection
or a click. Multiplying a column by one character width is what put the caret past Japanese
text — Menlo's advance is 0.6 em, the CJK fallback's is 1.0, and 2 × 0.6 is not 1.0. No
font stack metric-locks its fallbacks, so the page measures rather than assumes. A cell
column is also not a character index: pointer columns arrive as cells and the host converts
once on entry, and the status bar reports the character column, as VS Code does.

---

## Mobile Remote

The desktop is behind NAT, so it dials out and holds the connection open. Three modules own
that on this side; what the relay does in the middle belongs to `vmux-cloud`.

```mermaid
flowchart TB
    window["Window"] --> space["Space"]
    space --> tab["Tab"]
    tab --> split["PaneSplit"]
    split --> paneA["Pane"]
    split --> paneB["Pane"]
    paneA --> stackA["Stack"]
    paneB --> stackB["Stack"]
    stackA --> pageA["Page"]
    stackB --> pageB["Page"]
```

Position in the tree defines ownership. Each native window owns a complete tree. Spaces
identify projects; tabs preserve arrangements; splits tile panes; stacks hold page history.
Global actions resolve through the focused window and then through active descendants.

Agents receive a Space anchor. Relative tool operations resolve from that anchor, so
background work does not accidentally act on whichever window the user is viewing.

## Processes and persistence

The service owns terminals and agent sessions because their lifetime is longer than any page
or window. Clients subscribe to snapshots and updates; reconnecting does not recreate the
work.

Feature crates own their persistent data and migrations. Shared persistence infrastructure
provides atomic storage, versioning, and reconstruction. Application crates choose storage
paths and compose persistence plugins but do not interpret feature data.

Secrets use the platform credential store. Plain configuration may live in the profile
directory. A feature does not invent another storage root or secret format.

## Agents, commands, and tools

ACP agents run as sessions owned by the agent feature and background service. Their output is
projected into the same chat and workspace state a person sees.

Commands and tools follow one path:

```text
manifest definition
        ↓
typed feature-owned request
        ↓
feature ECS systems
        ↓
typed result or UiState update
```

The command bar, shortcuts, native menus, CLI, MCP, and in-app agents consume these
definitions. They do not maintain parallel command enums or callback tables.

MCP is a headless Bevy runtime over the same tool plugins. The transport understands JSON-RPC;
feature plugins understand the operation. The agent feature carries generic ACP and tool
envelopes but does not enumerate browser, Git, layout, vault, or editor behavior.

## Local and remote

Desktop pages talk directly to the host ECS. Mobile pages use the same typed contracts through
a remote `PageHost`.

The desktop dials out to the relay, so no inbound port is required. Pairing pins the desktop
certificate. The relay forwards encrypted packets and sees connection metadata, but not
session contents.

Remote is a lifecycle switch: when disabled, the desktop does not register or retry. Revoking
one paired client does not change other clients or the desktop registration.

Relay registry, admission, and deployment internals belong to `vmux-cloud`, not this
repository.

Tools are long-lived entities. Their name, aliases, schema, availability, and publication order
are components seeded from feature-local RON manifests by the shared tool registry system. A
feature marks each typed argument component with `#[vmux_tool::input]`; the type name supplies the
manifest key, so plugin registration is `register_tool::<T>()` without a second string name. Each
handwritten feature exposes only its plugin; its private update systems consume matching tool-call
components, validate typed arguments, and produce command or query dispatch.
Publication and execution query those entities directly; there is no separate runtime registry or
central function-pointer table. ACP sessions install the same tool plugin into the application's
world and submit tool-call entities there; they do not maintain a nested or thread-local Bevy app.
`vmux_acp` carries only the ACP and query envelopes. Browser, capture, bookmark, vault,
simulator, and other feature plugins decode their own query contracts, emit their own ECS requests,
and map their results back to the service boundary. Feature-specific agent instructions and
disabled skill roots are components registered by those same plugins. ACP and CLI launch boundaries
aggregate that policy without enumerating or naming the features installed beside them.

Application commands follow the same ownership rule. The shared command registry seeds
command-definition entities from each registered feature manifest. Feature systems bind their
typed Bevy requests or observer markers to those entities through `CommandRegistry`. Optional MCP
metadata lives on that same definition. The MCP process asks the running application for its
command tools and forwards calls back to the command-definition entities. Systems query those
entities directly to resolve aliases, validate authorization and arguments, and emit the typed
request. There is no command-catalog resource, second command enum, or MCP-only command catalog.

Every agent is launched **anchored to its own Space**. Tool calls resolve relative to that
anchor, so a background agent cannot read or disrupt the space you are looking at.

Each ACP agent's default config file is authoritative for its native MCP servers. Vmux may discover
recognized native configs for read-only display, but never copies them into `tools.toml` or injects
them back into ACP session creation. Agent launch injects only the internal `vmux` MCP anchored to
the Session workspace. Profile-owned integrations and OAuth are a separate future feature rather
than another representation of agent-native MCP config.

`read_layout` / `update_layout` are the interesting pair: fetch the pane tree with stable
ids, mutate it, commit it back. Vmux diffs against the live graph and reconciles
React-style, in one atomic transaction.

---

## Platforms

### Manual iteration

```text
agent: make build-dev -> signed Vmux Dev.app -> user: make <worktree>
standalone client: make run-dev
```

Debug uses the full desktop feature set, Bevy dynamic linking, no LTO, and incremental
compilation. Build and launch are separate: plain run-dev verifies the completed artifacts
and never compiles or kills an existing app. The cloud launcher requests build-if-needed,
which builds only when the completion stamp is missing or invalid. Unchanged signed inputs
reuse their bundled executables so a no-op build does not replace the daemon identity.
After a successful build, a launch link under `target/release/` lets the existing vmux-cloud
`make <worktree>` launcher find the debug app
without depending on cloud-specific files or changing the public client's build prerequisites.
Failed builds remove that link; real release bundles are never replaced. build-local remains
the bundle/OS-integration gate.

Dev, local, and release builds share a storage preflight and hold the target-cache lock
through dependency preparation, compilation, signing, and cleanup. Builds warn below
50 GiB free and refuse below 20 GiB after attempting to reclaim unused compile caches.
Successful local builds retain three local app bundles plus running apps; all builds prune compile
caches above a 50 GiB budget. Ready executables, runtime libraries, and release distribution
artifacts survive cleanup; active profiles and raw Cargo locks prevent cache deletion.
Removing compile caches invalidates their seed marker and requires recompilation on the
next build. The thresholds are configurable with `VMUX_BUILD_STORAGE_*`; cache size uses
allocated blocks, which can count shared APFS clones more than once. Storage cleanup stays
inside the selected Cargo target and never walks sibling worktrees or application data.

The dependency-free `vmux_builder` Rust command owns build sequencing, storage policy,
target locks, and local/release launch paths. `scripts/vmux-builder` only bootstraps a
missing host binary; Rust checks the embedded source snapshot and compiler, rebuilds
itself when needed, and caches binaries under `.build-tools/`. Capacity checks therefore
do not resolve Cargo dependencies or compile into the application target.
The launcher refreshes through `--print-bin` before forwarding arguments, so an older
binary can upgrade before parsing a newly added command.

The builder preserves the existing lock protocol: PID files on macOS and file locks on
Linux. Lock waits remain cancellable. Each build command gets a separate process group;
minimal POSIX signal handlers record cancellation and forward it to that group without
allocating. The builder waits for its child, skips successful-build cleanup on failure or
cancellation, and releases its locks before returning the original exit code.
Preflight failures refuse the build. After the command succeeds, cleanup inspection or
filesystem failures warn and preserve that success; cancellation still returns its signal code.

Debug preparation and macOS package orchestration are Rust stages. Debug owns Cargo
command order and the ready stamp/launch alias; macOS packaging owns env overrides,
temporary signing keychains, restoration, and certificate verification. Environment
overrides are attached to child commands without changing process-global env. Signing
keychains and private certificate files live in scoped temporary directories and are
removed on failure or cancellation. The legacy build scripts only delegate to these stages;
platform tools and the existing bundle/sign adapters remain subprocesses.

Local and debug share the workspace store and a stable Chromium directory per named profile,
not per worktree. On first use the most recently modified legacy local profile is reused
without merging or overwriting other profiles. Normal builds use the same Safe Storage key;
mock-key test sessions have their own data directory. Logs and service endpoints stay separate.
A process-lifetime desktop lock allows one GUI writer; desktop and daemon share a data lease
whose owner is the build and worktree. A different owner cannot start until both have exited,
and an endpoint lock prevents duplicate daemons from overwriting PID/socket files.
The debug `.app` contains application binaries, its icon, generated plists, and a service login item.
CEF stays in the installed SDK,
and Bevy/Rust dylibs are linked from the build/toolchain cache. The executable has bundle rpaths
so Finder and `open` need no shell library-path setup. This app is local, not a distributable bundle.
CEF receives the host's main bundle path so its SDK subprocess uses the same IPC port namespace.
macOS debug child launches normalize renderer and alert helper paths to the installed SDK
executable and retain the configured no-sandbox mode. Prewarmed terminal pages have no process
identity and do not count as running terminals in quit confirmation.
Both host and helper signing caches include identity and entitlements; a valid linker signature
alone does not satisfy the development signing contract.
Quit flushes pending layout changes, shuts down that profile's service, and waits for its daemon
lease and child process to retire. Closing a page or hiding the window preserves background work;
a successful restart within the same profile preserves the service too. Switching named profiles
ends the old profile's service before opening another writer. A different build drains the old
daemon and waits for its data lease automatically. Service shutdown closes agent inputs, pumps
ECS requests while their tasks drain, then aborts remaining tasks and reaps owned processes.
Detached children inherit only standard descriptors, and agent process groups
remain owned even after the parent agent exits. Old binaries without leases are not
protected by these advisory locks. VMUX_DATA_DIR selects a separate data root and runtime
endpoints for copied-state tests; manual service start inherits that root and named profile
instead of using a global launchd environment. Browser paths outside the chosen data root
are refused, including copied symlinks back to live data. Newer schemas refuse startup;
older or incompatible stores are backed up before resetting. Changing persistence schemas
against the live shared store is not an iteration shortcut.

| Platform | Role | Status |
|---|---|---|
| macOS | host | primary — packaged, launchd-managed, shipped |
| Linux | host | builds and tests in CI, not packaged |
| Windows | host | not started |
| iOS | remote client | linked in CI, packaged and shipped via `dx` |
| Android | remote client | configured, no platform code yet |

A remote client is strictly a client — the server half of `vmux_service` is compiled out.
`vmux_service` also owns its local connection, daemon control, paths, pairing, and authorization
APIs; these are facets of one service boundary rather than a separate generic client domain.
Persistent processes and their query runtime belong to this service boundary. Terminal pages
render and control those processes, but the service names and exposes process operations without
depending on terminal page concepts.
macOS process CPU percentages compare cumulative CPU nanoseconds with elapsed monotonic time
converted to nanoseconds; raw Mach clock ticks are not the same unit.
Repository project identity uses the primary checkout. Submodules resolve `core.worktree` from
their common Git directory; `.git/modules` is metadata, never a project root.

The CLI uses Clap only to parse argv. Feature crates contribute command metadata through local RON
manifests and own the typed request components and systems that execute those commands;
`vmux_app` only composes those plugins. A finite command runs in a short-lived Bevy app and
`AppExit` follows a typed result. Long-running MCP stdio remains a dedicated runtime over the same
feature-composed tool registry.

Two cfg aliases decide that split, emitted by `crates/build_platform_cfg.rs`: **`ui`** is
iOS or macOS, the surfaces that run pages; **`host`** is everything that is not iOS, the
desktop app and the daemon. Write `#[cfg(host)]` rather than a negation of target
predicates. Cargo's own `[target.'cfg(...)'.dependencies]` still has to spell the targets
out, because dependency resolution happens before any build script runs.

---

## Where a crate lives

```
crates/
├── app/                    something a user starts
│   ├── vmux_cli
│   ├── vmux_desktop
│   └── vmux_mobile
├── feature/                user-facing capability and page ownership
│   ├── vmux_acp
│   ├── vmux_command
│   ├── vmux_editor
│   ├── vmux_extension
│   ├── vmux_git
│   ├── vmux_history
│   ├── vmux_knowledge
│   ├── vmux_layout
│   ├── vmux_service
│   ├── vmux_session
│   ├── vmux_setting
│   ├── vmux_shortcut
│   ├── vmux_simulator
│   ├── vmux_space
│   ├── vmux_start
│   ├── vmux_team
│   ├── vmux_terminal
│   ├── vmux_tool
│   └── vmux_vault
└── util/                   reusable infrastructure without its own page
    ├── vmux_api
    ├── vmux_app            platform-neutral application plugin facade
    ├── vmux_browser        browser runtime and native-page renderer
    ├── vmux_clipboard
    ├── vmux_ecs
    ├── vmux_flex
    ├── vmux_macro
    ├── vmux_mcp
    ├── vmux_native         the VirtualDom driver and its wry webview
    ├── vmux_path           canonical and scoped path identities
    ├── vmux_profile
    ├── vmux_transport
    └── vmux_ui
```

Important boundaries:

Agent requests follow the same boundary. `#[vmux_api::agent]` generates only the opaque contract
for the type declared in a feature crate. That feature owns its manifest, registration, ECS routing,
handling, and result projection. `vmux_acp`, `vmux_mcp`, and `vmux_tool` transport and dispatch
opaque requests; they never enumerate browser, simulator, vault, layout, or other feature requests.
Only the application composition root knows which feature plugins are installed.
And the `host` cfg alias is **not** the directory: `vmux_ui` holds host-gated code while
staying flat.

`vmux_session` owns the durable unit of work: one canonical Session entity with its stable ID,
task metadata, working directory, data-driven stage, conversation-event children, optional agent
identity, and the derived Session catalog. Session creation generates its ID inside the typed
request and emits a typed creation event before navigation. Layout Stacks reference the Session
entity and may close without deleting it. `vmux://sessions/` opens the manager;
`vmux://sessions/{session_id}` opens a Session independently of its agent or provider conversation
identity. Session IDs are encoded as one URL path segment.
Session creation, metadata, stage, archive, workspace, and agent binding requests converge on one
typed mutation commit path. It validates unique Session identity and the expected revision before
changing state, then stores the operation UUID, payload, base revision, and committed revision on
a Session-owned record. Repeating the same operation returns its original committed revision;
a changed payload or stale revision is refused. UI messages, queued ACP commands, and atomic agent
conversation summaries share this path. Provider synchronization and conversation events keep
their own contracts.

Conversation events persist their identities, contents, timestamps, and source markers with the
Session. Restore displays that history before runtime attachment; provider replay reconciles the
same event identities without duplicating cached messages.

`vmux_acp` attaches transient runtime and ACP capability state to Session entities and owns the
provider-owned `AcpSessionId`, its persistence, and the single runtime detach path used by cleanup
and agent replacement. `vmux_session` renders the
Sessions page and Dioxus conversation UI. A dedicated manager-view marker is the authorization and
publication boundary for Session catalog and mutation UI events; Session detail views receive only
their bound Session state. The service carries serialized agent protocol messages and runs
persistent daemon sessions; `vmux_acp` owns the client-host Bevy messages produced when those
wire messages enter ECS.

The Start page offers a welcome and Start control when no agent is selected. Start opens the
setup conversation explicitly; launcher entry only redirects to a recorded or usable Session.
The guide asks which agent to
install or connect; selecting its compact composer choice starts only the required installation and
authentication. Work descriptions and project selection belong to the actual agent conversation.
An installed agent reuses its package and compatible credentials. A live, usable preferred Session
opens directly; a recorded setup Session resumes its unfinished step. Later launcher tabs keep
their ordinary navigation behavior.

`vmux_session::host::onboarding::Plugin` owns profile-local setup progress, agent selection, and the Session reference.
Explicit agent selection starts the persisted setup record, so an interrupted connection can resume.
`OnboardingConnect` creates or reopens setup through `StartRequest::for_setup`, which carries no user
content. Session creation enqueues only explicit user text or attachments; setup leaves the prompt
queue empty until the user sends a real message. Schema 2 setup records preserve the selected agent
while discarding the earlier task-first Session reference, without deleting conversation history.

The bootstrap and live Session share their conversation surface, transcript, assistant/user turns,
composer and choice controls. The guide offers up to three common agent families, preferring
an installed option within each family. Selection sits in the shared composer's outer notice frame;
search exposes the remaining supported catalog when requested. Authentication uses one Connect
control for the provider's account/browser method. Compatible API keys are a secondary opt-in route.
The host owns setup state; the cursor and search stay local to the UI. Normal text input is
disabled until readiness. Explicit key entry uses the same composer with a password input and
ordinary Send/Enter submission; private input never enters the normal draft. There is no separate
key form or persistence control. The uncommitted key-entry mode resets outside authentication.
Preparation and installation show the normal Stop control rather than management options.
Stop, Ctrl+C and Escape route to setup cancellation without flushing queued work or discarding
the Session. Shared loading/progress animations and phase icons accompany guide messages;
installation progress is indeterminate until an actual percentage is available.
ACP supplies its supported catalog, credential availability and managed installer.
Agent logos use the official registry's 16×16, monochrome SVG icon metadata. Shared thumbnails
and favicons adapt those icons to the current theme; supplied icons take precedence over legacy
alias fallbacks. The client does not maintain colored logo assets beside the registry.
Preparing and authentication states hold any genuinely submitted user messages until the provider
establishes its Session. Advertised authentication methods run through ACP `authenticate`;
cancellation detaches the waiter or runtime and keeps reusable package data. Provider sign-in
remains in its adapter, with shared connector authentication infrastructure reused where compatible.

New transport variants append to existing enums. Authentication lists use boxed archive storage to
preserve the deployed run-state layout while remaining ordinary arrays in JSON.
Credential discovery runs after the settings Startup load; initial app composition cannot read an
unregistered settings resource.

macOS API-key entry uses a redacted, zeroizing input contract and an asynchronous Safe Storage write.
`vmux_profile::CredentialStore` binds the encrypted envelope to the profile, credential namespace
and key name. Only availability and save status reach UI state; neither the draft nor settings hold
keys. A pending write holds conversation startup, and replacing a key retries only the same Session
while it still needs authentication. Launch reads the saved key from the installer's data root, so
fixture installers cannot read the personal credential store. The default projects directory remains
unbound context: the agent asks for a project when the selected task needs one.

`scripts/run-onboarding-test.sh clean` opens a prepared debug app with a fresh data directory and
named profile. `env` uses the same isolation while allowing the current process's supported API
keys. A supplied absolute second argument reuses a fixture for interruption tests. An isolated
data directory owns Vmux configuration, projects, managed ACP/LSP downloads, browser state and
an external-tool home per profile; agent and LSP subprocesses receive that home without unrelated inherited
credentials or login dotfiles. Normal profiles retain their shared package store and existing home.
Knowledge configuration and memory synchronization resolve the same isolated agent directories,
including when the host environment points to personal agent configuration.
System package managers and macOS permission grants remain machine resources.

External session integration is a lifecycle-owned child plugin of `AcpPlugin`. It registers its
catalog, history and archive state, observers, and ordered systems through `Plugin::build`;
production and integration tests compose that same plugin through `add_plugins`.
Its archive child owns native archive requests, workers, and reconciliation. This compatibility
boundary remains necessary because the current ACP session catalog carries no archive state.

ACP `session/list` is the primary provider catalog. The Sessions page starts each installed ACP only
long enough to enumerate its paginated catalog, then stops it; transport failure does not switch data
sources. A bounded provider-store adapter is used only when no installed ACP owns that agent family
or the ACP explicitly omits `session/list`. Browsing an external entry does not copy it into durable
Vmux state. Resuming it creates the canonical Session entity, records its `AcpSessionId`, and lets
the provider replay the conversation through `session/load`. Compatibility watches exist only while
the Sessions page or one of their imported Sessions is present, debounce into one wake, rescan only
the changed provider, and bound untrusted history traversal.
Missing history resumes from the catalog owner's original provider ID. Cached continuations keep
their runtime resume ID. Unsupported or failed loads report an error and keep the Session's identity.
Empty replays preserve cached history, and failed loads release the loading state.
External history adapters feed the same Session events as ACP updates. Source keys match agent
family and provider ID. Failed resumes retain the canonical Session, read its original history,
and append only unseen content; replay matching ignores presentation metadata and normalizes
assistant-block grouping. Open read-only views share a sleeping 250 ms worker that checks only
known files. Changed files wake cursor reads; leaving the view or regaining the writer releases
those watches. macOS file notifications alone may wait for the writer to close its file.
Claude and legacy chat JSONL share one record decoder and bounded file lookup. Current Vibe
uses a small read-only adapter to its installed storage SDK, retaining its chunk cache and
watching CURRENT plus the active recovery journal. The normal app-server launcher performs
worktree cleanup, so this adapter imports storage directly and acquires no writer lease.
Native mutable history entries retain their IDs and update existing events without duplicating
unchanged entries. ACP tool kind survives replay and updates; names are fallback classification.
ACP sessions withhold startup snapshots until history replay finishes. Compatibility readers leave
their loading state to the ACP protocol; they read and complete only their own provider-store tasks.
History loading starts and finishes publish a fresh chat snapshot independently of message
changes, so empty or unchanged replays still release the conversation skeleton.
Initial and pending bottom scrolls retain their follow state through layout-driven offset changes.
Scrolling away from the bottom pauses following; input that leaves the viewport at the bottom
keeps it pinned, and returning to the bottom resumes it. Older history is requested only after
the initial scroll has settled. The latest tool batch stays expanded until every call has a result
and subsequent assistant text arrives; earlier batches remain folded. Tool rows share native
Claude/Vibe name classification and summarize wrapped Codex command batches as Ran, Explored,
or Edited. Exploration exposes Read, Search, and List steps from the actual shell commands.
Session page roots install typed state listeners once. Child components read
`UiStateBinding<T>::use_context` and selectors, so replacing a manager or detail subtree preserves
the subscription and its retained state. Removing `SessionManagerView` immediately removes the
full catalog projection, even when page metadata does not change. Scoped projections retain their
own state.
Editor and Git roots likewise own their shared file, address, location, and key bindings.
Nested `use_ui_state<T>` calls reuse an inherited typed binding, selected once when their scope
mounts, instead of registering another transport listener. Child replacement therefore retains
the root snapshot and listener error state; selectors still subscribe only to the values they use.


The Session manager publishes only to its active page and leases Git metadata for every displayed
repository while that page remains active.
Its first discovery reserves a skeleton in one persistent scroll surface until rows are available.
Each provider publishes its completed catalog without waiting for other providers. Partial updates
keep existing rows and append new identities; a completed discovery can remove
missing identities without reordering the survivors. The view-control band and scrollbar gutter
keep their space during loading, with the selected view icon using the foreground color.
Once rows exist, background discovery never appends another initial-loading skeleton.
Git change counts live in the Space card with labels rather than as dots in the composer.
Startup failures retain their specific provider cause when the process exits. A conversation owned
by another client shows a neutral status card explaining that ownership, with retry and diagnostic
copy controls. Its composer disables input, attachments, sending and setting changes while keeping
the agent and known settings visible with a neutral ownership label. Ownership refusals cannot
enqueue prompts or clear drafts and attachments, and they do not emit an error toast.
Failed Codex resumes can read bounded local history under the original provider ID while retaining
the refusal; a successful retry or an already-populated transcript discards a late local read.
Retry preserves its provider identity and replaces only the failed local runtime.
Failed conversations release their transcript skeleton.
Composer workspace context resolves the Session named by the page URL even before runtime attachment
has bound the Stack. Unpublished context is retried after the first ready frame, so a late binding
cannot leave project and branch controls as placeholders. Branch menus enqueue their checkout reads
through `vmux_git::GitRuntime` and finish empty results rather than leaving a loading request open.
Project dashboards and resume suggestions use the same catalog stage and staleness projection.
Embedded Session groups show five entries at a time, with stale groups initially collapsed.
Resume suggestions restrict host contributions to the visible entries, so keyboard selection
cannot reach hidden sessions; each status has its own explicit expansion limit.
Project creation and editing share one form for a display name and a local folder or GitHub
repository. GitHub sources clone through the existing Git provisioning lifecycle before registration.
Registered Projects are entities owned by a Project `Root`, composed from `ProjectId`, `Name`,
`Cwd`, repository and shared revision components. The Editor adapter binds Profile `ProjectsRoot`
entities to that core root. `ProjectPlugin` loads once and serializes accepted writes per root;
`FilesystemPlugin` owns atomic IO tasks. Save operations carry preparation, provisioning and
commit state as components; systems enqueue immutable snapshots at the filesystem boundary.
Completed IO is polled before the core store sets so the worker's wake commits its result
in the same frame.
Writes check the current ECS revision and reject duplicate checkouts before dispatch. Only a
successful atomic write updates Project entities and emits `ProjectSaved`; failed writes retain
the previous registration. Stable Project URLs survive source edits. Removal persists hidden
URL entities for the registration and its imported aliases while leaving checkout files intact.
Alias resolution runs inside the root's write task, and committed hidden URLs are restored into ECS.
Page routing and catalog projections read ECS registrations. Native folder selection authorizes
a new external directory, and the form retains its draft on validation or clone failure.
Accepted saves and registration removals finish after the requesting view closes.
Project entities omit the workspace `Save` marker; `.projects.ron` is their only persistence owner.
Archive membership is also a Project-owned component and mutation. The filesystem adapter imports
legacy `.archive.ron` state until the first successful versioned `.projects.ron` write; later
restores use that committed snapshot, so an unarchive cannot be undone by the old archive file.
Root removal or replacement cancels domain work and stale completion messages cannot commit
into a replacement root. Removing the requester alone does not cancel an accepted save.

Project dashboards show related sessions, repository status and an entry point into Code,
without reading or listing arbitrary directories in the overview. Repository status uses the Git
feature's shared `RepositorySummaryRequest` subscription rather than loading a Git page. Reads
share a bounded worker task per requested root, publish only requested results, and settle failed
reads. Hidden pages release their tasks; returning pages retain their last result during refresh.
The repository root is a resource location, not a separate logical project identity.

The DOM interpreter measures the initial transcript rows before positioning them. Their
content-visibility estimates are replaced by real extents for the first paint, then normal skipping
resumes without waiting for fonts or streaming updates. History insertion captures the currently
visible row immediately before DOM edits and restores its position before paint, so loading cannot
replay an earlier scroll position over fresh input. A resize observer schedules following at browser
frames instead of writing layout from its notification loop. Any upward reading movement stops
following; only the actual bottom resumes it.

Session failures with a useful remedy occupy a light notification strip in the composer's outer
frame. The dark composer keeps its draft and metadata inside that frame. Busy sessions disable
editing, attachments and submission while Project and Branch menus remain usable. The notification
strip owns the locked-status message; the footer retains activity counts. Model and agent
configuration stays read-only. Busy sessions
explain where to release the writer; missing histories offer a new conversation; connection failures
offer retry. Other failures use a dismissible toast. Raw provider diagnostics remain in app logs.

Project summaries open Code through the same README-first entry selection. Direct directory routes
retain directory navigation. Code opens the owning Space's working directory and prefers its README, then a bounded set of
entry files, before falling back to directory columns. Directory pages use paths as page titles
and show columns without a duplicate Explorer tree. The outer editor container and the columns
share directory key handling; text view-mode updates cannot steal directory focus.
Git diff file placeholders come from the file catalog before the first diff viewport arrives,
so publishing file names does not create an empty interval while their bodies are still pending.
Editor route changes seed shared preferences into cold document state and retain the previous
presentation until the new mode's content is ready. Source rows wait for character measurements;
cursor and rendered-note state are projected together. Another checkout is a read-only preview
until project activation aligns the Space with that checkout, and the host rejects mutation and
Save requests while that preview remains active.
Page item close and reopen requests precede Stack fallback. Archive capture runs before source
destruction; the Editor persists ordered paths and selection and keeps unsaved buffers and Undo
in an in-memory archive companion. Restore transfers those into the new surface without saving.
Layout animation clocks start at the request, and one deadline scheduler drives their 16 ms wakes.
Sidebar content keeps its configured width while the reserved width and outer clip change, so pins
do not reflow through a zero-width column. Stack presentation rectangles interpolate structural
moves and resize and are consumed by both hosted child placement and native windowed frames.
Sidebar rows preserve their identity and animate from their previous DOM rectangles.
Ready Git pages retry checkout-context publication when parent or slot binding arrives late.

`vmux_profile` owns profile identity and filesystem locations. Tool inventory is a separate
capability in `vmux_tool`; consumers depend on it directly instead of reaching through a
`vmux_ecs` re-export. `vmux_tool` also owns the tool page, manifest, inventory lifecycle,
operation sequencing, and built-in Homebrew, NPM, MCP, and dotfile providers. Feature crates
register their own provider entities, such as ACP in `vmux_acp` and LSP in `vmux_editor`;
application crates only compose plugins. `vmux_path` gives filesystem boundaries one shared
identity rule and rejects scoped paths that traverse or resolve outside their root.
Tools use the shared workspace sidebar for category navigation and search; their URL trail is
Tools / category. Provider metadata publishes before inventory scanning finishes, and inventory
operations and view controls remain in the page. Project details publish Projects / project,
while Session directory links open the Code entry file. Branch controls share branch identicons. Session lists use the same board switch and column
icon in Project details. Popup dismissal starts after the first open, so mounting a closed native
popover cannot cancel its opening.
Accepted inventory scans reconcile catalog-owned Item entities by provider and item ID. Each item
composes `Name`, `ToolProviderId`, `RegistrationOrder`, and typed version, icon, detail, status,
management, and operation components. Refresh preserves surviving entity identities and removes
missing items. `ToolsSnapshot` and `ToolItem` are UI projections; counts and Apply selection read
the same canonical entities, and component changes advance the catalog publication revision.


Repository tool versions live in `tool-versions.env`; CI loads the same values used by local
scripts and the Makefile. Release asset names and URLs come from
`scripts/release-artifacts.sh`, while the standalone installer selects the matching asset from
the published release instead of reconstructing its name.

Feature PRs run Linux lint and tests, iOS checks, and macOS type checks of the desktop, CLI,
and daemon, including the lean desktop composition and generated UI asset validation.
The required `Build macOS` gate requires full packaging and the startup smoke test for changes
to native adapters, application composition,
dependency manifests, lockfiles, toolchains, build scripts, and packaging. Other feature PRs
require the macOS checks without a release build or DMG. Main pushes and manual CI runs retain
full packaging and startup validation; nightly and release workflows retain signing and
notarization. New PR commits cancel superseded CI runs without canceling main builds.

`vmux_builder ci-scope` compares a PR with its merge base and selects changed workspace
packages plus their transitive reverse dependencies, including optional, build, and test
dependencies. Formatting and Clippy use the same selection while excluding vendored patches.
Formatting runs before Linux SDK preparation in its own job. Linux Cargo checks share a
version-validated CEF SDK cache through `CEF_PATH` rather than downloading copies into build
directories. Pull requests restore Rust dependency caches, while only main saves them, keeping
per-PR cache entries from evicting the shared baseline.
Renames include both package owners. Manifests, lockfiles, build scripts,
CI configuration, shared API/ECS/macros/composition, and unmapped changes require the full
workspace. Scope discovery errors fail the test gate rather than skipping tests. Builder
fixtures and repository source conventions always run. Lean desktop checks and tests run for
direct desktop changes and full suites. Main code changes, manual CI, and the daily CI schedule
run the full suite; the schedule leaves macOS packaging to the nightly workflow.
