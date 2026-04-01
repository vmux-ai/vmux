//! Pointer hover → [`Active`] using **layout rects**, scheduled so meshes match CEF for the next frame.
//!
//! Previously this ran in [`PreUpdate`]: [`Active`] changed before [`Update`], while CEF still used
//! the **previous** frame’s expanded/biased meshes, so ray hits fought layout-based focus (“ragging”).
//!
//! This system runs in [`PostUpdate`] **after** [`camera_system`](bevy::render::camera::camera_system)
//! and **before** [`apply_pane_layout`](crate::pane_layout::apply_pane_layout), so the same frame
//! updates transforms for the new active pane before the next [`Update`].

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::CefKeyboardTarget;
use vmux_core::Active;
use vmux_core::VmuxCommandPaletteState;

use crate::{
    Layout, Pane, PixelRect, VmuxWorldCamera, layout_viewport_for_workspace,
    layout_workspace_pane_rects,
};
use vmux_settings::VmuxAppSettings;

/// Consecutive frames the cursor must stay in a pane’s layout rect before we switch [`Active`].
const HOVER_STABLE_FRAMES: u32 = 2;

fn pane_at_layout_cursor(cursor: Vec2, rects: &[(Entity, PixelRect)]) -> Option<Entity> {
    for &(entity, r) in rects {
        if r.w <= 0.0 || r.h <= 0.0 || !r.x.is_finite() || !r.y.is_finite() {
            continue;
        }
        let right = r.x + r.w;
        let bottom = r.y + r.h;
        if cursor.x >= r.x && cursor.x < right && cursor.y >= r.y && cursor.y < bottom {
            return Some(entity);
        }
    }
    None
}

#[derive(Default)]
pub(super) struct PanePointerFocusCache {
    rects: Vec<(Entity, PixelRect)>,
    layout_rev: u64,
    vw: f32,
    vh: f32,
    spacing: f32,
    pad: f32,
    pad_top: f32,
    /// Last raw hover target (before stability filter).
    stable_hover: Option<Entity>,
    stable_frames: u32,
    /// Last seen cursor position in window coordinates.
    last_cursor: Option<Vec2>,
    /// After a non-pointer focus change (keyboard/chord), ignore hover until the cursor moves.
    wait_for_cursor_motion: bool,
}

impl PanePointerFocusCache {
    fn rebuild_rects(
        &mut self,
        tree: &Layout,
        vw: f32,
        vh: f32,
        settings: &VmuxAppSettings,
        panes: &Query<Entity, With<Pane>>,
    ) {
        self.rects = layout_workspace_pane_rects(vw, vh, tree, settings, |e| panes.contains(e));
        self.layout_rev = tree.revision;
        self.vw = vw;
        self.vh = vh;
        self.spacing = settings.layout.pane_border_spacing_px;
        self.pad = settings.layout.window_padding_px;
        self.pad_top = settings.layout.window_padding_top_px;
    }

    fn needs_rect_rebuild(
        &self,
        tree: &Layout,
        vw: f32,
        vh: f32,
        settings: &VmuxAppSettings,
    ) -> bool {
        tree.revision != self.layout_rev
            || (vw - self.vw).abs() > 0.5
            || (vh - self.vh).abs() > 0.5
            || (settings.layout.pane_border_spacing_px - self.spacing).abs() > f32::EPSILON
            || (settings.layout.window_padding_px - self.pad).abs() > f32::EPSILON
            || (settings.layout.window_padding_top_px - self.pad_top).abs() > f32::EPSILON
    }
}

/// Set [`Active`] from layout hit-testing; run before [`apply_pane_layout`](crate::pane_layout::apply_pane_layout).
pub(super) fn update_active_pane_under_cursor(
    mut commands: Commands,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<&Camera, With<VmuxWorldCamera>>,
    layout_q: Query<&Layout, With<crate::Window>>,
    settings: Res<VmuxAppSettings>,
    panes: Query<Entity, With<Pane>>,
    active: Query<Entity, (With<Pane>, With<Active>)>,
    active_mutation: Query<Entity, (With<Pane>, Or<(Added<Active>, Changed<Active>)>)>,
    palette: Option<Res<VmuxCommandPaletteState>>,
    mut cache: Local<PanePointerFocusCache>,
) {
    if palette.is_some_and(|p| p.open) {
        return;
    }
    // Splits / keyboard focus change Active without moving the cursor; don’t fight that with stale hover stability.
    if !active_mutation.is_empty() {
        cache.stable_hover = None;
        cache.stable_frames = 0;
        cache.wait_for_cursor_motion = true;
    }
    let Ok(window) = window.single() else {
        return;
    };
    let Ok(camera) = camera.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        cache.stable_hover = None;
        cache.stable_frames = 0;
        cache.last_cursor = None;
        cache.wait_for_cursor_motion = false;
        return;
    };
    let moved = cache
        .last_cursor
        .map_or(true, |prev| (cursor - prev).length_squared() > 1.0);
    cache.last_cursor = Some(cursor);
    if cache.wait_for_cursor_motion {
        if moved {
            cache.wait_for_cursor_motion = false;
        } else {
            return;
        }
    }
    let Some((vw, vh)) = layout_viewport_for_workspace(window, camera) else {
        return;
    };
    let Ok(tree) = layout_q.single() else {
        return;
    };

    if cache.needs_rect_rebuild(tree, vw, vh, &settings) {
        cache.rebuild_rects(tree, vw, vh, &settings, &panes);
        cache.stable_hover = None;
        cache.stable_frames = 0;
    }

    let raw_hover = pane_at_layout_cursor(cursor, &cache.rects);
    let Some(raw) = raw_hover else {
        cache.stable_hover = None;
        cache.stable_frames = 0;
        return;
    };
    if !panes.contains(raw) {
        return;
    }

    if cache.stable_hover == Some(raw) {
        cache.stable_frames = cache.stable_frames.saturating_add(1);
    } else {
        cache.stable_hover = Some(raw);
        cache.stable_frames = 1;
    }
    if cache.stable_frames < HOVER_STABLE_FRAMES {
        return;
    }

    let target = raw;
    let Ok(current) = active.single() else {
        for e in active.iter() {
            commands.entity(e).remove::<Active>();
            commands.entity(e).remove::<CefKeyboardTarget>();
        }
        commands.entity(target).insert((Active, CefKeyboardTarget));
        return;
    };
    if current == target {
        return;
    }
    for e in active.iter() {
        commands.entity(e).remove::<Active>();
        commands.entity(e).remove::<CefKeyboardTarget>();
    }
    commands.entity(target).insert((Active, CefKeyboardTarget));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LayoutAxis, LayoutNode, Layout, VmuxWorldCamera, Workspace};
    use bevy::window::PrimaryWindow;

    fn active_pane(world: &mut World) -> Entity {
        world
            .query_filtered::<Entity, (With<Pane>, With<Active>)>()
            .iter(world)
            .next()
            .expect("expected one active pane")
    }

    #[test]
    fn keyboard_focus_is_not_immediately_overridden_by_stationary_hover() {
        let mut app = App::new();
        app.insert_resource(VmuxAppSettings::default());
        app.add_systems(PostUpdate, update_active_pane_under_cursor);

        app.world_mut()
            .spawn((bevy::window::Window::default(), PrimaryWindow));
        app.world_mut().spawn((Camera::default(), VmuxWorldCamera));

        let left = app.world_mut().spawn((Pane, Active)).id();
        let right = app.world_mut().spawn(Pane).id();
        app.world_mut().spawn(Workspace).with_children(|parent| {
            parent.spawn((
                crate::Window,
                Layout {
                    root: LayoutNode::Split {
                        axis: LayoutAxis::Horizontal,
                        ratio: 0.5,
                        left: Box::new(LayoutNode::Leaf(left)),
                        right: Box::new(LayoutNode::Leaf(right)),
                    },
                    revision: 1,
                    zoom_pane: None,
                },
            ));
        });

        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            let mut window = q
                .single_mut(world)
                .expect("expected exactly one primary window");
            window.set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        }

        // Build hover stability state over the left pane.
        app.update();
        app.update();
        assert_eq!(active_pane(app.world_mut()), left);

        // Simulate keyboard navigation moving focus to the right pane.
        app.world_mut().entity_mut(left).remove::<Active>();
        app.world_mut().entity_mut(right).insert(Active);
        assert_eq!(active_pane(app.world_mut()), right);

        // Pointer did not move; hover must not steal focus back.
        app.update();
        app.update();
        assert_eq!(active_pane(app.world_mut()), right);

        // After the pointer moves again, hover is allowed to retake focus.
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
            let mut window = q
                .single_mut(world)
                .expect("expected exactly one primary window");
            window.set_cursor_position(Some(Vec2::new(120.0, 100.0)));
        }
        app.update();
        app.update();
        assert_eq!(active_pane(app.world_mut()), left);
    }
}
