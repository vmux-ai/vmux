# Framework quickstart

Get a Vmux Framework application running in minutes—either as a CEF-free native desktop window app or a renderer-free headless ECS service.

## 1. Run an existing example

From a Vmux checkout, run any headless example directly:

```sh
cargo run -p vmux_examples --example selection
cargo run -p vmux_examples --example operation_policy
cargo run -p vmux_examples --example panes
```

On macOS, run a native desktop window example (`vmux_window` + `vmux_page`, zero Chromium/CEF):

```sh
cargo run -p vmux_examples --features window --example window_counter
cargo run -p vmux_examples --features window --example window_notes
cargo run -p vmux_examples --features window --example window_split_workspace
```

Set `VMUX_EXAMPLE_HEADLESS=1` to run the same windowed examples headlessly in CI or a terminal.

## 2. Build a minimal desktop window app

Enable `window-native` and `ui` on `vmux_app`, plus `dioxus` and `vmux_api`:

```toml
[dependencies]
bevy_ecs = "0.19"
dioxus = "=0.7.9"
rkyv = { version = "0.8", features = ["alloc"] }
serde = { version = "1", features = ["derive"] }
vmux_api = { git = "https://github.com/vmux-ai/vmux", rev = "<commit>" }
vmux_app = { git = "https://github.com/vmux-ai/vmux", rev = "<commit>", features = ["window-native", "ui"] }
```

Define one `#[ui_state]`, one `#[ui_event]`, one Dioxus `Page`, and compose them with `WindowHostPlugin`:

```rust
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_app::prelude::*;

#[ui_state(Default, Eq)]
struct CounterUiState {
    count: i32,
}

#[ui_event(Eq)]
struct CounterDelta {
    delta: i32,
}

#[derive(Component, Default)]
struct CounterValue(i32);

struct CounterPlugin;

impl Plugin for CounterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            UiStatePlugin::<CounterUiState>::default(),
            UiIpcEventPlugin::<CounterDelta>::default(),
        ))
        .add_observer(attach_counter)
        .add_observer(apply_delta);
    }
}

fn attach_counter(
    trigger: On<Add, UiIpcReady>,
    pages: Query<(), With<HostedPage>>,
    mut writer: UiStateWriter<CounterUiState>,
    mut commands: Commands,
) {
    let page = trigger.event_target();
    if pages.contains(page) {
        commands.entity(page).try_insert(CounterValue(0));
        writer.push(page, CounterUiState { count: 0 });
    }
}

fn apply_delta(
    trigger: On<UiIpcInput<CounterDelta>>,
    mut counters: Query<&mut CounterValue>,
    mut writer: UiStateWriter<CounterUiState>,
) {
    let page = trigger.event().page;
    if let Ok(mut value) = counters.get_mut(page) {
        value.0 += trigger.event().delta;
        writer.push(page, CounterUiState { count: value.0 });
    }
}

#[component]
fn CounterPageView() -> Element {
    let binding = use_ui_state::<CounterUiState>();
    let count = binding.state.read().count;
    rsx! {
        div {
            span { "{count}" }
            button { onclick: move |_| { let _ = send_ui_event(&CounterDelta { delta: 1 }); }, "+" }
        }
    }
}

static COUNTER_PAGE: Page = Page {
    permissions: &[
        CounterUiState::PERMISSION,
        CounterDelta::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://counter/", CounterPageView).titled("Counter")
};

fn main() {
    App::new()
        .add_plugins((
            WindowHostPlugin::new(
                WindowHostConfig::new("Counter")
                    .size(480, 320)
                    .page(&COUNTER_PAGE),
            ),
            CounterPlugin,
        ))
        .run();
}
```

See the full runnable version in [`examples/window/counter.rs`](https://github.com/vmux-ai/vmux/blob/main/examples/window/counter.rs).

## 3. Build a headless ECS service

Headless apps omit `window` and `ui` completely so zero windowing, WebView, or Chromium crates compile:

```rust
use vmux_app::prelude::*;

let mut app = App::new();
app.add_plugins((
    VmuxPlugin::builder().layout(true).build(),
    SelectionPlugin::<()>::default(),
));
app.update();
```

For long-running background daemons and HTTP/QUIC servers, compose `vmux_app::runner::HeadlessTokioRunner` with `ShutdownPlugin`, `vmux_app::http::HttpServerPlugin`, or `vmux_app::quic::QuicServerPlugin`.

## Next steps

- Learn the ownership and composition model in [Core concepts](framework-core-concepts.md).
- Wire multi-pane layouts, menus, and assets in [Windows, pages, and menus](framework-windows-and-pages.md).
