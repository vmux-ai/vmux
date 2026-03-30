use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_cef::prelude::*;
use leafwing_input_manager::prelude::*;
use vmux_core::{SessionSavePath, SessionSaveQueue};
use vmux_layout::{
    Active, LayoutAxis, LayoutTree, Pane, PaneChromeOwner, PaneChromeStrip, PaneLastUrl, Root,
    SessionLayoutSnapshot, try_cycle_pane_focus, try_kill_active_pane, try_mirror_pane_layout,
    try_rotate_window, try_split_active_pane, try_toggle_zoom_pane,
};
use vmux_settings::VmuxAppSettings;

use crate::component::{AppAction, AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixState};

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

/// Tmux-style **Ctrl+B** chord handling (splits / focus / zoom (`resize-pane -Z`) / mirror / rotate-window (`{` `}` or `r` `R`) / kill-pane).
#[allow(clippy::too_many_arguments)]
pub fn tmux_prefix_commands(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut prefix_q: Query<&mut VmuxPrefixState, With<AppInputRoot>>,
    mut commands: Commands,
    mut layout_q: Query<&mut LayoutTree, With<Root>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WebviewExtendStandardMaterial>>,
    mut snapshot: ResMut<SessionLayoutSnapshot>,
    pane_last: Query<&PaneLastUrl>,
    webview_src: Query<&WebviewSource>,
    chrome_q: Query<(Entity, &PaneChromeOwner), With<PaneChromeStrip>>,
    path: Option<Res<SessionSavePath>>,
    mut session_queue: ResMut<SessionSaveQueue>,
    settings: Res<VmuxAppSettings>,
) {
    let Ok(mut prefix) = prefix_q.single_mut() else {
        return;
    };
    let default_url = settings.default_webview_url.as_str();

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
            &mut meshes,
            &mut materials,
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
            &mut meshes,
            &mut materials,
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
            &mut meshes,
            &mut materials,
            &mut snapshot,
            &pane_last,
            &webview_src,
            &chrome_q,
            path.as_ref(),
            &mut session_queue,
            default_url,
        );
    }
}
