//! Tmux-style chord: **Ctrl+B** (prefix), then **%** / **"** / **o** / **x** within [`PREFIX_TIMEOUT_SECS`](vmux_input::PREFIX_TIMEOUT_SECS).

use bevy::prelude::*;
use bevy_cef::prelude::*;

use vmux_core::SessionSavePath;
use vmux_input::{AppInputRoot, PREFIX_TIMEOUT_SECS, VmuxPrefixState};
use vmux_layout::{Active, LayoutAxis, LayoutTree, Pane, PaneLastUrl, Root, SessionLayoutSnapshot};

use crate::layout::{try_cycle_pane_focus, try_kill_active_pane, try_split_active_pane};

#[allow(clippy::too_many_arguments)]
pub(crate) fn tmux_prefix_commands(
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
    path: Option<Res<SessionSavePath>>,
) {
    let Ok(mut prefix) = prefix_q.single_mut() else {
        return;
    };

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
        );
        return;
    }

    if keys.just_pressed(KeyCode::KeyO) {
        prefix.awaiting = false;
        let Ok(tree) = layout_q.single() else {
            return;
        };
        let Ok(cur) = active.single() else {
            return;
        };
        try_cycle_pane_focus(&mut commands, tree, cur);
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
            &mut snapshot,
            &pane_last,
            &webview_src,
            path.as_ref(),
        );
    }
}
