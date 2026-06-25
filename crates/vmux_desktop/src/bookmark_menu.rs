use bevy::prelude::*;

pub(crate) struct BookmarkMenuPlugin;

impl Plugin for BookmarkMenuPlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(target_os = "macos")]
        _app.add_systems(Update, macos::show_bookmark_menu);
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use bevy::ecs::system::NonSendMarker;
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;

    thread_local! {
        static HELD_MENU: std::cell::RefCell<Option<muda::Menu>> =
            const { std::cell::RefCell::new(None) };
    }

    /// Pop a native macOS context menu with a "New Folder" item when the page
    /// requests it (right-click on the empty bookmarks placeholder). The item id
    /// matches `AppCommand::from_menu_id("bookmark_new_folder")`, so the existing
    /// `os_menu` event routing dispatches it.
    pub(super) fn show_bookmark_menu(
        _non_send: NonSendMarker,
        mut reader: MessageReader<vmux_layout::bookmark::ShowBookmarkMenuRequest>,
        primary: Query<Entity, With<PrimaryWindow>>,
    ) {
        use bevy::winit::WINIT_WINDOWS;
        use muda::{ContextMenu, Menu, MenuItem};
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let mut requested = false;
        for _ in reader.read() {
            requested = true;
        }
        if !requested {
            return;
        }

        let Ok(window_entity) = primary.single() else {
            return;
        };
        let view_ptr = WINIT_WINDOWS.with_borrow(|windows| {
            let id = windows.entity_to_winit.get(&window_entity)?;
            let wrapper = windows.windows.get(id)?;
            let handle = wrapper.window_handle().ok()?;
            match handle.as_raw() {
                RawWindowHandle::AppKit(h) => Some(h.ns_view.as_ptr()),
                _ => None,
            }
        });
        let Some(view_ptr) = view_ptr else {
            return;
        };

        let menu = Menu::new();
        let item = MenuItem::with_id("bookmark_new_folder", "New Folder", true, None);
        if menu.append(&item).is_err() {
            return;
        }
        unsafe {
            menu.show_context_menu_for_nsview(view_ptr as _, None);
        }
        HELD_MENU.with(|held| *held.borrow_mut() = Some(menu));
    }
}
