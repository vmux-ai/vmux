use bevy_app::PostStartup;
use bevy_ecs::prelude::*;
use dioxus::prelude::*;
use vmux_app::flex::{FlexDirection, Val};
use vmux_app::menu::menu as app_menu;
use vmux_app::prelude::*;

#[app_menu]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceMenuAction {
    #[submenu("Workspace")]
    #[item("Focus Left Pane", id = "focus_left", accelerator = "super+1")]
    FocusLeft,
    #[item("Focus Right Pane", id = "focus_right", accelerator = "super+2")]
    FocusRight,
}

#[derive(Component)]
struct LeftPane;

#[derive(Component)]
struct RightPane;

#[derive(Component)]
struct PanePage(Entity);

struct WorkspaceDemoPlugin {
    headless: bool,
}

impl Plugin for WorkspaceDemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MenuActionPlugin::<WorkspaceMenuAction>::default());
        if !self.headless {
            app.add_plugins(NativeMenuPlugin);
        }
        app.add_systems(PostStartup, setup_workspace)
            .add_systems(Update, (handle_menu_actions, focus_selected_pane).chain());
    }
}

fn setup_workspace(windows: Query<Entity, With<HostWindow>>, mut commands: Commands) {
    let Ok(window) = windows.single() else {
        return;
    };
    let root = commands.spawn(Name::new("Workspace")).id();
    let flex_root = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                ..Default::default()
            },
            FlexViewport(window),
            ChildOf(window),
        ))
        .id();
    let left_page = commands
        .spawn((
            HostedPage::new(&LEFT_PANE_PAGE),
            Node {
                flex_grow: 1.0,
                height: Val::Percent(100.0),
                ..Default::default()
            },
            ChildOf(flex_root),
        ))
        .id();
    let right_page = commands
        .spawn((
            HostedPage::new(&RIGHT_PANE_PAGE),
            Node {
                flex_grow: 1.0,
                height: Val::Percent(100.0),
                ..Default::default()
            },
            ChildOf(flex_root),
        ))
        .id();
    let left = commands
        .spawn((Pane, LeftPane, PanePage(left_page), ChildOf(root)))
        .id();
    let _right = commands
        .spawn((Pane, RightPane, PanePage(right_page), ChildOf(root)))
        .id();
    commands.write_message(PaneSelectRequest { root, pane: left });
}

fn handle_menu_actions(
    mut selected: MessageReader<MenuActionSelected<WorkspaceMenuAction>>,
    items: Query<&WorkspaceMenuAction>,
    roots: Query<Entity, With<Name>>,
    left: Query<Entity, With<LeftPane>>,
    right: Query<Entity, With<RightPane>>,
    mut requests: MessageWriter<PaneSelectRequest>,
) {
    let Some(root) = roots.iter().next() else {
        return;
    };
    if items.is_empty() {
        return;
    }
    for MenuActionSelected(action) in selected.read() {
        match action {
            WorkspaceMenuAction::FocusLeft => {
                if let Ok(pane) = left.single() {
                    requests.write(PaneSelectRequest { root, pane });
                }
            }
            WorkspaceMenuAction::FocusRight => {
                if let Ok(pane) = right.single() {
                    requests.write(PaneSelectRequest { root, pane });
                }
            }
        }
    }
}

fn focus_selected_pane(
    mut changes: MessageReader<PaneSelectionChanged>,
    panes: Query<&PanePage>,
    mut commands: Commands,
) {
    for change in changes.read() {
        if let Ok(pane_page) = panes.get(change.pane) {
            commands.trigger(PageFocusRequest { page: pane_page.0 });
        }
    }
}

#[component]
fn LeftPaneView() -> Element {
    rsx! { div { "Left Pane" } }
}

#[component]
fn RightPaneView() -> Element {
    rsx! { div { "Right Pane" } }
}

static LEFT_PANE_PAGE: Page = Page::pane("app://left/", LeftPaneView).titled("Left");
static RIGHT_PANE_PAGE: Page = Page::pane("app://right/", RightPaneView).titled("Right");

fn main() {
    let headless = std::env::var_os("VMUX_EXAMPLE_HEADLESS").is_some();
    let mut config = WindowHostConfig::new("Vmux Split Workspace").size(1024, 640);
    if headless {
        config = config.headless();
    }
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowHostPlugin::new(config)),
            WorkspaceDemoPlugin { headless },
        ))
        .run();
}
