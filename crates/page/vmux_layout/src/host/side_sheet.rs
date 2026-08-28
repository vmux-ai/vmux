use super::Open;
use crate::settings::LayoutSettings;
#[cfg(target_os = "macos")]
use bevy::{ecs::system::NonSendMarker, winit::WINIT_WINDOWS};
use bevy::{prelude::*, window::PrimaryWindow};
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
            "projects" => &mut self.projects,
            "bookmarks" => &mut self.bookmarks,
            "knowledge" => &mut self.knowledge,
            "tools" => &mut self.tools,
            _ => return false,
        };
        *value = expanded;
        true
    }

    pub fn is_empty(self) -> bool {
        !self.projects && !self.bookmarks && !self.knowledge && !self.tools
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
    let mut left_open: Option<bool> = None;
    for entity in &added {
        if let Ok((_, pos, _, _)) = side_sheet_q.get(entity)
            && *pos == SideSheetPosition::Left
        {
            left_open = Some(true);
        }
    }
    for entity in removed.read() {
        if let Ok((_, pos, _, _)) = side_sheet_q.get(entity)
            && *pos == SideSheetPosition::Left
        {
            left_open = Some(false);
        }
    }

    let Some(is_open) = left_open else { return };

    if width_res.0 <= 0.0 {
        width_res.0 = crate::event::SideSheetResizeEvent {
            width: settings.side_sheet.width,
        }
        .clamped();
    }

    let width = width_res.0;
    for (_, pos, mut vis, mut node) in &mut side_sheet_q {
        if *pos != SideSheetPosition::Left {
            continue;
        }
        if is_open {
            *vis = Visibility::Visible;
            node.display = Display::Flex;
            node.width = Val::Px(width);
        } else {
            *vis = Visibility::Hidden;
            node.display = Display::None;
        }
    }
}

#[cfg(target_os = "macos")]
fn sync_window_buttons_visibility(
    side_sheet_q: Query<(&SideSheetPosition, Has<Open>), With<SideSheet>>,
    window_q: Query<Entity, With<PrimaryWindow>>,
    mut last_open: Local<Option<bool>>,
    _non_send: NonSendMarker,
) {
    let is_open = side_sheet_q
        .iter()
        .any(|(pos, open)| *pos == SideSheetPosition::Left && open);

    if *last_open == Some(is_open) {
        return;
    }

    *last_open = Some(is_open);

    let Ok(entity) = window_q.single() else {
        warn!("sync_window_buttons: no PrimaryWindow entity");
        return;
    };

    WINIT_WINDOWS.with_borrow(|winit_windows| {
        let Some(winit_win) = winit_windows.get_window(entity) else {
            warn!("sync_window_buttons: winit window not found, will retry");
            *last_open = None;
            return;
        };

        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = winit_win.window_handle() else {
            warn!("sync_window_buttons: no window handle");
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            warn!("sync_window_buttons: not AppKit handle");
            return;
        };

        let ns_view = appkit.ns_view.as_ptr();
        unsafe {
            use objc_ffi::sel;

            type MsgSendNoArgs =
                unsafe extern "C" fn(*mut libc::c_void, *const libc::c_void) -> *mut libc::c_void;
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
            let send_bool: MsgSendBool = std::mem::transmute(objc_ffi::objc_msgSend as *const ());

            let ns_window = send_no_args(ns_view, sel("window"));
            if ns_window.is_null() {
                return;
            }
            let hidden: libc::c_schar = if is_open { 0 } else { 1 };
            for button_type in 0u64..=2 {
                let button = send_u64(ns_window, sel("standardWindowButton:"), button_type);
                if !button.is_null() {
                    send_bool(button, sel("setHidden:"), hidden);
                }
            }
        }
    });
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
