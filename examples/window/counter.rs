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
    if !pages.contains(page) {
        return;
    }
    commands.entity(page).try_insert(CounterValue(0));
    writer.push(page, CounterUiState { count: 0 });
}

fn apply_delta(
    trigger: On<UiIpcInput<CounterDelta>>,
    mut counters: Query<&mut CounterValue>,
    mut writer: UiStateWriter<CounterUiState>,
) {
    let page = trigger.event().page;
    let Ok(mut value) = counters.get_mut(page) else {
        return;
    };
    value.0 += trigger.event().delta;
    writer.push(page, CounterUiState { count: value.0 });
}

#[component]
fn CounterPageView() -> Element {
    let binding = use_ui_state::<CounterUiState>();
    let count = binding.state.read().count;
    rsx! {
        div { class: "root",
            h1 { "Vmux Counter" }
            div { class: "controls",
                button { onclick: move |_| { let _ = send_ui_event(&CounterDelta { delta: -1 }); }, "-" }
                span { class: "value", "{count}" }
                button { onclick: move |_| { let _ = send_ui_event(&CounterDelta { delta: 1 }); }, "+" }
            }
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

const CSS: &str = "body{background:#111318;color:#f3f4f6;display:flex;align-items:center;justify-content:center;font-family:system-ui,sans-serif}.root{text-align:center}.controls{display:flex;gap:16px;align-items:center;justify-content:center;margin-top:16px}button{padding:8px 18px;font-size:18px;border-radius:8px;border:1px solid #334155;background:#1e293b;color:#fff;cursor:pointer}.value{font-size:28px;min-width:48px;font-variant-numeric:tabular-nums}";

fn main() {
    let mut config = WindowHostConfig::new("Vmux Counter")
        .size(480, 320)
        .page(&COUNTER_PAGE)
        .stylesheet(CSS);
    if std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some() {
        config = config.headless();
    }
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            CounterPlugin,
        ))
        .run();
}
