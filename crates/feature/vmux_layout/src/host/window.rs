use crate::{
    Header, LayoutStartupSet, TabLayoutSpawnContent, TabLayoutSpawnRequest,
    cef::layout_cef_bundle,
    pane::{Pane, PaneSplit, PaneSplitDirection, leaf_pane_bundle, pane_split_gaps},
    settings::LayoutSettings,
    side_sheet::{SideSheet, SideSheetPosition},
    stack::stack_bundle,
    tab::{Tab, tab_bundle},
    unit::WindowExt,
};
use bevy::{
    asset::Asset, ecs::system::SystemParam, prelude::*, window::PrimaryWindow, winit::WINIT_WINDOWS,
};
use bevy_cef::prelude::*;
use moonshine_save::prelude::*;
use vmux_command::{BindCommands, CommandInvocation, CommandRegistry, CommandRuntimePlugin};
#[cfg(test)]
use vmux_core::host::manifest::FeaturePlugin;
use vmux_core::host::persistence::PersistenceAppExt;
use vmux_core::page::PageEmbedSet;
use vmux_core::{
    Active, EffectiveStartupUrl, Order, PageOpenRequest, PageOpenSet, PageOpenTarget, PendingPrompt,
};
use vmux_flex::prelude::*;
use vmux_history::{CreatedAt, LastActivatedAt};

use super::command::LayoutRequestSet;

pub struct WindowLayoutPlugin;

impl Plugin for WindowLayoutPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(test)]
        app.add_plugins(FeaturePlugin::<crate::Feature>::default());
        if !app.is_plugin_added::<CommandRuntimePlugin>() {
            app.add_plugins(CommandRuntimePlugin);
        }
        app.add_plugins(LayoutSpawnPlugin)
            .add_message::<MinimizeWindowRequest>()
            .add_message::<NewWindowRequest>()
            .add_message::<CloseFocusedWindowRequest>()
            .add_message::<ToggleFullscreenRequest>()
            .register_persisted::<WindowGeometry>()
            .register_type::<Option<IVec2>>()
            .register_type::<Option<Vec2>>()
            .add_systems(
                Startup,
                setup_shells
                    .in_set(LayoutStartupSet::Window)
                    .after(PageEmbedSet),
            )
            .add_systems(
                Startup,
                request_default.in_set(LayoutStartupSet::DefaultTab),
            )
            .add_systems(Startup, bind_commands.in_set(BindCommands))
            .add_systems(
                Startup,
                fit_to_screen
                    .in_set(LayoutStartupSet::Post)
                    .after(crate::stack::OpenStartupPageSet),
            )
            .add_systems(
                PostUpdate,
                (fit_to_screen, sync_to_settings, sync_main_column_gap),
            )
            .add_systems(
                Update,
                (sync_focused, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(WindowFocusSet),
            )
            .add_systems(
                Update,
                (setup_shells, bevy::ecs::schedule::ApplyDeferred)
                    .chain()
                    .in_set(WindowShellSet)
                    .after(WindowFocusSet),
            )
            .add_systems(Update, minimize_focused.in_set(LayoutRequestSet::Handle));

        app.init_resource::<Assets<WindowMaterial>>()
            .init_resource::<WindowBackground>();
    }
}

pub(crate) struct LayoutSpawnPlugin;

impl Plugin for LayoutSpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::LauncherDismissRequest>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_message::<PageOpenRequest>()
            .add_systems(
                Update,
                spawn_requested_tab_layouts
                    .in_set(TabLayoutSpawnSet)
                    .after(LayoutRequestSet::Handle)
                    .after(crate::stack::OpenStartupPageSet)
                    .after(WindowShellSet)
                    .before(PageOpenSet::ResolveTarget),
            );
    }
}

#[derive(SystemParam)]
pub struct FocusedWindow<'w, 's> {
    windows: Query<'w, 's, (Entity, Ref<'static, Active>), With<Window>>,
}

impl FocusedWindow<'_, '_> {
    pub fn entity(&self) -> Option<Entity> {
        self.windows.iter().next().map(|(entity, _)| entity)
    }

    pub fn is_changed(&self) -> bool {
        self.windows.iter().any(|(_, active)| active.is_changed())
    }
}

#[derive(Message)]
struct MinimizeWindowRequest;

impl TryFrom<&CommandInvocation> for MinimizeWindowRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "minimize_window")
            .then_some(Self)
            .ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewWindowRequest;

impl TryFrom<&CommandInvocation> for NewWindowRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "new_window").then_some(Self).ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseFocusedWindowRequest;

impl TryFrom<&CommandInvocation> for CloseFocusedWindowRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "close_window").then_some(Self).ok_or(())
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleFullscreenRequest;

impl TryFrom<&CommandInvocation> for ToggleFullscreenRequest {
    type Error = ();

    fn try_from(invocation: &CommandInvocation) -> Result<Self, Self::Error> {
        (invocation.id == "toggle_fullscreen")
            .then_some(Self)
            .ok_or(())
    }
}

fn bind_commands(registry: CommandRegistry, mut commands: Commands) {
    registry.message::<MinimizeWindowRequest>(&mut commands);
    registry.message::<NewWindowRequest>(&mut commands);
    registry.message::<CloseFocusedWindowRequest>(&mut commands);
    registry.message::<ToggleFullscreenRequest>(&mut commands);
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowFocusSet;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct WindowShellSet;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TabLayoutSpawnSet;

#[derive(Component)]
pub struct NewWindowWorkspace;

pub const SIDE_SHEET_TOP_PADDING_PX: f32 = 22.0;

pub const WEBVIEW_Z_BASE: f32 = 0.01;
pub const WEBVIEW_Z_MAIN: f32 = 0.018;
pub const WEBVIEW_Z_FOCUS_RING: f32 = 0.02;
pub const WEBVIEW_Z_HEADER: f32 = 0.022;
pub const WEBVIEW_Z_SIDE_SHEET: f32 = 0.022;
pub const WEBVIEW_Z_MODAL: f32 = 0.06;
pub const WEBVIEW_MESH_DEPTH_BIAS: f32 = 0.0;

const _: () = {
    assert!(WEBVIEW_Z_BASE < WEBVIEW_Z_MAIN);
    assert!(WEBVIEW_Z_MAIN <= 0.025);
    assert!(WEBVIEW_Z_FOCUS_RING > WEBVIEW_Z_MAIN);
    assert!(WEBVIEW_Z_HEADER <= 0.03);
    assert!(WEBVIEW_Z_SIDE_SHEET <= 0.03);
    assert!(WEBVIEW_Z_MODAL <= 0.08);
    assert!(WEBVIEW_MESH_DEPTH_BIAS >= 0.0);
};

#[derive(Asset, TypePath)]
pub struct WindowMaterial;

pub const WINDOW_BACKGROUND_SRGB: [f32; 3] = [0.13, 0.13, 0.14];

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct WindowBackground(pub Color);

impl Default for WindowBackground {
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            Self(Color::NONE)
        } else {
            Self(Color::BLACK)
        }
    }
}

fn minimize_focused(
    mut reader: MessageReader<MinimizeWindowRequest>,
    focused_window: FocusedWindow,
) {
    for _ in reader.read() {
        let Some(entity) = focused_window.entity() else {
            continue;
        };
        WINIT_WINDOWS.with_borrow(|winit_windows| {
            if let Some(winit_win) = winit_windows.get_window(entity) {
                winit_win.set_minimized(true);
            }
        });
    }
}

#[derive(Bundle)]
struct WindowBundle {
    marker: VmuxWindow,
    host_window: HostWindow,
    viewport: FlexViewport,
    surface: WindowSurface,
    transform: Transform,
    node: Node,
}

#[derive(Component)]
pub struct VmuxWindow;

#[derive(Component)]
pub struct Main;

#[derive(Component)]
pub struct MainColumn;

#[derive(Component)]
pub struct WindowSurface;

#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::window"]
#[require(Save)]
pub struct WindowGeometry {
    pub fullscreen: bool,
    pub position: Option<IVec2>,
    pub size: Option<Vec2>,
}

fn sync_focused(
    windows: Query<(Entity, &Window, Has<PrimaryWindow>, Has<Active>)>,
    mut commands: Commands,
) {
    let next = windows
        .iter()
        .find_map(|(entity, window, _, _)| (window.visible && window.focused).then_some(entity))
        .or_else(|| {
            windows.iter().find_map(|(entity, window, _, active)| {
                (active && window.visible).then_some(entity)
            })
        })
        .or_else(|| {
            windows.iter().find_map(|(entity, window, primary, _)| {
                (primary && window.visible).then_some(entity)
            })
        })
        .or_else(|| {
            windows
                .iter()
                .find_map(|(entity, window, _, _)| window.visible.then_some(entity))
        })
        .or_else(|| windows.iter().next().map(|(entity, _, _, _)| entity));
    for (entity, _, _, active) in &windows {
        if next == Some(entity) && !active {
            commands.entity(entity).insert(Active);
        } else if next != Some(entity) && active {
            commands.entity(entity).remove::<Active>();
        }
    }
}

#[derive(SystemParam)]
pub struct WindowHierarchy<'w, 's> {
    child_of: Query<'w, 's, &'static ChildOf>,
    host_windows: Query<'w, 's, &'static HostWindow>,
}

impl WindowHierarchy<'_, '_> {
    pub fn get(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            if let Ok(host) = self.host_windows.get(current) {
                return Some(host.0);
            }
            current = self.child_of.get(current).ok()?.parent();
        }
    }
}

fn setup_shells(
    windows: Query<(Entity, &Window, Has<NewWindowWorkspace>)>,
    roots: Query<&HostWindow, With<VmuxWindow>>,
    spaces: Query<&crate::space::SpaceId, With<crate::space::Space>>,
    focused_space: crate::space::FocusedSpace,
    mut requests: MessageWriter<TabLayoutSpawnRequest>,
    mut commands: Commands,
    settings: Res<LayoutSettings>,
) {
    let mut existing_space_ids: std::collections::HashSet<String> =
        spaces.iter().map(|id| id.0.clone()).collect();
    let mut next_space_number = existing_space_ids.len() + 1;
    for (window_entity, window, new_workspace) in &windows {
        if roots.iter().any(|root| root.0 == window_entity) {
            continue;
        }
        let main = spawn_window_shell(&mut commands, window_entity, window, &settings);
        if new_workspace {
            while existing_space_ids.contains(&format!("space-{next_space_number}")) {
                next_space_number += 1;
            }
            let id = format!("space-{next_space_number}");
            let name = format!("Space {next_space_number}");
            existing_space_ids.insert(id.clone());
            next_space_number += 1;
            spawn_new_window_workspace(
                &mut commands,
                &mut requests,
                window_entity,
                main,
                id,
                name,
                focused_space.startup_url().map(str::to_string),
            );
        }
    }
}

fn spawn_window_shell(
    commands: &mut Commands,
    window_entity: Entity,
    window: &Window,
    settings: &LayoutSettings,
) -> Entity {
    let m = window.meters();

    let root_commands = commands.spawn(WindowBundle {
        marker: VmuxWindow,
        host_window: HostWindow(window_entity),
        viewport: FlexViewport(window_entity),
        surface: WindowSurface,
        transform: Transform {
            translation: Vec3::new(0.0, m.y * 0.5, 0.0),
            scale: Vec3::new(m.x, m.y, 1.0),
            ..default()
        },
        node: Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Relative,
            flex_direction: FlexDirection::Row,
            padding: UiRect {
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(settings.window.pad_right()),
                bottom: Val::Px(settings.window.pad_bottom()),
            },
            column_gap: Val::Px(crate::event::PANE_GAP_PX),
            ..default()
        },
    });
    let root = root_commands.id();

    let _left_side_sheet = commands
        .spawn((
            SideSheet,
            SideSheetPosition::Left,
            crate::Open,
            Transform::default(),
            Visibility::Visible,
            Node {
                width: Val::Px(
                    crate::event::SideSheetResizeEvent::live(settings.side_sheet.width).clamped(),
                ),
                min_height: Val::Px(0.0),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Column,
                padding: UiRect {
                    top: Val::Px(SIDE_SHEET_TOP_PADDING_PX),
                    ..default()
                },
                ..default()
            },
            ChildOf(root),
        ))
        .id();

    let main_column = commands
        .spawn((
            MainColumn,
            Transform::default(),
            Node {
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                min_width: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(0.0),
                ..default()
            },
            ChildOf(root),
        ))
        .id();

    commands.spawn((
        Header,
        crate::Open,
        Visibility::Visible,
        Transform::default(),
        Node {
            height: Val::Px(crate::event::CEF_RESERVED_HEIGHT_PX),
            flex_shrink: 0.0,
            ..default()
        },
        ChildOf(main_column),
    ));

    let main = commands
        .spawn((
            Main,
            Transform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                ..default()
            },
            ChildOf(main_column),
        ))
        .id();

    commands.spawn((
        SideSheet,
        SideSheetPosition::Right,
        crate::Open,
        Node {
            width: Val::Px(280.0),
            position_type: PositionType::Absolute,
            right: Val::Px(settings.window.pad_right()),
            top: Val::Px(settings.window.pad_top()),
            bottom: Val::Px(settings.window.pad_bottom()),
            display: Display::None,
            ..default()
        },
        ChildOf(root),
    ));

    commands.spawn((
        SideSheet,
        SideSheetPosition::Bottom,
        crate::Open,
        Node {
            height: Val::Px(200.0),
            position_type: PositionType::Absolute,
            left: Val::Px(settings.window.pad_left()),
            right: Val::Px(settings.window.pad_right()),
            bottom: Val::Px(settings.window.pad_bottom()),
            display: Display::None,
            ..default()
        },
        ChildOf(root),
    ));

    commands.spawn((layout_cef_bundle(window_entity), ChildOf(root)));
    main
}

fn spawn_new_window_workspace(
    commands: &mut Commands,
    requests: &mut MessageWriter<TabLayoutSpawnRequest>,
    window: Entity,
    main: Entity,
    id: String,
    name: String,
    startup_url: Option<String>,
) {
    let space = commands
        .spawn((
            crate::space::Space,
            crate::space::SpaceId(id.clone()),
            Name::new(name),
            Order(0),
            Active,
            EffectiveStartupUrl(startup_url.clone().unwrap_or_default()),
            LastActivatedAt::now(),
            crate::space::space_view_bundle(),
            ChildOf(main),
        ))
        .id();
    requests.write(TabLayoutSpawnRequest {
        space,
        primary_window: window,
        name: None,
        startup_dir: None,
        content: startup_url
            .filter(|url| !url.is_empty())
            .map(|url| TabLayoutSpawnContent::Url {
                url,
                pending_prompt: None,
            })
            .unwrap_or(TabLayoutSpawnContent::StartupUrlOrPrompt),
        clear_pending_stack: false,
        focus: true,
    });
    commands.entity(window).remove::<NewWindowWorkspace>();
}

fn request_default(
    tab_q: Query<(), With<Tab>>,
    hierarchy: WindowHierarchy,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    focused_space: crate::space::FocusedSpace,
    mut requests: MessageWriter<TabLayoutSpawnRequest>,
) {
    if !tab_q.is_empty() {
        return;
    }

    let Some((space, startup_dir)) = focused_space.get() else {
        return;
    };
    requests.write(TabLayoutSpawnRequest {
        space,
        primary_window: hierarchy
            .get(space)
            .or_else(|| primary_window.single().ok())
            .unwrap_or(Entity::PLACEHOLDER),
        name: None,
        startup_dir: startup_dir.clone(),
        content: TabLayoutSpawnContent::StartupUrlOrPrompt,
        clear_pending_stack: false,
        focus: true,
    });
}

pub struct TabScaffold {
    pub tab: Entity,
    pub pane: Entity,
    pub stack: Entity,
}

pub fn spawn_tab_scaffold_in_space(
    commands: &mut Commands,
    space: Entity,
    primary_window: Entity,
    gap_px: f32,
) -> TabScaffold {
    let tab = commands
        .spawn((
            tab_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(space),
        ))
        .id();

    let gap = pane_split_gaps(PaneSplitDirection::Row, gap_px);
    let split_root = commands
        .spawn((
            Pane,
            PaneSplit {
                direction: PaneSplitDirection::Row,
            },
            HostWindow(primary_window),
            Transform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: gap.column_gap,
                row_gap: gap.row_gap,
                ..default()
            },
            ChildOf(tab),
        ))
        .id();

    let pane = commands
        .spawn((
            leaf_pane_bundle(),
            LastActivatedAt::now(),
            ChildOf(split_root),
        ))
        .id();

    let stack = commands
        .spawn((
            stack_bundle(),
            LastActivatedAt::now(),
            CreatedAt::now(),
            ChildOf(pane),
        ))
        .id();

    TabScaffold { tab, pane, stack }
}

fn spawn_requested_tab_layouts(
    mut reader: MessageReader<TabLayoutSpawnRequest>,
    settings: Res<LayoutSettings>,
    mut page_open_requests: MessageWriter<PageOpenRequest>,
    spaces: Query<&EffectiveStartupUrl, With<crate::space::Space>>,
    mut commands: Commands,
) {
    for request in reader.read() {
        let Ok(startup_url) = spaces.get(request.space) else {
            continue;
        };
        let startup_dir = request
            .startup_dir
            .as_ref()
            .and_then(|startup_dir| startup_dir.canonicalize().ok())
            .filter(|startup_dir| startup_dir.is_dir())
            .and_then(|startup_dir| startup_dir.to_str().map(str::to_string));
        let TabScaffold {
            tab: tab_e,
            pane: leaf,
            stack,
        } = spawn_tab_scaffold_in_space(
            &mut commands,
            request.space,
            request.primary_window,
            settings.pane.gap,
        );
        commands.entity(tab_e).insert(Tab {
            name: request.name.clone().unwrap_or_default(),
            startup_dir,
        });
        if !request.focus {
            commands.entity(tab_e).insert(LastActivatedAt(0));
            commands.entity(leaf).insert(LastActivatedAt(0));
            commands.entity(stack).insert(LastActivatedAt(0));
        }
        match &request.content {
            TabLayoutSpawnContent::StartupUrlOrPrompt => {
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url: EffectiveStartupUrl::resolve(Some(startup_url)),
                    request_id: None,
                });
            }
            TabLayoutSpawnContent::Url {
                url,
                pending_prompt,
            } => {
                if let Some(prompt) = pending_prompt {
                    commands.entity(stack).insert(PendingPrompt(prompt.clone()));
                }
                page_open_requests.write(PageOpenRequest {
                    target: PageOpenTarget::Stack(stack),
                    url: url.clone(),
                    request_id: None,
                });
            }
        }
    }
}

fn sync_to_settings(
    settings: Res<LayoutSettings>,
    hidden_windows: Query<(), With<crate::toggle::LayoutHidden>>,
    mut window_q: Query<
        (&HostWindow, &mut Node),
        (With<VmuxWindow>, Without<SideSheet>, Without<MainColumn>),
    >,
    mut main_column_q: Query<
        &mut Node,
        (With<MainColumn>, Without<VmuxWindow>, Without<SideSheet>),
    >,
    mut sheet_q: Query<
        (&SideSheetPosition, &mut Node),
        (With<SideSheet>, Without<VmuxWindow>, Without<MainColumn>),
    >,
) {
    if !settings.is_changed() {
        return;
    }

    let pad_top = settings.window.pad_top();
    let pad_right = settings.window.pad_right();
    let pad_bottom = settings.window.pad_bottom();
    let pad_left = settings.window.pad_left();
    let gap = crate::event::PANE_GAP_PX;
    let configured_width =
        crate::event::SideSheetResizeEvent::live(settings.side_sheet.width).clamped();
    for (host, mut node) in &mut window_q {
        let full_padding = hidden_windows.contains(host.0);
        node.padding = UiRect {
            top: Val::Px(if full_padding { pad_top } else { 0.0 }),
            left: Val::Px(if full_padding { pad_left } else { 0.0 }),
            right: Val::Px(pad_right),
            bottom: Val::Px(pad_bottom),
        };
        node.column_gap = Val::Px(gap);
    }

    for _ in &mut main_column_q {}

    for (pos, mut node) in &mut sheet_q {
        match pos {
            SideSheetPosition::Left => {
                if !matches!(node.width, Val::Px(width) if width > 0.0) {
                    node.width = Val::Px(configured_width);
                }
            }
            SideSheetPosition::Right => {
                node.right = Val::Px(pad_right);
                node.top = Val::Px(pad_top);
                node.bottom = Val::Px(pad_bottom);
            }
            SideSheetPosition::Bottom => {
                node.left = Val::Px(pad_left);
                node.right = Val::Px(pad_right);
                node.bottom = Val::Px(pad_bottom);
            }
        }
    }
}

fn sync_main_column_gap(
    focus: crate::stack::FocusedStack,
    layout_focus: crate::stack::LayoutFocus,
    mut main_column_q: Query<&mut Node, With<MainColumn>>,
) {
    let pane_count = focus
        .tab
        .map(|tab| layout_focus.leaves(tab).len())
        .unwrap_or(0);
    let target = if pane_count > 1 {
        crate::event::PANE_GAP_PX
    } else {
        0.0
    };
    for mut node in &mut main_column_q {
        let current = match node.row_gap {
            Val::Px(v) => v,
            _ => f32::NAN,
        };
        if (current - target).abs() > 0.01 {
            node.row_gap = Val::Px(target);
        }
    }
}

fn fit_to_screen(
    windows: Query<&bevy::window::Window>,
    mut last_sizes: Local<std::collections::HashMap<Entity, Vec2>>,
    mut roots: Query<(&HostWindow, &mut Transform), With<VmuxWindow>>,
) {
    for (host, mut transform) in &mut roots {
        let Ok(window) = windows.get(host.0) else {
            continue;
        };
        let m = window.meters();
        if last_sizes
            .get(&host.0)
            .is_some_and(|last| (m.x - last.x).abs() < 0.001 && (m.y - last.y).abs() < 0.001)
        {
            continue;
        }
        last_sizes.insert(host.0, m);
        transform.translation = Vec3::new(0.0, m.y * 0.5, 0.0);
        transform.scale = Vec3::new(m.x, m.y, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cef::LayoutCef;
    use bevy::ecs::relationship::Relationship;
    use bevy::window::Monitor;

    #[test]
    fn scaffold_builds_tab_pane_stack_under_space() {
        use bevy::ecs::system::SystemState;
        let mut app = App::new();
        let space = app.world_mut().spawn(crate::space::Space).id();
        let window = app.world_mut().spawn_empty().id();
        let result = {
            let world = app.world_mut();
            let mut state = SystemState::<Commands>::new(world);
            let mut commands = state.get_mut(world).unwrap();
            let r = spawn_tab_scaffold_in_space(&mut commands, space, window, 8.0);
            state.apply(world);
            r
        };
        assert!(app.world().get::<crate::tab::Tab>(result.tab).is_some());
        assert!(app.world().get::<crate::pane::Pane>(result.pane).is_some());
        assert!(
            app.world()
                .get::<crate::stack::Stack>(result.stack)
                .is_some()
        );
        assert_eq!(app.world().get::<ChildOf>(result.tab).unwrap().get(), space);
    }

    static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct HomeEnvGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        old_home: Option<std::ffi::OsString>,
    }

    impl HomeEnvGuard {
        fn use_temp_home(name: &str) -> Self {
            let guard = HOME_ENV_LOCK.lock().expect("home env lock");
            let old_home = std::env::var_os("HOME");
            let home =
                std::env::temp_dir().join(format!("vmux-test-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).expect("create temp home");
            unsafe {
                std::env::set_var("HOME", &home);
            }
            Self {
                _guard: guard,
                old_home,
            }
        }
    }

    impl Drop for HomeEnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(home) = &self.old_home {
                    std::env::set_var("HOME", home);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    fn test_settings(gap: f32) -> LayoutSettings {
        LayoutSettings {
            radius: 0.0,
            window: crate::settings::WindowSettings { padding: 0.0 },
            pane: crate::settings::PaneSettings { gap },
            side_sheet: crate::settings::SideSheetSettings::default(),
            focus_ring: crate::settings::FocusRingSettings::default(),
        }
    }

    fn setup_window_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings(8.0))
            .init_resource::<Assets<WindowMaterial>>()
            .add_message::<TabLayoutSpawnRequest>();
        app.world_mut().spawn((
            Window {
                resolution: (1200, 800).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.add_systems(Startup, setup_shells);
        app
    }

    #[test]
    fn header_lives_in_main_column_above_main() {
        let mut app = setup_window_app();
        app.update();

        let header = app
            .world_mut()
            .query_filtered::<Entity, With<Header>>()
            .single(app.world())
            .expect("header");
        let main_col = app
            .world_mut()
            .query_filtered::<Entity, With<MainColumn>>()
            .single(app.world())
            .expect("main column");
        let parent = app
            .world()
            .get::<ChildOf>(header)
            .map(Relationship::get)
            .expect("header parent");

        assert_eq!(parent, main_col);
    }

    #[test]
    fn setup_spawns_one_window_surface() {
        let mut app = setup_window_app();
        app.update();

        let count = app
            .world_mut()
            .query_filtered::<Entity, With<WindowSurface>>()
            .iter(app.world())
            .count();

        assert_eq!(count, 1);
    }

    #[test]
    fn setup_spawns_one_layout_root_per_native_window() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings(8.0))
            .init_resource::<Assets<WindowMaterial>>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_systems(Update, setup_shells);
        let first = app.world_mut().spawn(Window::default()).id();
        let second = app.world_mut().spawn(Window::default()).id();

        app.update();

        let hosts = app
            .world_mut()
            .query_filtered::<(&HostWindow, &FlexViewport), With<VmuxWindow>>()
            .iter(app.world())
            .map(|(host, viewport)| (host.0, viewport.0))
            .collect::<std::collections::HashSet<_>>();
        let expected: std::collections::HashSet<_> =
            [(first, first), (second, second)].into_iter().collect();
        assert_eq!(hosts, expected);
    }

    #[test]
    fn a_new_window_gets_its_own_space_and_tab_request() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(test_settings(8.0))
            .init_resource::<Assets<WindowMaterial>>()
            .add_message::<TabLayoutSpawnRequest>()
            .add_systems(
                Update,
                (setup_shells, bevy::ecs::schedule::ApplyDeferred).chain(),
            );
        let window = app
            .world_mut()
            .spawn((Window::default(), NewWindowWorkspace))
            .id();

        app.update();

        let space = app
            .world_mut()
            .query_filtered::<Entity, With<crate::space::Space>>()
            .single(app.world())
            .expect("space");
        let request = app
            .world_mut()
            .resource_mut::<Messages<TabLayoutSpawnRequest>>()
            .drain()
            .next()
            .expect("tab request");
        assert_eq!(request.space, space);
        assert_eq!(request.primary_window, window);
    }

    #[test]
    fn setup_window_gap_matches_header_layout_gap() {
        let mut app = setup_window_app();
        app.update();

        let root = app
            .world_mut()
            .query_filtered::<Entity, With<VmuxWindow>>()
            .single(app.world())
            .expect("window root");
        let node = app.world().get::<Node>(root).expect("window node");

        assert_eq!(node.column_gap, Val::Px(crate::event::PANE_GAP_PX));
    }

    #[test]
    fn layout_shell_never_asks_cef_for_a_browser() {
        let mut app = setup_window_app();
        app.update();

        let layout_shell = app
            .world_mut()
            .query_filtered::<Entity, With<LayoutCef>>()
            .single(app.world())
            .expect("layout shell");

        assert!(app.world().get::<WebviewSource>(layout_shell).is_none());
        assert!(
            app.world()
                .get::<ResolvedWebviewUri>(layout_shell)
                .is_none()
        );
    }

    #[test]
    fn default_tab_opens_the_start_page() {
        let _home = HomeEnvGuard::use_temp_home("default-tab");
        let startup_dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutSpawnPlugin))
            .add_message::<crate::LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings { padding: 0.0 },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .add_systems(Update, request_default.before(TabLayoutSpawnSet));

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        app.world_mut()
            .entity_mut(space)
            .insert(crate::space::EffectiveStartupDir(Some(
                startup_dir.path().to_path_buf(),
            )));

        app.update();

        let opened = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(
            opened.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(),
            [vmux_core::EffectiveStartupUrl::START_PAGE],
            "a tab with nothing configured still opens a page rather than staging an empty stack"
        );
    }

    #[test]
    fn default_tab_stores_workspace_directory() {
        let _home = HomeEnvGuard::use_temp_home("default-tab-workspace");
        let startup_dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutSpawnPlugin))
            .add_message::<crate::LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings(0.0))
            .add_systems(Update, request_default.before(TabLayoutSpawnSet));

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        app.world_mut()
            .entity_mut(space)
            .insert(crate::space::EffectiveStartupDir(Some(
                startup_dir.path().to_path_buf(),
            )));

        app.update();

        let tab = app.world_mut().query::<&Tab>().single(app.world()).unwrap();
        assert_eq!(
            tab.startup_dir.as_deref(),
            startup_dir.path().canonicalize().unwrap().to_str()
        );
    }

    #[test]
    fn default_tab_without_configured_startup_dir_has_no_workspace() {
        let _home = HomeEnvGuard::use_temp_home("default-tab-no-workspace");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutSpawnPlugin))
            .add_message::<crate::LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings(0.0))
            .add_systems(Update, request_default.before(TabLayoutSpawnSet));

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        app.world_mut()
            .entity_mut(space)
            .insert(crate::space::EffectiveStartupDir(None));

        app.update();

        let tab = app.world_mut().query::<&Tab>().single(app.world()).unwrap();
        assert_eq!(tab.startup_dir, None);
    }

    #[test]
    fn tab_request_with_missing_startup_dir_spawns_without_workspace() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutSpawnPlugin))
            .add_message::<crate::LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings(0.0));
        let window = app.world_mut().spawn(PrimaryWindow).id();
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<TabLayoutSpawnRequest>>()
            .write(TabLayoutSpawnRequest {
                space,
                primary_window: window,
                name: None,
                startup_dir: Some(root.path().join("missing")),
                content: crate::TabLayoutSpawnContent::StartupUrlOrPrompt,
                clear_pending_stack: false,
                focus: true,
            });

        app.update();

        let tab = app.world_mut().query::<&Tab>().single(app.world()).unwrap();
        assert_eq!(tab.startup_dir, None);
    }

    #[test]
    fn tab_request_keeps_space_active_when_request_was_created() {
        let startup_dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutSpawnPlugin))
            .add_message::<crate::LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(test_settings(0.0));
        let window = app.world_mut().spawn(PrimaryWindow).id();
        let main = app.world_mut().spawn(Main).id();
        let requested_space = app
            .world_mut()
            .spawn((crate::space::Space, vmux_core::Active, ChildOf(main)))
            .id();
        let later_space = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        app.world_mut()
            .resource_mut::<Messages<TabLayoutSpawnRequest>>()
            .write(TabLayoutSpawnRequest {
                space: requested_space,
                primary_window: window,
                name: None,
                startup_dir: Some(startup_dir.path().to_path_buf()),
                content: crate::TabLayoutSpawnContent::StartupUrlOrPrompt,
                clear_pending_stack: false,
                focus: true,
            });
        app.world_mut()
            .entity_mut(requested_space)
            .remove::<vmux_core::Active>();
        app.world_mut()
            .entity_mut(later_space)
            .insert(vmux_core::Active);

        app.update();

        let tab = app
            .world_mut()
            .query_filtered::<Entity, With<Tab>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world()
                .get::<ChildOf>(tab)
                .map(|parent| parent.parent()),
            Some(requested_space)
        );
    }

    #[test]
    fn cold_start_seeds_exactly_one_default_tab() {
        let _home = HomeEnvGuard::use_temp_home("cold-start-one-tab");
        let startup_dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutSpawnPlugin))
            .add_message::<crate::LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings { padding: 0.0 },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .add_systems(Startup, request_default);

        app.world_mut().spawn(PrimaryWindow);
        let main = app.world_mut().spawn(Main).id();
        let space = app
            .world_mut()
            .spawn((crate::space::Space, ChildOf(main)))
            .id();
        app.world_mut().entity_mut(space).insert((
            crate::space::EffectiveStartupDir(Some(startup_dir.path().to_path_buf())),
            vmux_core::EffectiveStartupUrl("vmux://sessions/vibe/".to_string()),
        ));

        app.update();

        let mut tabs = app.world_mut().query_filtered::<Entity, With<Tab>>();
        assert_eq!(
            tabs.iter(app.world()).count(),
            1,
            "cold start must seed exactly one default tab; the Startup-written request must not be re-read by the Update consumer"
        );
    }

    #[test]
    fn default_tab_adopts_existing_space_when_none_active() {
        use bevy::ecs::relationship::Relationship;
        let _home = HomeEnvGuard::use_temp_home("default-tab-adopts-space");
        let startup_dir = tempfile::tempdir().unwrap();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, LayoutSpawnPlugin))
            .add_message::<crate::LauncherDismissRequest>()
            .add_message::<PageOpenRequest>()
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings { padding: 0.0 },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .add_systems(Startup, request_default);

        app.world_mut().spawn(Main);
        app.world_mut().spawn(PrimaryWindow);
        let space = app.world_mut().spawn(crate::space::Space).id();
        app.world_mut().entity_mut(space).insert((
            crate::space::EffectiveStartupDir(Some(startup_dir.path().to_path_buf())),
            vmux_core::EffectiveStartupUrl("vmux://sessions/vibe/".to_string()),
        ));

        app.update();

        let mut tabs = app.world_mut().query_filtered::<&ChildOf, With<Tab>>();
        let child_of = tabs
            .iter(app.world())
            .next()
            .expect("a default tab should be spawned");
        assert_eq!(
            child_of.get(),
            space,
            "default tab must be parented under the existing space, not Main"
        );
    }

    #[test]
    fn visible_fills_monitor_window_sync_clears_top_left_padding() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutSettings {
                radius: 0.0,
                window: crate::settings::WindowSettings { padding: 16.0 },
                pane: crate::settings::PaneSettings { gap: 0.0 },
                side_sheet: crate::settings::SideSheetSettings::default(),
                focus_ring: crate::settings::FocusRingSettings::default(),
            })
            .add_systems(Update, sync_to_settings);
        let window = app
            .world_mut()
            .spawn((
                Window {
                    resolution: (1200, 800).into(),
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        app.world_mut().spawn(Monitor {
            name: None,
            physical_width: 1200,
            physical_height: 800,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: None,
            scale_factor: 1.0,
            video_modes: Vec::new(),
        });
        let root = app
            .world_mut()
            .spawn((VmuxWindow, HostWindow(window), Node::default()))
            .id();

        app.update();

        let node = app.world().get::<Node>(root).expect("window node");
        assert_eq!(node.padding.top, Val::Px(0.0));
        assert_eq!(node.padding.left, Val::Px(0.0));
        assert_eq!(node.padding.right, Val::Px(16.0));
        assert_eq!(node.padding.bottom, Val::Px(16.0));
    }

    #[test]
    fn window_geometry_round_trips_position_size_fullscreen() {
        let g = WindowGeometry {
            fullscreen: true,
            position: Some(IVec2::new(100, 200)),
            size: Some(Vec2::new(1280.0, 800.0)),
        };
        assert_eq!(g, g);
        assert_eq!(g.position, Some(IVec2::new(100, 200)));
        assert_eq!(g.size, Some(Vec2::new(1280.0, 800.0)));
        assert!(g.fullscreen);
        assert_eq!(
            WindowGeometry::default(),
            WindowGeometry {
                fullscreen: false,
                position: None,
                size: None,
            }
        );
    }
}
