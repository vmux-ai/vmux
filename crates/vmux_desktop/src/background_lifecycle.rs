use bevy::ecs::message::Messages;
use bevy::prelude::*;
use bevy::window::Window;

use crate::terminal::{self};
use vmux_terminal::{PtyExited, Terminal};

#[derive(Message, Debug, Clone, Copy)]
pub enum LifecycleEvent {
    HideAllWindows,
    ShowAllWindows,
    QuitVmux,
}

pub struct BackgroundLifecyclePlugin;

impl Plugin for BackgroundLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LifecycleEvent>();
        app.add_systems(Update, handle_lifecycle_events);
    }
}

fn handle_lifecycle_events(world: &mut World) {
    let drained: Vec<LifecycleEvent> = {
        let mut events = world.resource_mut::<Messages<LifecycleEvent>>();
        events.drain().collect()
    };

    for event in drained {
        match event {
            LifecycleEvent::HideAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) {
                    w.visible = false;
                }
            }
            LifecycleEvent::ShowAllWindows => {
                let mut q = world.query::<&mut Window>();
                for mut w in q.iter_mut(world) {
                    w.visible = true;
                }
            }
            LifecycleEvent::QuitVmux => {
                let live = {
                    let mut q = world.query_filtered::<(), (With<Terminal>, Without<PtyExited>)>();
                    q.iter(world).count()
                };
                if live > 0 && !terminal::confirm_quit_dialog(live) {
                    continue;
                }
                world
                    .resource_mut::<Messages<AppExit>>()
                    .write(AppExit::Success);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn handle_lifecycle_events_uses_world_for_confirm_dialog() {
        let source = include_str!("background_lifecycle.rs");
        let exclusive_marker = ["world", ": ", "&mut", " World"].concat();
        assert!(
            source.contains(&exclusive_marker),
            "handle_lifecycle_events must be an exclusive &mut World system to call confirm_quit_dialog"
        );
        let confirm_call = ["confirm", "_quit_dialog"].concat();
        assert!(
            source.contains(&confirm_call),
            "QuitVmux arm must call terminal::confirm_quit_dialog"
        );
    }
}
