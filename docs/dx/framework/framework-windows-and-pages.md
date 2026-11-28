# Windows, pages, and menus

Vmux keeps ECS as the single authority for application state while rendering Dioxus pages inside lightweight native WebViews (`vmux_page` + `wry`), without requiring Chromium (`bevy_cef`).

## WindowHostPlugin

`WindowHostPlugin` (`vmux_app::window`) is the one-line reactive desktop window shell:

```rust
App::new().add_plugins(WindowHostPlugin::new(
    WindowHostConfig::new("My App")
        .size(1024, 720)
        .appearance(PageAppearance::Dark)
        .page(&MY_PAGE),
));
```

It composes:
- `WindowPlugin` with centered primary window and IME enabled.
- Reactive `WinitSettings::desktop_app()` and `WinitWakePlugin` (never `UpdateMode::Continuous`).
- `ShutdownPlugin` and `DisplayRelocationPlugin` (recenters windows when monitors disconnect).
- `PageHostPlugin` (`vmux_page/host`) and automatic attachment of `startup_page` as a full-window `PageRoot`.
- Multi-window messages: `OpenWindowRequest`, `CloseWindowRequest`, and `HideAllWindowsRequest`.

Call `.headless()` on `WindowHostConfig` to run the same plugin graph in tests or CI without opening an OS window.

## Page hosting and static assets

`PageHostPlugin` (`vmux_app::page`) manages `HostedPage` entities:

- **Single-page apps**: attach `PageRoot` alongside `HostedPage::new(&PAGE)` and `ChildOf(window)` to automatically fill the window viewport.
- **Multi-pane apps**: omit `PageRoot` and pair `HostedPage` with `vmux_flex::Node` / `ComputedNode` and `vmux_layout::Pane`. `PageHostPlugin` synchronizes each page's native bounds after `LayoutSystems::Layout`.
- **Static assets**: attach `PageStylesheet(css)` and/or shared `vmux_ecs::FilePath` directly to a `HostedPage` entity (or via `WindowHostConfig::stylesheet` / `.asset_directory`). The native Wry driver serves files over the page's custom scheme (`app://...`, `vmux://...`) with path-traversal protection and a minimal fallback stylesheet.
- **Appearance and scripts**: mutate `PageAppearance` (`Light`, `Dark`, `System`) on a page entity, or trigger `PageScriptRequest { page, script }` for targeted DOM evaluation.

## UiState and UiEvent contracts

UI IPC in `vmux_ecs::ui` is completely renderer-agnostic:

1. **Host → UI (`UiStatePlugin<S>`)**: write updates through `UiStateWriter<S>` or `UiStateWrite<S>`. `UiStatePlugin<S>` batches same-frame updates, enforces `UiEventPermissions` and authority (`UiIpcGeneration`, `UiIpcNavigationRevision`, `UiIpcEpoch`), emits `UiEmitEvent`, tracks delivery receipts with bounded retries, and replays the latest snapshot on `UiPageReady` or `Remove<UiIpcBlocked>`.
2. **UI → Host (`UiIpcEventPlugin<E>`)**: Dioxus pages send typed `#[ui_event]` values via `send_ui_event(&event)` and subscribe via `use_ui_state::<S>()`. `UiIpcEventPlugin<E>` validates authority, blocking state, and `UiEventPermissions` before triggering `On<UiIpcInput<E>>`.

## Declarative OS menus

Define native menu bars and keyboard accelerators with `#[vmux_app::menu::menu]` (`vmux_app::menu`):

```rust
#[vmux_app::menu::menu(standard_edit = "Edit")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppMenu {
    #[submenu("File")]
    #[item("New Note", id = "new_note", accelerator = "super+n")]
    NewNote,
}
```

Install `MenuPlugin` and `MenuActionPlugin::<AppMenu>::default()` (plus `NativeMenuPlugin` when `menu-native` is enabled), then handle `MessageReader<Selected<AppMenu>>`. See [`examples/menu/declarative_menu.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/menu/declarative_menu.rs) and [`examples/window/split_workspace.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/window/split_workspace.rs).

## Hosting an existing web application

Enable `vmux_app/hosting` to host an existing HTTP(S) web app with a versioned RON package envelope pinned to the host's 40-character SDK revision:

```ron
(
    schema_version: 1,
    sdk_revision: "<the host's full 40-character SDK commit>",
    entry: "https://example.test/app/",
    manifest: (
        metadata: Some((id: "example.web", version: "1.0.0")),
        pages: [(
            url: "https://example.test/app/",
            title: "Example",
            startup: true,
            permissions: ["PageReady"],
        )],
    ),
)
```

Decode with `Package::try_parse` and add `WebHostPlugin::new(Contract::new(verified_sdk_revision), package)`. Web packages remain presentation-only; native behavior comes from statically linked Rust plugins declared in `metadata.dependencies` and exposed via `Contract::event::<Request>()`. See [`examples/window/web_host.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/window/web_host.rs) and [`examples/manifest/web_host.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/manifest/web_host.rs).
