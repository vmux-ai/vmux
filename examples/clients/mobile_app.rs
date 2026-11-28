use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_app::prelude::*;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
enum MobileTab {
    #[default]
    Home,
    Sessions,
    Settings,
}

#[ui_state(Default, Eq)]
struct MobileShellUiState {
    active_tab: MobileTab,
    connected_host: String,
    session_count: u32,
}

#[ui_event(Eq)]
struct SelectMobileTab {
    tab: MobileTab,
}

#[derive(Component)]
struct MobileScreen;

#[derive(Component)]
struct MobileModel {
    active_tab: MobileTab,
    connected_host: String,
    session_count: u32,
}

#[derive(Resource, Default)]
struct LastRenderedHtml(String);

struct MobileClientPlugin;

impl Plugin for MobileClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MinimalPlugins,
            UiIpcPlugin,
            UiStatePlugin::<MobileShellUiState>::default(),
            UiIpcEventPlugin::<SelectMobileTab>::default(),
        ))
        .init_resource::<LastRenderedHtml>()
        .add_systems(Startup, spawn_mobile_screen)
        .add_observer(handle_tab_select)
        .add_systems(PostUpdate, render_mobile_ui.after(UiStateFlushSet));
    }
}

fn spawn_mobile_screen(mut writer: UiStateWriter<MobileShellUiState>, mut commands: Commands) {
    let screen = commands
        .spawn((
            MobileScreen,
            MobileModel {
                active_tab: MobileTab::Home,
                connected_host: String::from("workstation.local"),
                session_count: 3,
            },
            vmux_app::ecs::PageMetadata {
                title: String::from("Mobile Shell"),
                url: String::from("mobile://shell/"),
                icon: vmux_app::api::PageIcon::None,
                bg_color: None,
            },
            UiEventPermissions {
                url: "mobile://shell/",
                owns_subtree: true,
                permissions: &[MobileShellUiState::PERMISSION, SelectMobileTab::PERMISSION],
            },
            UiIpcReady,
            UiIpcGeneration(1),
            UiIpcNavigationRevision(1),
            UiIpcEpoch(1),
        ))
        .id();
    writer.push(
        screen,
        MobileShellUiState {
            active_tab: MobileTab::Home,
            connected_host: String::from("workstation.local"),
            session_count: 3,
        },
    );
}

fn handle_tab_select(
    trigger: On<UiIpcInput<SelectMobileTab>>,
    mut screens: Query<&mut MobileModel>,
    mut writer: UiStateWriter<MobileShellUiState>,
) {
    let page = trigger.event().page;
    let Ok(mut model) = screens.get_mut(page) else {
        return;
    };
    model.active_tab = trigger.event().tab;
    writer.push(
        page,
        MobileShellUiState {
            active_tab: model.active_tab,
            connected_host: model.connected_host.clone(),
            session_count: model.session_count,
        },
    );
}

fn render_mobile_ui(
    screens: Query<&MobileModel, With<MobileScreen>>,
    mut rendered: ResMut<LastRenderedHtml>,
) {
    let Ok(model) = screens.single() else {
        return;
    };
    let state = MobileShellUiState {
        active_tab: model.active_tab,
        connected_host: model.connected_host.clone(),
        session_count: model.session_count,
    };
    rendered.0 = dioxus_ssr::render_element(rsx! {
        MobileShellView { state }
    });
    println!("{}", rendered.0);
}

#[component]
fn MobileShellView(state: MobileShellUiState) -> Element {
    let heading = match state.active_tab {
        MobileTab::Home => "Home",
        MobileTab::Sessions => "Sessions",
        MobileTab::Settings => "Settings",
    };
    rsx! {
        section { class: "mobile-shell",
            header {
                h1 { "{heading}" }
                span { "{state.connected_host}" }
            }
            main {
                match state.active_tab {
                    MobileTab::Home => rsx! { p { "Connected to {state.connected_host} ({state.session_count} live sessions)" } },
                    MobileTab::Sessions => rsx! { p { "Showing {state.session_count} active remote sessions" } },
                    MobileTab::Settings => rsx! { p { "Paired host: {state.connected_host}" } },
                }
            }
            nav {
                span { "Home" }
                span { "Sessions" }
                span { "Settings" }
            }
        }
    }
}

fn main() {
    App::new().add_plugins(MobileClientPlugin).run();
}
