use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_app::ecs::{EffectiveStartupUrl, UiIpcSet};
use vmux_app::manifest::hosting::{
    Contract, HostedApplication, HostedApplicationReady, Package, WebHostPlugin,
};
use vmux_app::manifest::{FeatureManifestOwner, FeatureManifestSource, FeaturePlugin};
use vmux_app::prelude::*;

const SDK_REVISION: &str = "1111111111111111111111111111111111111111";

const PACKAGE: &str = r#"(
    schema_version: 1,
    sdk_revision: "1111111111111111111111111111111111111111",
    entry: "https://example.test/app/",
    manifest: (
        metadata: Some((
            id: "example.window_web_host",
            version: "1.0.0",
            dependencies: [(id: "native.ping", version: "^1.0")],
            capabilities: ["native.ping"],
            targets: ["headless"],
        )),
        pages: [(
            url: "https://example.test/app/",
            title: "Hosted Web App",
            startup: true,
            permissions: ["PageReady", "PingRequest"],
        )],
    ),
)"#;

struct NativePing;

impl FeatureManifestSource for NativePing {
    const SOURCE: &'static str =
        r#"(metadata: Some((id: "native.ping", version: "1.0.0", provides: ["native.ping"])))"#;
}

#[ui_state(Default, Eq)]
struct WebHostUiState {
    entry_url: String,
    total: u32,
}

#[ui_event]
#[derive(Message)]
struct PingRequest {
    count: u32,
}

#[derive(Component, Default)]
struct PingTotal(u32);

struct NativePingPlugin;

impl Plugin for NativePingPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<FeatureManifestOwner<NativePing>, PingTotal>()
            .add_plugins((
                FeaturePlugin::<NativePing>::default(),
                UiStatePlugin::<WebHostUiState>::default(),
                UiIpcEventPlugin::<PingRequest>::default(),
            ))
            .add_message::<PingRequest>()
            .add_observer(forward_ui_ping)
            .add_systems(
                Update,
                (handle_ping, publish_state)
                    .chain()
                    .after(UiIpcSet::Dispatch),
            );
    }
}

fn forward_ui_ping(trigger: On<UiIpcInput<PingRequest>>, mut requests: MessageWriter<PingRequest>) {
    requests.write(PingRequest {
        count: trigger.event().count,
    });
}

fn handle_ping(
    mut requests: MessageReader<PingRequest>,
    mut totals: Query<&mut PingTotal, With<FeatureManifestOwner<NativePing>>>,
) {
    for request in requests.read() {
        for mut total in &mut totals {
            total.0 += request.count;
        }
    }
}

fn publish_state(
    totals: Query<&PingTotal, With<FeatureManifestOwner<NativePing>>>,
    changed_totals: Query<(), (With<FeatureManifestOwner<NativePing>>, Changed<PingTotal>)>,
    urls: Query<&EffectiveStartupUrl, With<HostedApplication>>,
    pages: Query<Entity, (With<HostedPage>, With<UiIpcReady>)>,
    new_pages: Query<(), (With<HostedPage>, Added<UiIpcReady>)>,
    mut writer: UiStateWriter<WebHostUiState>,
) {
    if changed_totals.is_empty() && new_pages.is_empty() {
        return;
    }
    let total = totals.iter().next().map(|t| t.0).unwrap_or(0);
    let entry_url = urls
        .iter()
        .next()
        .map(|u| u.0.clone())
        .unwrap_or_else(|| "https://example.test/app/".to_string());
    for page in &pages {
        writer.push(
            page,
            WebHostUiState {
                entry_url: entry_url.clone(),
                total,
            },
        );
    }
}

#[component]
fn WebHostPageView() -> Element {
    let binding = use_ui_state::<WebHostUiState>();
    let state = binding.state.read().clone();
    rsx! {
        div { class: "web-host",
            h1 { "Vmux Web Host" }
            p { "Hosted entry: {state.entry_url}" }
            p { "Ping total: {state.total}" }
            button {
                onclick: move |_| {
                    let _ = send_ui_event(&PingRequest { count: 1 });
                },
                "Send Ping"
            }
        }
    }
}

static WEB_HOST_PAGE: Page = Page {
    permissions: &[
        WebHostUiState::PERMISSION,
        PingRequest::PERMISSION,
        vmux_app::api::PageReady::PERMISSION,
    ],
    ..Page::pane("app://web-host/", WebHostPageView).titled("Hosted Web App")
};

fn main() {
    let headless = std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some();
    let mut config = WindowHostConfig::new("Vmux Web Host")
        .size(960, 640)
        .page(&WEB_HOST_PAGE);
    if headless {
        config = config.headless();
    }
    let package = Package::try_parse(PACKAGE).expect("valid package");
    let contract = Contract::new(SDK_REVISION).event::<PingRequest>();

    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowHostPlugin::new(config)),
        NativePingPlugin,
        WebHostPlugin::new(contract, package),
    ));
    if headless {
        app.finish();
        app.cleanup();
        app.update();
        let page = app
            .world_mut()
            .query_filtered::<Entity, With<UiIpcReady>>()
            .single(app.world())
            .unwrap();
        let authority = (
            app.world().get::<UiIpcGeneration>(page).unwrap().0,
            app.world().get::<UiIpcNavigationRevision>(page).unwrap().0,
            app.world().get::<UiIpcEpoch>(page).unwrap().0,
        );
        app.world_mut().write_message(UiReceiveRaw::from_event(
            page,
            "app://web-host/",
            authority,
            &PingRequest { count: 4 },
        ));
        app.update();
        let world = app.world_mut();
        let url = world
            .query_filtered::<&EffectiveStartupUrl, With<HostedApplication>>()
            .single(world)
            .unwrap()
            .0
            .clone();
        let total = world.query::<&PingTotal>().single(world).unwrap().0;
        let ready = world.resource::<Messages<HostedApplicationReady>>().len();
        assert_eq!(total, 4);
        println!("window web_host smoke passed: entry={url} total={total} ready_events={ready}");
        return;
    }
    app.run();
}
