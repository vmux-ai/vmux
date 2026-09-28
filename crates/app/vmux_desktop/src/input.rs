use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use vmux_core::input::{ConsumesNativeKey, NativeKey, NativeKeyClaimSet, NativeKeyInput};

pub(crate) struct DesktopInputPlugin;

impl Plugin for DesktopInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((vmux_input::KeyboardPlugin, DesktopKeyBindingPlugin));
    }
}

struct DesktopKeyBindingPlugin;

impl Plugin for DesktopKeyBindingPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<crate::runtime::HideAllWindowsRequest>()
            .add_message::<NativeKeyInput>()
            .add_systems(Startup, spawn_application_key_bindings)
            .add_systems(
                Update,
                sync_application_key_bindings
                    .in_set(NativeKeyClaimSet)
                    .after(vmux_input::KeyboardContextSet)
                    .after(crate::window::SyncWindowFullscreen),
            )
            .add_systems(
                Update,
                handle_application_key_input.after(vmux_core::input::NativeKeyInputSet),
            );
    }
}

#[derive(Component)]
struct ExitFullscreenKey;

#[derive(Component)]
struct HideWindowsKey;

fn spawn_application_key_bindings(mut commands: Commands) {
    commands.spawn((
        Name::new("Hide windows key"),
        HideWindowsKey,
        NativeKey {
            key: KeyCode::KeyQ,
            modifiers: vmux_core::KeyModifiers {
                super_key: true,
                ..default()
            },
        },
        ConsumesNativeKey,
        vmux_core::Active,
    ));
    for shift in [false, true] {
        commands.spawn((
            Name::new("Exit fullscreen key"),
            ExitFullscreenKey,
            NativeKey {
                key: KeyCode::Escape,
                modifiers: vmux_core::KeyModifiers { shift, ..default() },
            },
            ConsumesNativeKey,
        ));
    }
}

fn sync_application_key_bindings(
    context: Query<&vmux_input::KeyboardContext>,
    focused_window: vmux_layout::window::FocusedWindow,
    fullscreen: Query<&crate::window::WindowFullscreen>,
    bindings: Query<(Entity, Has<vmux_core::Active>), With<ExitFullscreenKey>>,
    mut commands: Commands,
) {
    let page_owns_escape = context
        .single()
        .ok()
        .is_some_and(|context| context.page_owns_escape);
    let enabled = focused_window
        .entity()
        .and_then(|window| fullscreen.get(window).ok())
        .is_some_and(|fullscreen| fullscreen.0)
        && !page_owns_escape;
    for (entity, active) in &bindings {
        if active == enabled {
            continue;
        }
        if enabled {
            commands.entity(entity).insert(vmux_core::Active);
        } else {
            commands.entity(entity).remove::<vmux_core::Active>();
        }
    }
}

fn handle_application_key_input(
    mut inputs: MessageReader<NativeKeyInput>,
    bindings: Query<(Has<ExitFullscreenKey>, Has<HideWindowsKey>), With<vmux_core::Active>>,
    mut fullscreen: MessageWriter<crate::window::ExitFullscreenRequest>,
    mut hide_windows: MessageWriter<crate::runtime::HideAllWindowsRequest>,
) {
    for input in inputs.read() {
        let Some(claim) = input.claim else { continue };
        let Ok((exits_fullscreen, hides_windows)) = bindings.get(claim) else {
            continue;
        };
        if exits_fullscreen {
            fullscreen.write(crate::window::ExitFullscreenRequest);
        }
        if hides_windows {
            hide_windows.write(crate::runtime::HideAllWindowsRequest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    #[test]
    fn fullscreen_escape_becomes_an_ecs_request_unless_the_page_owns_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<crate::window::ExitFullscreenRequest>()
            .add_plugins(DesktopKeyBindingPlugin);
        let window = app
            .world_mut()
            .spawn((Window::default(), crate::window::WindowFullscreen(true)))
            .id();
        app.world_mut().entity_mut(window).insert(vmux_core::Active);
        app.update();
        let mut claims = app.world_mut().query_filtered::<
            (Entity, &NativeKey),
            (With<ExitFullscreenKey>, With<vmux_core::Active>),
        >();
        let claim = claims
            .iter(app.world())
            .find_map(|(entity, key)| (!key.modifiers.shift).then_some(entity))
            .unwrap();
        app.world_mut()
            .resource_mut::<Messages<NativeKeyInput>>()
            .write(NativeKeyInput {
                key: Some(KeyCode::Escape),
                native_code: 0,
                text: String::new(),
                modifiers: Default::default(),
                repeat: false,
                captured: false,
                claim: Some(claim),
                pressed_at_ms: 0,
            });
        app.update();
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<crate::window::ExitFullscreenRequest>>()
                .drain()
                .count(),
            1
        );

        app.world_mut().spawn(vmux_input::KeyboardContext {
            page_owns_escape: true,
            text_entry_owns_keys: false,
        });
        app.update();

        assert!(
            app.world_mut()
                .query_filtered::<Entity, (With<ExitFullscreenKey>, With<vmux_core::Active>)>()
                .iter(app.world())
                .next()
                .is_none()
        );
    }
}
