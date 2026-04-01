use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use leafwing_input_manager::prelude::*;
use vmux_core::{SessionSavePath, SessionSaveQueue, VmuxWorldCamera};
use vmux_layout::{
    Active, History, LayoutAxis, Layout, LoadingBarMaterial, Pane, Webview,
    PaneChromeLoadingBar, PaneChromeOwner, PaneChromeStrip, PaneFocusIncoming, PaneLastUrl,
    PaneSwapDir, SessionLayoutSnapshot, layout_viewport_for_workspace,
    layout_workspace_pane_rects, try_cycle_pane_focus, try_kill_active_pane,
    try_mirror_pane_layout, try_rotate_window, try_select_pane_direction, try_split_active_pane,
    try_swap_active_pane, try_toggle_zoom_pane,
};
use vmux_settings::{BindingCommandId, VmuxBindingGeneration, VmuxAppSettings};

use crate::component::{AppInputRoot, KeyAction, VmuxPrefixState};
use crate::input_map::build_input_map;

/// Asset stores used when spawning panes from tmux chord handlers (keeps system param count low).
#[derive(SystemParam)]
pub struct PaneSpawnAssets<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<WebviewExtendStandardMaterial>>,
    pub loading_bar_materials: ResMut<'w, Assets<LoadingBarMaterial>>,
}

/// Bundles chord input resources so [`tmux_prefix_commands`] stays within Bevy’s system-parameter limit (16).
#[derive(SystemParam)]
pub struct TmuxChordInput<'w> {
    pub time: Res<'w, Time>,
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    pub pane_focus_incoming: Res<'w, PaneFocusIncoming>,
}

pub(crate) fn spawn_app_input(mut commands: Commands, settings: Res<VmuxAppSettings>) {
    let input_map = build_input_map(&settings.input);
    commands.spawn((
        AppInputRoot,
        VmuxPrefixState::default(),
        input_map,
        ActionState::<KeyAction>::default(),
    ));
}

pub fn sync_input_map_from_settings(
    binding_gen: Res<VmuxBindingGeneration>,
    mut last: Local<u64>,
    settings: Res<VmuxAppSettings>,
    mut q: Query<&mut InputMap<KeyAction>, With<AppInputRoot>>,
) {
    if *last == binding_gen.0 {
        return;
    }
    *last = binding_gen.0;
    let Ok(mut input_map) = q.single_mut() else {
        return;
    };
    *input_map = build_input_map(&settings.input);
}

pub(crate) fn exit_on_quit_action(
    query: Query<&ActionState<KeyAction>, With<AppInputRoot>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Ok(state) = query.single() else {
        return;
    };
    if state.just_pressed(&KeyAction::Quit) {
        app_exit.write(AppExit::Success);
    }
}

/// Tmux-style **Ctrl+B** chord handling (splits / focus / zoom (`resize-pane -Z`) / mirror / **select-pane** (arrows) / **swap-pane** (Ctrl+arrows) / rotate-window (`{` `}` or `r` `R`) / kill-pane).
#[allow(clippy::too_many_arguments)]
pub fn tmux_prefix_commands(
    input: TmuxChordInput,
    mut prefix_q: Query<&mut VmuxPrefixState, With<AppInputRoot>>,
    mut commands: Commands,
    mut layout_q: Query<&mut Layout, With<vmux_layout::Window>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut spawn_assets: PaneSpawnAssets,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    (pane_last, webview_src, history_panes): (
        Query<&PaneLastUrl>,
        Query<&WebviewSource>,
        Query<Entity, (With<Pane>, With<Webview>, With<History>)>,
    ),
    chrome_or_border_q: Query<
        (Entity, &PaneChromeOwner),
        Or<(With<PaneChromeStrip>, With<PaneChromeLoadingBar>)>,
    >,
    path: Option<Res<SessionSavePath>>,
    mut session_queue: ResMut<SessionSaveQueue>,
    settings: Res<VmuxAppSettings>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<&Camera, With<VmuxWorldCamera>>,
    panes: Query<Entity, With<Pane>>,
) {
    let Ok(mut prefix) = prefix_q.single_mut() else {
        return;
    };
    let default_url = settings.browser.default_webview_url.as_str();

    let keys = &input.keys;
    let time = &input.time;
    let bindings = &settings.input;
    let timeout = bindings.prefix.timeout_secs;

    if prefix.awaiting && time.elapsed_secs() > prefix.deadline_secs {
        prefix.awaiting = false;
    }

    if !prefix.awaiting {
        if bindings.prefix_lead_just_pressed(keys) {
            prefix.awaiting = true;
            prefix.deadline_secs = time.elapsed_secs() + timeout;
        }
        return;
    }

    if bindings.prefix_lead_just_pressed(keys) {
        prefix.awaiting = false;
        return;
    }

    let Some(cmd) = bindings.prefix_second_command(keys) else {
        return;
    };
    prefix.awaiting = false;

    let Ok(window) = window.single() else {
        return;
    };
    let Ok(camera) = camera.single() else {
        return;
    };
    let Some((vw, vh)) = layout_viewport_for_workspace(window, camera) else {
        return;
    };
    let Ok(mut tree) = layout_q.single_mut() else {
        return;
    };
    let Ok(active_ent) = active.single() else {
        return;
    };
    let rects = layout_workspace_pane_rects(vw, vh, &tree, &settings, |e| panes.get(e).is_ok());

    match cmd {
        BindingCommandId::SplitHorizontal => {
            try_split_active_pane(
                &mut commands,
                &mut tree,
                active_ent,
                LayoutAxis::Horizontal,
                &mut spawn_assets.meshes,
                &mut spawn_assets.materials,
                &mut spawn_assets.loading_bar_materials,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::SplitVertical => {
            try_split_active_pane(
                &mut commands,
                &mut tree,
                active_ent,
                LayoutAxis::Vertical,
                &mut spawn_assets.meshes,
                &mut spawn_assets.materials,
                &mut spawn_assets.loading_bar_materials,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::CycleNextPane => {
            try_cycle_pane_focus(&mut commands, &mut tree, active_ent);
        }
        BindingCommandId::SelectPaneLeft => {
            let prefer = input.pane_focus_incoming.0.get(&active_ent).copied();
            try_select_pane_direction(
                &mut commands,
                &mut tree,
                active_ent,
                PaneSwapDir::Left,
                &rects,
                prefer,
            );
        }
        BindingCommandId::SelectPaneRight => {
            let prefer = input.pane_focus_incoming.0.get(&active_ent).copied();
            try_select_pane_direction(
                &mut commands,
                &mut tree,
                active_ent,
                PaneSwapDir::Right,
                &rects,
                prefer,
            );
        }
        BindingCommandId::SelectPaneUp => {
            let prefer = input.pane_focus_incoming.0.get(&active_ent).copied();
            try_select_pane_direction(
                &mut commands,
                &mut tree,
                active_ent,
                PaneSwapDir::Up,
                &rects,
                prefer,
            );
        }
        BindingCommandId::SelectPaneDown => {
            let prefer = input.pane_focus_incoming.0.get(&active_ent).copied();
            try_select_pane_direction(
                &mut commands,
                &mut tree,
                active_ent,
                PaneSwapDir::Down,
                &rects,
                prefer,
            );
        }
        BindingCommandId::SwapPaneLeft => {
            try_swap_active_pane(
                &mut tree,
                active_ent,
                PaneSwapDir::Left,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::SwapPaneRight => {
            try_swap_active_pane(
                &mut tree,
                active_ent,
                PaneSwapDir::Right,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::SwapPaneUp => {
            try_swap_active_pane(
                &mut tree,
                active_ent,
                PaneSwapDir::Up,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::SwapPaneDown => {
            try_swap_active_pane(
                &mut tree,
                active_ent,
                PaneSwapDir::Down,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::ToggleZoom => {
            try_toggle_zoom_pane(&mut tree, active_ent);
        }
        BindingCommandId::MirrorLayout => {
            try_mirror_pane_layout(
                &mut tree,
                active_ent,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::RotateBackward => {
            try_rotate_window(
                &mut commands,
                &mut tree,
                active_ent,
                true,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::RotateForward => {
            try_rotate_window(
                &mut commands,
                &mut tree,
                active_ent,
                false,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::ClosePane => {
            try_kill_active_pane(
                &mut commands,
                &mut tree,
                active_ent,
                &mut spawn_assets.meshes,
                &mut spawn_assets.materials,
                &mut spawn_assets.loading_bar_materials,
                &mut snapshot,
                &pane_last,
                &webview_src,
                &history_panes,
                &chrome_or_border_q,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        }
        BindingCommandId::Quit
        | BindingCommandId::ToggleCommandPalette
        | BindingCommandId::FocusCommandPaletteUrl
        | BindingCommandId::OpenHistory
        | BindingCommandId::OpenHistoryInNewTab => {}
    }
}

/// Direct pane focus movement with Ctrl+Arrow (without tmux prefix).
#[allow(clippy::too_many_arguments)]
pub fn ctrl_arrow_focus_commands(
    keys: Res<ButtonInput<KeyCode>>,
    prefix_q: Query<&VmuxPrefixState, With<AppInputRoot>>,
    mut commands: Commands,
    mut layout_q: Query<&mut Layout, With<vmux_layout::Window>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    settings: Res<VmuxAppSettings>,
    pane_focus_incoming: Res<PaneFocusIncoming>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<&Camera, With<VmuxWorldCamera>>,
    panes: Query<Entity, With<Pane>>,
) {
    let Ok(prefix) = prefix_q.single() else {
        return;
    };
    if prefix.awaiting {
        return;
    }
    if !settings.input.ctrl_arrow_focus {
        return;
    }

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !ctrl {
        return;
    }

    let arrow_dir = if keys.just_pressed(KeyCode::ArrowLeft) {
        Some(PaneSwapDir::Left)
    } else if keys.just_pressed(KeyCode::ArrowRight) {
        Some(PaneSwapDir::Right)
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        Some(PaneSwapDir::Up)
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        Some(PaneSwapDir::Down)
    } else {
        None
    };
    let Some(dir) = arrow_dir else {
        return;
    };

    let Ok(window) = window.single() else {
        return;
    };
    let Ok(camera) = camera.single() else {
        return;
    };
    let Some((vw, vh)) = layout_viewport_for_workspace(window, camera) else {
        return;
    };
    let Ok(mut tree) = layout_q.single_mut() else {
        return;
    };
    let Ok(active_ent) = active.single() else {
        return;
    };
    let rects = layout_workspace_pane_rects(vw, vh, &tree, &settings, |e| panes.get(e).is_ok());
    let prefer = pane_focus_incoming.0.get(&active_ent).copied();
    try_select_pane_direction(&mut commands, &mut tree, active_ent, dir, &rects, prefer);
}
