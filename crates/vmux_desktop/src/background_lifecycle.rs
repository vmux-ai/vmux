use bevy::prelude::*;
use bevy::window::Window;

#[derive(Message, Debug, Clone, Copy)]
pub enum LifecycleEvent {
    HideAllWindows,
    #[allow(dead_code)]
    ShowAllWindows,
    #[allow(dead_code)]
    QuitVmux,
}

pub struct BackgroundLifecyclePlugin;

impl Plugin for BackgroundLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LifecycleEvent>();
        app.add_systems(Update, handle_lifecycle_events);
    }
}

fn handle_lifecycle_events(
    mut events: MessageReader<LifecycleEvent>,
    mut windows: Query<&mut Window>,
    mut exit: MessageWriter<AppExit>,
) {
    for event in events.read() {
        match event {
            LifecycleEvent::HideAllWindows => {
                for mut window in &mut windows {
                    window.visible = false;
                }
            }
            LifecycleEvent::ShowAllWindows => {
                for mut window in &mut windows {
                    window.visible = true;
                }
            }
            LifecycleEvent::QuitVmux => {
                exit.write(AppExit::Success);
            }
        }
    }
}
