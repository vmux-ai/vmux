# Vmux Framework Examples

Runnable, Bevy-style examples for building **Desktop**, **Web**, **TUI**, **Mobile**, and **Service/Daemon** applications on Vmux Framework (`DefaultPlugins`, `MinimalPlugins`, `ServerPlugins`), plus low-level ECS primitive references.

Run any example from the repository root (`cargo run -p vmux_examples ...`) or from inside `examples/` (`cargo run ...`). Pass `VMUX_EXAMPLE_HEADLESS=1` to run interactive client examples as a single-shot smoke test in a terminal or CI.

## Client Applications

### Desktop (`DefaultPlugins` — CEF-free `vmux_window` + `vmux_page` + Dioxus)

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| Counter App | [`window/counter.rs`](window/counter.rs) | Minimal desktop app bootstrapping with `DefaultPlugins.set(WindowHostPlugin::new(...))`, `#[ui_state]`, `#[ui_event]`, and a Dioxus `Page`. | `cargo run -p vmux_examples --features window --example window_counter` |
| Notes App | [`window/notes.rs`](window/notes.rs) | Persistent multi-note desktop app combining `DefaultPlugins`, `IdentityIndexPlugin`, and `AutoSavePlugin`. | `cargo run -p vmux_examples --features window --example window_notes` |
| Split Workspace | [`window/split_workspace.rs`](window/split_workspace.rs) | Multi-pane desktop workspace combining `DefaultPlugins` (`LayoutPlugin` + `FlexPlugin` + `MenuPlugin`) and declarative `#[app_menu]`. | `cargo run -p vmux_examples --features window --example window_split_workspace` |
| Web Host App | [`window/web_host.rs`](window/web_host.rs) | `DefaultPlugins` + `WebHostPlugin` + linked Rust native plugin (`PingRequest`). | `cargo run -p vmux_examples --features window --example window_web_host` |

### Web (`ServerPlugins` + `HttpServerPlugin` + Dioxus SSR)

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| Web Client | [`clients/web_app.rs`](clients/web_app.rs) | Interactive server-rendered Dioxus web app + JSON API backed by canonical ECS state via `ServerPlugins` and `HeadlessTokioRunner`. | `cargo run -p vmux_examples --features web --example client_web` |

### TUI (`MinimalPlugins` + `LayoutPlugin` + `FlexPlugin`)

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| TUI Workspace | [`clients/tui_workspace.rs`](clients/tui_workspace.rs) | Multi-pane terminal UI computing terminal cell bounds with `FlexPlugin` and handling focus/split commands via `LayoutPlugin`. | `cargo run -p vmux_examples --example client_tui` |

### Mobile (`MinimalPlugins` + `UiStatePlugin` + Dioxus, zero desktop deps)

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| Mobile Shell | [`clients/mobile_app.rs`](clients/mobile_app.rs) | Mobile-safe client architecture composing `MinimalPlugins`, `UiIpcPlugin`, `#[ui_state]`, `#[ui_event]`, and Dioxus screens without windowing or CEF crates. | `cargo run -p vmux_examples --features mobile --example client_mobile` |

### Service / Daemon (`ServerPlugins` + `HeadlessTokioRunner` + `ShutdownPlugin`)

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| Service Daemon | [`clients/service_daemon.rs`](clients/service_daemon.rs) | Long-running HTTP service with `/healthz`, `HeadlessTokioRunner`, and graceful `ShutdownBarrier` draining. | `cargo run -p vmux_examples --features service --example client_service` |

---

## Low-Level Primitives & Util Reference

Small focused examples isolating individual ECS, concurrency, layout, menu, and manifest plugins.

### ECS Vocabulary & State

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| Selection | [`ecs/selection.rs`](ecs/selection.rs) | Scoped single-entity selection with `SelectionPlugin` and `SelectRequest`. | `cargo run -p vmux_examples --example selection` |
| Leases | [`ecs/lease.rs`](ecs/lease.rs) | FIFO shared/exclusive resource leases with `LeasePlugin` and automatic release on despawn. | `cargo run -p vmux_examples --example lease` |
| Generation & Dirty | [`ecs/generation_dirty.rs`](ecs/generation_dirty.rs) | Rejecting stale operations with `GenerationPlugin` and running dirty projections with `DirtyPlugin`. | `cargo run -p vmux_examples --example generation_dirty` |
| Vocabulary | [`ecs/vocabulary.rs`](ecs/vocabulary.rs) | Shared `RenamePlugin`, `OrderPlugin`, `TogglesPlugin`, and `StatusPlugin` entity vocabulary. | `cargo run -p vmux_examples --example vocabulary` |
| Operation Policies | [`ecs/operation_policy.rs`](ecs/operation_policy.rs) | Per-target FIFO serialization (`SerializePerTarget`) and superseding (`SupersedePrevious`) via `OperationPlugin`. | `cargo run -p vmux_examples --example operation_policy` |
| AutoSave Store | [`ecs/autosave_store.rs`](ecs/autosave_store.rs) | Macro-tracked store dirty detection and debounced persistence with `AutoSavePlugin` and `#[vmux_ecs::store(track; ...)]`. | `cargo run -p vmux_examples --example autosave_store` |

### Async, Concurrency & Time

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| Async Channels | [`concurrency/async_channel.rs`](concurrency/async_channel.rs) | Streaming messages, spawned entities, and entity-bound channels from a background thread via `AsyncChannelPlugin`. | `cargo run -p vmux_examples --example async_channel` |
| Async Task | [`concurrency/async_task.rs`](concurrency/async_task.rs) | Entity-owned `AsyncTask<T>` completion and `TaskToken` cancellation with `TaskPlugin<T>`. | `cargo run -p vmux_examples --example async_task` |
| IO Task | [`concurrency/io_task.rs`](concurrency/io_task.rs) | Typed async IO outcomes (`Ok` component insertion and `TaskError<T>`) with `IoTaskPlugin`. | `cargo run -p vmux_examples --example io_task` |
| Blocking Jobs | [`concurrency/jobs.rs`](concurrency/jobs.rs) | Blocking thread-pool `JobPlugin<J>` integrated with `Operation` completion. | `cargo run -p vmux_examples --example jobs` |
| Deadlines & Clock | [`concurrency/deadline.rs`](concurrency/deadline.rs) | Deterministic `Clock::fake`, `DeadlinePlugin`, `Debounce`, and `ExpiresAt`. | `cargo run -p vmux_examples --example deadline` |
| File Watch | [`concurrency/watch.rs`](concurrency/watch.rs) | Watching a directory and receiving `FileWatchEvent` messages through `FileWatchPlugin`. | `cargo run -p vmux_examples --example watch` |

### Layout, Menus & Manifests

| Example | File | What it demonstrates | Command |
|---|---|---|---|
| Panes | [`layout/panes.rs`](layout/panes.rs) | Headless pane tree splits, scoped selection, and directional neighbor queries via `VmuxPlugin::builder().layout(true)`. | `cargo run -p vmux_examples --example panes` |
| Flex Geometry | [`layout/flex.rs`](layout/flex.rs) | Renderer-independent flexbox layout calculation via `VmuxPlugin::builder().flex(true)`. | `cargo run -p vmux_examples --example flex` |
| Declarative Menu | [`menu/declarative_menu.rs`](menu/declarative_menu.rs) | Defining typed application menus with `#[vmux_app::menu::menu]` and dispatching actions via `MenuActionPlugin`. | `cargo run -p vmux_examples --example declarative_menu` |
| External Feature | [`manifest/external_feature.rs`](manifest/external_feature.rs) | Registering RON feature manifests and validating dependencies/permissions via `FeaturePlugin` and `CompositionPlugin`. | `cargo run -p vmux_examples --example external_feature` |
| Web Host Package | [`manifest/web_host.rs`](manifest/web_host.rs) | Validating a pinned web application envelope and observing `HostedApplicationReady` via `WebHostPlugin`. | `cargo run -p vmux_examples --example web_host` |

### Containers

| Example | Directory | What it demonstrates |
|---|---|---|
| Nginx Compose Demo | [`containers/`](containers/) | Minimal Dockerfile and `compose.yaml` for testing VM port forwarding (see [`docs/ux/containers.md`](../docs/ux/containers.md)). |

## SDK Compatibility Suites

Independent Cargo workspace compatibility and dependency-isolation tests live separately under [`tests/sdk/core`](../tests/sdk/core) and [`tests/sdk/host`](../tests/sdk/host), run via `vmux dx test sdk`.
