# Core concepts

Vmux Framework models an entire application—state, async work, UI projections, menus, persistence, CLI, and MCP—as one typed Bevy ECS plugin graph.

## The eight primitives

| Primitive | Role | Familiar web equivalent |
|---|---|---|
| Crate | Owns one capability or reusable infrastructure boundary | Package / workspace |
| Plugin | Registers components, messages, systems, and child plugins | Feature module / provider |
| Entity | Stable identity for a document, pane, page, session, or operation | Record ID / tree node |
| Component | Canonical state attached to an entity | Normalized store slice |
| System / Observer | Pure state transition reading/writing components and messages | Reducer / command handler |
| Message / EntityEvent | Typed domain request or lifecycle notification | Dispatched action |
| `#[ui_event]` | Typed intent sent from a page to ECS | Mutation input |
| `#[ui_state]` | Read-only snapshot/patch projected from ECS to a page | Query selector / view props |

## Composition and target profiles

Select capabilities at compile time via `vmux_app` Cargo features, and install plugins at startup via `VmuxPlugin::builder()` or directly on `App`:

- **Headless / Daemon / CLI / MCP**: enable `ecs`, `jobs`, `store`, `runner`, `http-server`, `quic-server`, `manifest`, or domain cores (`layout`, `flex`, `project`, `lsp`, `extension-core`). Zero windowing or WebView dependencies are linked.
- **Desktop Window (CEF-free)**: enable `window-native` and `ui`. `WindowHostPlugin` (`vmux_window`) provides reactive `winit` windowing, `PageHostPlugin` (`vmux_page/native` Wry + Dioxus), and `FlexPlugin`.
- **Full Browser**: add `browser` (`vmux_browser`) only when your application embeds Chromium web tabs.

## Shared entity vocabulary

Before inventing feature-specific state machines, compose the shared plugins in `vmux_app::ecs`:

- **Naming, ordering, toggles, status**: `RenamePlugin`, `OrderPlugin`, `TogglesPlugin`, `StatusPlugin`, `TimestampPlugin`.
- **Scoped identity**: `IdentityIndexPlugin<Id, Marker>` resolves IDs within a `ChildOf(owner)` scope without leaking across independent roots.
- **Selection**: `SelectionPlugin<Scope>` enforces at most one `Selected<Scope>` per parent/root scope.
- **Leases**: `LeasePlugin` arbitrates FIFO shared/exclusive access to any entity and releases leases automatically on despawn.
- **Stale-work guarding**: `GenerationPlugin` + `AtGeneration` + `Stale` automatically cancel in-flight work when an owner's generation advances; `DirtyPlugin<Tag>` drives bounded projection passes.

See [`examples/ecs/`](https://github.com/vmux-ai/vmux/blob/main/examples/README.md#ecs-vocabulary--state) for runnable examples of each plugin.

## Operations, async tasks, and reactive wakes

Every reactive crate wakes the host event loop through `vmux_ecs::JobWake` (`WinitWakePlugin` on desktop, `HeadlessTokioRunner` in daemons)—never `UpdateMode::Continuous`.

- **Operations (`OperationPlugin`)**: represent long-running work as canonical `Operation` entities with `OperationPolicy` (`SerializePerTarget`, `SupersedePrevious`), `OperationTimeout`, and `OperationRetry` exponential backoff.
- **Async & IO (`vmux_task`)**: `TaskPlugin<T>`, `IoTaskPlugin<T>`, and abort-on-drop `TokioTask` tie background futures to entity lifetimes.
- **Blocking pools (`vmux_job`)**: `JobsPlugin` and `JobPlugin<J>` isolate blocking filesystem/CPU work in bounded worker lanes.
- **Channels, timers & file watching**: `AsyncChannelPlugin` + `#[vmux_ecs::channels]`, deterministic `Clock` (`Clock::fake`) + `DeadlinePlugin` + `RateLimiter`, and `FileWatchPlugin` (`vmux_watch`).

## Persistence and autosave

Declare persisted component sets with `#[vmux_ecs::store(track; ...)]`, register `SchemaPlugin<S>`, and attach `AutoSavePlugin<S>` + `AutoSavePolicy` + `StorePath` for debounced, retryable persistence with graceful exit flushes via `ShutdownPlugin`.

See [`examples/ecs/autosave_store.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/ecs/autosave_store.rs) and [`examples/window/notes.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/window/notes.rs).
