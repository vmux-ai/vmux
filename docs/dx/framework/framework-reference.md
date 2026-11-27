# Crates and macros reference

## Public namespaces and Cargo features

Enable capabilities on `vmux_app` via Cargo features, then install only the plugins your target needs:

| Public namespace | `vmux_app` Cargo feature | Available behavior |
|---|---|---|
| `vmux_app::bevy` | default (`app`), `ecs` (`ecs`) | Re-exported `bevy_app` and `bevy_ecs` matching the pinned SDK Bevy minor (`0.19`). |
| `vmux_app::api` | `contracts` | Shared typed contracts (`UiState`, `UiEvent`, `HostEvent`, `UiEventPermissions`, `UiStateFrame`, `PageReady`). |
| `vmux_app::ecs` | `ecs`; optionally `ui`, `watch`, `contracts` | Core entity vocabulary, scoped identity, `OperationPlugin` policies, leases, generations, dirty/selection, `ShutdownPlugin`, `TaskPlugin`/`IoTaskPlugin`, `channel`, `time`, renderer-free `ui` + `UiStatePlugin`, and optional `watch`. |
| `vmux_app::page` | `page`; `page-host`, `page-host-native` | `Page`, `PagePlugin`, `#[page]`, plus CEF-free Wry + Dioxus `PageHostPlugin`, `HostedPage`, `PageRoot`, `PageStylesheet` + `FilePath`, `PageBounds`, `PageAppearance`, and `PageScriptRequest`. |
| `vmux_app::window` | `window`; `window-native` | Reusable reactive `WindowHostPlugin`, `WindowHostConfig`, `DisplayRelocationPlugin`, and window lifecycle requests. |
| `vmux_app::ui` | `ui` | Lean `vmux_ui` component kit (`default-features = false`), theme, Fluent i18n, `use_ui_state`, and `send`. |
| `vmux_app::job` | `jobs` | Bounded multi-lane blocking worker pools and owned `JobPlugin` lifecycle. |
| `vmux_app::store` | `store` | Declarative `#[store]` schemas, world serialization, validation, and `AutoSavePlugin`. |
| `vmux_app::runner` | `runner` (`winit` via `window-native` / `runtime`) | Wake-driven `HeadlessTokioRunner` and `WinitWakePlugin`. |
| `vmux_app::http` | `http-server` | ECS-native `HttpServerPlugin` with channel-backed request entities and `ShutdownPlugin` drain. |
| `vmux_app::quic` | `quic-server` | ECS-native `QuicServerPlugin` for authenticated QUIC endpoints. |
| `vmux_app::menu` | `menu`; `menu-native` | Declarative `#[vmux_app::menu::menu]`, `MenuPlugin`, `MenuActionPlugin`, and OS `NativeMenuPlugin`. |
| `vmux_app::flex` | `flex` | Renderer-independent flexbox layout and `WindowViewportPlugin`. |
| `vmux_app::layout` | `layout`; `layout-runtime` | Owner-scoped pane topology and selection. |
| `vmux_app::project` | `project` | Project registration, archive, and persistence requests. |
| `vmux_app::extension` | `extension-core`; `extension` | Extension catalog and installation lifecycle. |
| `vmux_app::lsp` | `lsp` | Server, document, request, and diagnostics lifecycles (transport supplied by app). |
| `vmux_app::manifest` | `manifest`; `hosting` | `FeatureManifest`, `FeaturePlugin`, `CompositionPlugin`, `Profile`, `WebHostPlugin`, and `Contract`. |
| `vmux_app::host` | `host` | Compiled product identity and isolated `ProfilePaths` / `ServicePaths`. |

## Attribute macros cheat sheet

| Macro | Purpose |
|---|---|
| `#[vmux_api::ui_state]` | Declares a projected `UiState` contract (optionally `patch = MyPatch`) serialized via `rkyv`. |
| `#[vmux_api::ui_state_patch]` | Declares a delta patch type applied to a `UiState`. |
| `#[vmux_api::ui_event]` | Declares a typed intent sent from pages to ECS. |
| `#[vmux_api::host_event]` | Declares a one-way host-to-UI binary event. |
| `#[vmux_page::page(...)]` | Binds a Dioxus `#[component]` and declared state/event contracts to a `Page`. |
| `#[vmux_app::menu::menu]` | Generates declarative menu bar / submenu / accelerator definitions for `MenuActionPlugin`. |
| `#[vmux_ecs::store(track; ...)]` | Declares a persisted store schema and change-tracking observers for `AutoSavePlugin`. |
| `#[vmux_ecs::channels]` | Eliminates boilerplate when draining multiple `AsyncChannelPlugin` streams. |
| `#[vmux_macro::app_plugin]` | Composes Cargo-feature-gated plugin graphs with transitive dependency resolution. |

## Platform support matrix

| Target | Core ECS / Headless / Runner / HTTP / QUIC | Native Desktop Window (`vmux_window` + `vmux_page`) | Product Host |
|---|---|---|---|
| macOS (Apple Silicon) | Verified | Verified (Wry + Dioxus, CEF-free) | Verified (`vmux_desktop`) |
| Linux (CI) | Verified | Headless & compile-checked (native WebKitGTK planned) | Headless / CLI / Daemon |
| iOS | Core verified | `vmux_page` Wry support | Verified (`vmux_mobile`) |
| Windows / Android / Web | Unverified | Planned | Unverified |
