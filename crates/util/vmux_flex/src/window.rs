use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::prelude::*;
use bevy_math::Vec2;
use bevy_window::{PrimaryWindow, Window};

use crate::{LayoutContext, LayoutSystems, PrimaryViewport};

pub struct WindowViewportPlugin;

impl Plugin for WindowViewportPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, sync.in_set(LayoutSystems::Viewport));
    }
}

#[derive(Component)]
struct Bound;

impl From<&Window> for LayoutContext {
    fn from(window: &Window) -> Self {
        Self {
            scale_factor: window.resolution.scale_factor(),
            physical_size: Vec2::new(
                window.resolution.physical_width() as f32,
                window.resolution.physical_height() as f32,
            ),
        }
    }
}

type Windows<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Window,
        Option<&'static LayoutContext>,
        Has<PrimaryWindow>,
        Has<PrimaryViewport>,
        Has<Bound>,
    ),
>;

fn sync(
    windows: Windows,
    bound: Query<(), With<Bound>>,
    mut removed: RemovedComponents<Window>,
    mut commands: Commands,
) {
    for entity in removed.read() {
        if !windows.contains(entity)
            && bound.contains(entity)
            && let Ok(mut entity) = commands.get_entity(entity)
        {
            entity.remove::<(LayoutContext, PrimaryViewport, Bound)>();
        }
    }
    for (entity, window, current, primary, marked, bound) in &windows {
        let context = LayoutContext::from(window);
        if current != Some(&context) {
            commands.entity(entity).insert(context);
        }
        if !bound {
            commands.entity(entity).insert(Bound);
        }
        if primary && !marked {
            commands.entity(entity).insert(PrimaryViewport);
        } else if !primary && marked {
            commands.entity(entity).remove::<PrimaryViewport>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ComputedNode, FlexPlugin, FlexViewport, Node, Val};

    #[test]
    fn window_geometry_and_primary_changes_apply_in_one_update() {
        let mut app = App::new();
        app.add_plugins((FlexPlugin, WindowViewportPlugin));
        let mut window = Window::default();
        window.resolution.set_physical_resolution(1200, 800);
        window.resolution.set_scale_factor_override(Some(2.0));
        let first = app.world_mut().spawn((window, PrimaryWindow)).id();
        let second = app
            .world_mut()
            .spawn(Window {
                resolution: (640, 480).into(),
                ..Default::default()
            })
            .id();
        let root = app
            .world_mut()
            .spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Default::default()
            })
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<ComputedNode>(root)
                .unwrap()
                .to_logical()
                .size,
            Vec2::new(600.0, 400.0)
        );
        app.world_mut().entity_mut(first).remove::<PrimaryWindow>();
        app.world_mut().entity_mut(second).insert(PrimaryWindow);
        app.update();
        assert_eq!(
            app.world().get::<ComputedNode>(root).unwrap().size,
            Vec2::new(640.0, 480.0)
        );
        app.world_mut().entity_mut(root).insert(FlexViewport(first));
        app.world_mut()
            .get_mut::<Window>(first)
            .unwrap()
            .resolution
            .set_physical_resolution(1600, 1000);
        app.update();
        assert_eq!(
            app.world()
                .get::<ComputedNode>(root)
                .unwrap()
                .to_logical()
                .size,
            Vec2::new(800.0, 500.0)
        );
        app.world_mut().entity_mut(first).remove::<Window>();
        app.update();
        assert!(app.world().get::<LayoutContext>(first).is_none());
        assert!(app.world().get::<PrimaryViewport>(first).is_none());
    }
}
