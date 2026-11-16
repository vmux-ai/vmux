use bevy::prelude::*;
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(super) struct WindowButtonsPlugin;

impl Plugin for WindowButtonsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WindowButtonsEffect>()
            .add_observer(retire_applied_visibility)
            .add_systems(PostUpdate, project_visibility);
        #[cfg(target_os = "macos")]
        app.add_systems(PostUpdate, apply_visibility.after(project_visibility));
    }
}

#[derive(Component)]
struct WindowButtonsApplied {
    visible: bool,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct WindowButtonsEffect {
    window: Entity,
    visible: bool,
}

fn project_visibility(
    headers: Query<(Entity, Has<crate::Open>), With<crate::Header>>,
    hierarchy: crate::window::WindowHierarchy,
    windows: Query<(Entity, Ref<Window>, Option<&WindowButtonsApplied>)>,
    mut effects: MessageWriter<WindowButtonsEffect>,
) {
    for (entity, window, applied) in &windows {
        let mut visible = false;
        for (header, open) in &headers {
            if open && hierarchy.get(header) == Some(entity) {
                visible = true;
                break;
            }
        }
        if !window.is_added() && applied.is_some_and(|applied| applied.visible == visible) {
            continue;
        }
        effects.write(WindowButtonsEffect {
            window: entity,
            visible,
        });
    }
}

fn retire_applied_visibility(removed: On<Remove, Window>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<WindowButtonsApplied>();
}

#[cfg(target_os = "macos")]
fn apply_visibility(
    mut effects: MessageReader<WindowButtonsEffect>,
    windows: Query<(), With<Window>>,
    mut commands: Commands,
    _non_send: bevy::ecs::system::NonSendMarker,
) {
    for effect in effects.read() {
        if windows.contains(effect.window) && NativeWindowButtons::apply(*effect) {
            commands.entity(effect.window).insert(WindowButtonsApplied {
                visible: effect.visible,
            });
        }
    }
}

#[cfg(target_os = "macos")]
struct NativeWindowButtons;

#[cfg(target_os = "macos")]
impl NativeWindowButtons {
    fn apply(effect: WindowButtonsEffect) -> bool {
        bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
            let Some(window) = windows.get_window(effect.window) else {
                return false;
            };
            let Ok(handle) = window.window_handle() else {
                return false;
            };
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return false;
            };
            unsafe {
                let send_no_args: unsafe extern "C" fn(
                    *mut libc::c_void,
                    *const libc::c_void,
                ) -> *mut libc::c_void = std::mem::transmute(objc_ffi::objc_msgSend as *const ());
                let send_u64: unsafe extern "C" fn(
                    *mut libc::c_void,
                    *const libc::c_void,
                    u64,
                ) -> *mut libc::c_void = std::mem::transmute(objc_ffi::objc_msgSend as *const ());
                let send_bool: unsafe extern "C" fn(
                    *mut libc::c_void,
                    *const libc::c_void,
                    libc::c_schar,
                ) = std::mem::transmute(objc_ffi::objc_msgSend as *const ());
                let ns_window =
                    send_no_args(appkit.ns_view.as_ptr(), objc_ffi::Selector::get(c"window"));
                if ns_window.is_null() {
                    return false;
                }
                let hidden: libc::c_schar = if effect.visible { 0 } else { 1 };
                for button_type in 0u64..=2 {
                    let button = send_u64(
                        ns_window,
                        objc_ffi::Selector::get(c"standardWindowButton:"),
                        button_type,
                    );
                    if !button.is_null() {
                        send_bool(button, objc_ffi::Selector::get(c"setHidden:"), hidden);
                    }
                }
            }
            true
        })
    }
}

#[cfg(target_os = "macos")]
mod objc_ffi {
    unsafe extern "C" {
        pub fn objc_msgSend(
            obj: *mut libc::c_void,
            sel: *const libc::c_void,
            ...
        ) -> *mut libc::c_void;
        fn sel_registerName(name: *const libc::c_char) -> *const libc::c_void;
    }

    pub(super) struct Selector;

    impl Selector {
        pub(super) fn get(name: &std::ffi::CStr) -> *const libc::c_void {
            unsafe { sel_registerName(name.as_ptr()) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy_cef::prelude::HostWindow;

    struct Windows {
        app: App,
        cursor: MessageCursor<WindowButtonsEffect>,
    }

    impl Windows {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, WindowButtonsPlugin));
            Self {
                app,
                cursor: MessageCursor::default(),
            }
        }

        fn window(&mut self) -> Entity {
            self.app.world_mut().spawn(Window::default()).id()
        }

        fn header(&mut self, window: Entity) -> Entity {
            let root = self.app.world_mut().spawn(HostWindow(window)).id();
            self.app
                .world_mut()
                .spawn((crate::Header, crate::Open, ChildOf(root)))
                .id()
        }

        fn update(&mut self) -> Vec<WindowButtonsEffect> {
            self.app.update();
            self.cursor
                .read(self.app.world().resource::<Messages<WindowButtonsEffect>>())
                .copied()
                .collect()
        }

        fn acknowledge(&mut self, effects: &[WindowButtonsEffect]) {
            for effect in effects {
                self.app
                    .world_mut()
                    .entity_mut(effect.window)
                    .insert(WindowButtonsApplied {
                        visible: effect.visible,
                    });
            }
        }
    }

    #[test]
    fn only_the_window_with_an_open_header_shows_buttons() {
        let mut windows = Windows::new();
        let first = windows.window();
        let second = windows.window();
        let header = windows.header(first);
        let effects = windows.update();
        assert_eq!(effects.len(), 2);
        assert!(effects.contains(&WindowButtonsEffect {
            window: first,
            visible: true
        }));
        assert!(effects.contains(&WindowButtonsEffect {
            window: second,
            visible: false
        }));
        windows.acknowledge(&effects);
        assert!(windows.update().is_empty());
        windows
            .app
            .world_mut()
            .entity_mut(header)
            .remove::<crate::Open>();
        assert_eq!(
            windows.update(),
            vec![WindowButtonsEffect {
                window: first,
                visible: false
            }]
        );
    }

    #[test]
    fn unavailable_native_window_retries_until_acknowledged() {
        let mut windows = Windows::new();
        let window = windows.window();
        windows.header(window);
        let effects = windows.update();
        assert_eq!(
            effects,
            vec![WindowButtonsEffect {
                window,
                visible: true
            }]
        );
        assert_eq!(windows.update(), effects);
        windows.acknowledge(&effects);
        assert!(windows.update().is_empty());
    }

    #[test]
    fn removing_and_reattaching_window_reapplies_unchanged_visibility() {
        let mut windows = Windows::new();
        let window = windows.window();
        windows.header(window);
        let effects = windows.update();
        windows.acknowledge(&effects);
        assert!(windows.update().is_empty());
        windows
            .app
            .world_mut()
            .entity_mut(window)
            .remove::<Window>();
        windows
            .app
            .world_mut()
            .entity_mut(window)
            .insert(Window::default());
        assert_eq!(windows.update(), effects);
    }

    #[test]
    fn reparenting_header_updates_both_windows() {
        let mut windows = Windows::new();
        let first = windows.window();
        let second = windows.window();
        let header = windows.header(first);
        let effects = windows.update();
        windows.acknowledge(&effects);
        let root = windows.app.world_mut().spawn(HostWindow(second)).id();
        windows
            .app
            .world_mut()
            .entity_mut(header)
            .insert(ChildOf(root));
        let effects = windows.update();
        assert_eq!(effects.len(), 2);
        assert!(effects.contains(&WindowButtonsEffect {
            window: first,
            visible: false
        }));
        assert!(effects.contains(&WindowButtonsEffect {
            window: second,
            visible: true
        }));
    }

    #[test]
    fn removed_window_stops_effects_without_retaining_its_receipt() {
        let mut windows = Windows::new();
        let window = windows.window();
        windows.header(window);
        let effects = windows.update();
        windows.acknowledge(&effects);
        windows
            .app
            .world_mut()
            .entity_mut(window)
            .remove::<Window>();
        assert!(windows.update().is_empty());
        assert!(
            windows
                .app
                .world()
                .get::<WindowButtonsApplied>(window)
                .is_none()
        );
        windows.app.world_mut().entity_mut(window).despawn();
        assert!(windows.update().is_empty());
    }
}
