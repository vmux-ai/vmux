use super::Open;
use crate::settings::LayoutSettings;
use bevy::prelude::*;
#[cfg(target_os = "macos")]
use bevy::{ecs::system::NonSendMarker, winit::WINIT_WINDOWS};
#[cfg(target_os = "macos")]
use bevy_cef::prelude::HostWindow;
use vmux_flex::prelude::*;

impl Plugin for SideSheetLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SideSheetSectionsExpanded>()
            .register_type::<SideSheetPaneExpanded>()
            .insert_resource(SideSheetWidth(0.0))
            .add_systems(
                PostUpdate,
                (
                    sync_side_sheet_visibility.before(LayoutSystems::Layout),
                    sync_window_buttons_visibility,
                ),
            );
    }
}

pub(crate) struct SideSheetLayoutPlugin;

#[derive(Component)]
pub struct SideSheet;

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::side_sheet"]
#[require(moonshine_save::prelude::Save)]
pub struct SideSheetSectionsExpanded {
    pub projects: bool,
    pub bookmarks: bool,
    pub knowledge: bool,
    pub tools: bool,
}

impl SideSheetSectionsExpanded {
    pub fn set(&mut self, section: &str, expanded: bool) -> bool {
        let value = match section {
            "bookmarks" => &mut self.bookmarks,
            _ => return false,
        };
        *value = expanded;
        true
    }

    pub fn is_empty(self) -> bool {
        !self.bookmarks
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct SideSheetSections<'w, 's> {
    spaces: Query<'w, 's, (), With<super::space::Space>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    expanded: Query<'w, 's, &'static SideSheetSectionsExpanded, With<super::space::Space>>,
}

impl SideSheetSections<'_, '_> {
    pub fn space_of(&self, entity: Entity) -> Option<Entity> {
        super::space::space_of(entity, &self.child_of, &self.spaces)
    }

    pub fn under(&self, entity: Entity) -> SideSheetSectionsExpanded {
        let Some(space) = self.space_of(entity) else {
            return SideSheetSectionsExpanded::default();
        };

        self.expanded.get(space).copied().unwrap_or_default()
    }
}

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::side_sheet"]
#[require(moonshine_save::prelude::Save)]
pub struct SideSheetPaneExpanded;

#[derive(Component, PartialEq, Eq)]
pub enum SideSheetPosition {
    Left,
    Right,
    Bottom,
}

#[derive(Resource)]
pub struct SideSheetWidth(pub f32);

impl SideSheetWidth {
    pub fn apply(
        &mut self,
        width: f32,
        sheets: &mut Query<(&SideSheetPosition, &mut Node), With<SideSheet>>,
    ) {
        self.0 = width;
        for (position, mut node) in sheets {
            if *position == SideSheetPosition::Left {
                node.width = Val::Px(width);
            }
        }
    }
}

fn sync_side_sheet_visibility(
    settings: Res<LayoutSettings>,
    mut width_res: ResMut<SideSheetWidth>,
    mut side_sheet_q: Query<
        (Entity, &SideSheetPosition, &mut Visibility, &mut Node),
        With<SideSheet>,
    >,
    added: Query<Entity, (With<SideSheet>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    if width_res.0 <= 0.0 {
        width_res.0 = crate::event::SideSheetResizeEvent::live(settings.side_sheet.width).clamped();
    }

    let width = width_res.0;
    for entity in &added {
        if let Ok((_, pos, mut visibility, mut node)) = side_sheet_q.get_mut(entity)
            && *pos == SideSheetPosition::Left
        {
            *visibility = Visibility::Visible;
            node.display = Display::Flex;
            node.width = Val::Px(width);
        }
    }
    for entity in removed.read() {
        if let Ok((_, pos, mut visibility, mut node)) = side_sheet_q.get_mut(entity)
            && *pos == SideSheetPosition::Left
        {
            *visibility = Visibility::Hidden;
            node.display = Display::None;
        }
    }
}

#[cfg(target_os = "macos")]
fn sync_window_buttons_visibility(
    side_sheet_q: Query<(Entity, &SideSheetPosition, Has<Open>), With<SideSheet>>,
    child_of: Query<&ChildOf>,
    host_windows: Query<&HostWindow>,
    window_q: Query<Entity, With<Window>>,
    mut last_open: Local<std::collections::HashMap<Entity, bool>>,
    _non_send: NonSendMarker,
) {
    for entity in &window_q {
        let is_open = side_sheet_q.iter().any(|(side_sheet, pos, open)| {
            *pos == SideSheetPosition::Left
                && open
                && crate::window::host_window_of(side_sheet, &child_of, &host_windows)
                    == Some(entity)
        });
        if last_open.get(&entity) == Some(&is_open) {
            continue;
        }

        let updated = WINIT_WINDOWS.with_borrow(|winit_windows| {
            let Some(winit_win) = winit_windows.get_window(entity) else {
                return false;
            };

            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let Ok(handle) = winit_win.window_handle() else {
                return false;
            };
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return false;
            };

            let ns_view = appkit.ns_view.as_ptr();
            unsafe {
                use objc_ffi::sel;

                type MsgSendNoArgs = unsafe extern "C" fn(
                    *mut libc::c_void,
                    *const libc::c_void,
                ) -> *mut libc::c_void;
                type MsgSendU64 = unsafe extern "C" fn(
                    *mut libc::c_void,
                    *const libc::c_void,
                    u64,
                ) -> *mut libc::c_void;
                type MsgSendBool =
                    unsafe extern "C" fn(*mut libc::c_void, *const libc::c_void, libc::c_schar);

                let send_no_args: MsgSendNoArgs =
                    std::mem::transmute(objc_ffi::objc_msgSend as *const ());
                let send_u64: MsgSendU64 = std::mem::transmute(objc_ffi::objc_msgSend as *const ());
                let send_bool: MsgSendBool =
                    std::mem::transmute(objc_ffi::objc_msgSend as *const ());

                let ns_window = send_no_args(ns_view, sel("window"));
                if ns_window.is_null() {
                    return false;
                }
                let hidden: libc::c_schar = if is_open { 0 } else { 1 };
                for button_type in 0u64..=2 {
                    let button = send_u64(ns_window, sel("standardWindowButton:"), button_type);
                    if !button.is_null() {
                        send_bool(button, sel("setHidden:"), hidden);
                    }
                }
            }
            true
        });
        if updated {
            last_open.insert(entity, is_open);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn sync_window_buttons_visibility() {}

#[cfg(target_os = "macos")]
mod objc_ffi {
    unsafe extern "C" {
        pub fn objc_msgSend(
            obj: *mut libc::c_void,
            sel: *const libc::c_void,
            ...
        ) -> *mut libc::c_void;
        pub fn sel_registerName(name: *const libc::c_char) -> *const libc::c_void;
    }

    pub fn sel(name: &str) -> *const libc::c_void {
        let c = std::ffi::CString::new(name).unwrap();
        unsafe { sel_registerName(c.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_changes_only_for_the_side_sheet_whose_open_state_changed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutSettings::default())
            .insert_resource(SideSheetWidth(0.0))
            .add_systems(Update, sync_side_sheet_visibility);
        let first = app
            .world_mut()
            .spawn((
                SideSheet,
                SideSheetPosition::Left,
                Open,
                Visibility::Hidden,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                SideSheet,
                SideSheetPosition::Left,
                Visibility::Hidden,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<Visibility>(first),
            Some(&Visibility::Visible)
        );
        assert_eq!(
            app.world().get::<Visibility>(second),
            Some(&Visibility::Hidden)
        );

        app.world_mut().entity_mut(first).remove::<Open>();
        app.world_mut().entity_mut(second).insert(Open);
        app.update();

        assert_eq!(
            app.world().get::<Visibility>(first),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(second),
            Some(&Visibility::Visible)
        );
    }
}
