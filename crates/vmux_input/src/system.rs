use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::*;
use leafwing_input_manager::prelude::*;
use vmux_core::{SessionSavePath, SessionSaveQueue};
use vmux_layout::{
    Active, LayoutAxis, LayoutTree, LoadingBarMaterial, Pane, PaneChromeLoadingBar, PaneChromeOwner,
    PaneChromeStrip, PaneLastUrl,
    PaneSwapDir, Root, SessionLayoutSnapshot, VmuxWorldCamera, layout_viewport_for_workspace,
    layout_workspace_pane_rects, try_cycle_pane_focus, try_kill_active_pane,
    try_mirror_pane_layout, try_rotate_window, try_select_pane_direction, try_split_active_pane,
    try_swap_active_pane, try_toggle_zoom_pane,
};
use vmux_settings::VmuxAppSettings;

use crate::component::{AppAction, AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixState};

/// Asset stores used when spawning panes from tmux chord handlers (keeps system param count low).
#[derive(SystemParam)]
pub struct PaneSpawnAssets<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<WebviewExtendStandardMaterial>>,
    pub loading_bar_materials: ResMut<'w, Assets<LoadingBarMaterial>>,
}

/// Bundles `Res<Time>` + `Res<ButtonInput>` so [`tmux_prefix_commands`] stays within Bevy’s system-parameter limit (16).
#[derive(SystemParam)]
pub struct TmuxChordInput<'w> {
    pub time: Res<'w, Time>,
    pub keys: Res<'w, ButtonInput<KeyCode>>,
}

pub(crate) fn spawn_app_input(mut commands: Commands) {
    let mut input_map = InputMap::<AppAction>::default();
    input_map.insert(
        AppAction::Quit,
        ButtonlikeChord::modified(ModifierKey::Super, KeyCode::KeyQ),
    );
    input_map.insert(
        AppAction::Quit,
        ButtonlikeChord::modified(ModifierKey::Control, KeyCode::KeyQ),
    );
    commands.spawn((
        AppInputRoot,
        VmuxPrefixState::default(),
        input_map,
        ActionState::<AppAction>::default(),
    ));
}

pub(crate) fn exit_on_quit_action(
    query: Query<&ActionState<AppAction>, With<AppInputRoot>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let Ok(state) = query.single() else {
        return;
    };
    if state.just_pressed(&AppAction::Quit) {
        app_exit.write(AppExit::Success);
    }
}

/// Tmux-style **Ctrl+B** chord handling (splits / focus / zoom (`resize-pane -Z`) / mirror / **select-pane** (arrows) / **swap-pane** (Ctrl+arrows) / rotate-window (`{` `}` or `r` `R`) / kill-pane).
#[allow(clippy::too_many_arguments)]
pub fn tmux_prefix_commands(
    input: TmuxChordInput,
    mut prefix_q: Query<&mut VmuxPrefixState, With<AppInputRoot>>,
    mut commands: Commands,
    mut layout_q: Query<&mut LayoutTree, With<Root>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut spawn_assets: PaneSpawnAssets,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
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
    let default_url = settings.default_webview_url.as_str();

    let keys = &input.keys;
    let time = &input.time;
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if prefix.awaiting && time.elapsed_secs() > prefix.deadline_secs {
        prefix.awaiting = false;
    }

    if !prefix.awaiting {
        if ctrl && keys.just_pressed(KeyCode::KeyB) {
            prefix.awaiting = true;
            prefix.deadline_secs = time.elapsed_secs() + PREFIX_TIMEOUT_SECS;
        }
        return;
    }

    if ctrl && keys.just_pressed(KeyCode::KeyB) {
        prefix.awaiting = false;
        return;
    }

    if shift && keys.just_pressed(KeyCode::Digit5) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
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
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
        return;
    }

    if shift && keys.just_pressed(KeyCode::Quote) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
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
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
        return;
    }

    if keys.just_pressed(KeyCode::KeyO) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(cur) = active.single() else {
            return;
        };
        try_cycle_pane_focus(&mut commands, &mut tree, cur);
        return;
    }

    // select-pane -L/-R/-U/-D: prefix + arrows (tmux default). swap-pane: prefix + Ctrl+arrows.
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
    if let Some(dir) = arrow_dir {
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
        let rects = layout_workspace_pane_rects(vw, vh, &tree, &settings, |e| {
            panes.get(e).is_ok()
        });
        if ctrl {
            try_swap_active_pane(
                &mut tree,
                active_ent,
                dir,
                &mut snapshot,
                &pane_last,
                &webview_src,
                path.as_ref(),
                &mut session_queue,
                default_url,
            );
        } else {
            try_select_pane_direction(&mut commands, &mut tree, active_ent, dir, &rects);
        }
        return;
    }

    if keys.just_pressed(KeyCode::KeyZ) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
        try_toggle_zoom_pane(&mut tree, active_ent);
        return;
    }

    // Mirror: swap halves of the innermost split containing the active pane (prefix + m).
    if keys.just_pressed(KeyCode::KeyM) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
        try_mirror_pane_layout(
            &mut tree,
            active_ent,
            &mut snapshot,
            &pane_last,
            &webview_src,
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
        return;
    }

    // rotate-window -D / -U (tmux default: prefix + `}` / `{`).
    if shift && keys.just_pressed(KeyCode::BracketRight) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
        try_rotate_window(
            &mut commands,
            &mut tree,
            active_ent,
            true,
            &mut snapshot,
            &pane_last,
            &webview_src,
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
        return;
    }
    if shift && keys.just_pressed(KeyCode::BracketLeft) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
        try_rotate_window(
            &mut commands,
            &mut tree,
            active_ent,
            false,
            &mut snapshot,
            &pane_last,
            &webview_src,
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
        return;
    }

    // rotate-window -D / -U (prefix + r / R, same as `}` / `{`).
    if keys.just_pressed(KeyCode::KeyR) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
        try_rotate_window(
            &mut commands,
            &mut tree,
            active_ent,
            !shift,
            &mut snapshot,
            &pane_last,
            &webview_src,
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
        return;
    }

    if keys.just_pressed(KeyCode::KeyX) {
        prefix.awaiting = false;
        let Ok(mut tree) = layout_q.single_mut() else {
            return;
        };
        let Ok(active_ent) = active.single() else {
            return;
        };
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
            &chrome_or_border_q,
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
    }
}
