//! [`NSGlassEffectView`] via [liquid-glass-rs] behind the winit content view.
//!
//! We do not keep [`GlassViewManager`] in a [`Resource`]: the crate is not `Send`/`Sync` due to
//! ObjC pointers; the glass subview stays in the `NSView` hierarchy after `add_glass_view`.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_winit::WINIT_WINDOWS;
use liquid_glass_rs::{GlassOptions, GlassViewManager};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn apply_macos_liquid_glass(
    primary: Query<Entity, With<PrimaryWindow>>,
    mut attempts: Local<u32>,
    mut applied: Local<bool>,
) {
    const MAX_ATTEMPTS: u32 = 180;
    if *applied {
        return;
    }
    if *attempts >= MAX_ATTEMPTS {
        *applied = true;
        return;
    }

    let Ok(entity) = primary.single() else {
        return;
    };

    WINIT_WINDOWS.with(|cell| {
        let windows = cell.borrow();
        let Some(winit) = windows.get_window(entity) else {
            *attempts += 1;
            return;
        };

        winit.set_transparent(true);

        let window_handle = match winit.window_handle() {
            Ok(h) => h,
            Err(_) => {
                *attempts += 1;
                return;
            }
        };

        let ptr = match window_handle.as_raw() {
            RawWindowHandle::AppKit(h) => h.ns_view.as_ptr().cast::<std::ffi::c_void>(),
            _ => {
                warn!("vmux: expected AppKit window handle for liquid glass");
                *attempts = MAX_ATTEMPTS;
                return;
            }
        };

        let manager = GlassViewManager::new();
        let options = GlassOptions {
            corner_radius: 12.0,
            tint_color: Some("#ffffff10".to_string()),
            opaque: false,
        };

        match manager.add_glass_view(ptr, options) {
            Ok(_view_id) => {
                *applied = true;
            }
            Err(e) => {
                *attempts += 1;
                if *attempts >= MAX_ATTEMPTS {
                    warn!(
                        "vmux: liquid glass failed after retries ({e:?}); falling back to CGS blur"
                    );
                    winit.set_blur(true);
                    *applied = true;
                }
            }
        }
    });
}
