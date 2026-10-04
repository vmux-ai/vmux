use bevy::prelude::{Entity, Vec2, Window};
#[cfg(target_os = "macos")]
use bevy::winit::WINIT_WINDOWS;
use bevy_cef_core::prelude::NativeMouseButtons;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSEvent, NSView};
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::hint::spin_loop;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering, fence};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static X_BITS: AtomicU32 = AtomicU32::new(0);
static Y_BITS: AtomicU32 = AtomicU32::new(0);
static BUTTONS: AtomicU8 = AtomicU8::new(0);
static MOTION_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static WINDOW_GESTURE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePointerSnapshot {
    pub sequence: u64,
    pub motion_sequence: u64,
    pub position_px: Vec2,
    pub buttons: NativeMouseButtons,
}

pub struct NativePointer;

impl NativePointer {
    pub fn publish(position_px: Vec2, buttons: NativeMouseButtons, motion: bool) {
        let writing = loop {
            let current = SEQUENCE.load(Ordering::Acquire);
            if current & 1 != 0 {
                spin_loop();
                continue;
            }
            if SEQUENCE
                .compare_exchange_weak(
                    current,
                    current.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                break current.wrapping_add(1);
            }
        };
        X_BITS.store(position_px.x.to_bits(), Ordering::Relaxed);
        Y_BITS.store(position_px.y.to_bits(), Ordering::Relaxed);
        BUTTONS.store(Self::button_bits(buttons), Ordering::Relaxed);
        let completed = writing.wrapping_add(1);
        if motion {
            MOTION_SEQUENCE.store(completed, Ordering::Relaxed);
        }
        SEQUENCE.store(completed, Ordering::Release);
    }

    pub fn publish_buttons(buttons: NativeMouseButtons) {
        if let Some(pointer) = Self::snapshot() {
            Self::publish(pointer.position_px, buttons, false);
        }
    }

    pub fn snapshot() -> Option<NativePointerSnapshot> {
        loop {
            let before = SEQUENCE.load(Ordering::Acquire);
            if before == 0 {
                return None;
            }
            if before & 1 != 0 {
                spin_loop();
                continue;
            }
            let position_px = Vec2::new(
                f32::from_bits(X_BITS.load(Ordering::Relaxed)),
                f32::from_bits(Y_BITS.load(Ordering::Relaxed)),
            );
            let buttons = Self::buttons_from_bits(BUTTONS.load(Ordering::Relaxed));
            let motion_sequence = MOTION_SEQUENCE.load(Ordering::Relaxed);
            fence(Ordering::Acquire);
            let after = SEQUENCE.load(Ordering::Relaxed);
            if before == after {
                return Some(NativePointerSnapshot {
                    sequence: after,
                    motion_sequence,
                    position_px,
                    buttons,
                });
            }
        }
    }

    pub fn set_window_gesture(active: bool) {
        WINDOW_GESTURE.store(active, Ordering::Relaxed);
    }

    pub fn window_gesture() -> bool {
        WINDOW_GESTURE.load(Ordering::Relaxed)
    }

    pub fn position(window_entity: Entity, window: &Window) -> Option<Vec2> {
        #[cfg(target_os = "macos")]
        {
            Self::native_position(window_entity, window).or_else(|| {
                window
                    .physical_cursor_position()
                    .map(|position| Vec2::new(position.x, position.y))
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window_entity;
            window
                .physical_cursor_position()
                .map(|position| Vec2::new(position.x, position.y))
        }
    }

    #[cfg(target_os = "macos")]
    fn native_position(window_entity: Entity, window: &Window) -> Option<Vec2> {
        WINIT_WINDOWS.with_borrow(|winit_windows| {
            let mtm = objc2::MainThreadMarker::new()?;
            if !NSApplication::sharedApplication(mtm).isActive() {
                return None;
            }
            let winit_window = winit_windows.get_window(window_entity)?;
            let handle = winit_window.window_handle().ok()?;
            let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
                return None;
            };
            let view: &NSView = unsafe { &*appkit.ns_view.as_ptr().cast::<NSView>() };
            let ns_window = view.window()?;
            let screen_point = NSEvent::mouseLocation();
            let window_point = ns_window.convertPointFromScreen(screen_point);
            let point = view.convertPoint_fromView(window_point, None);
            let bounds = view.bounds();
            let y = if view.isFlipped() {
                point.y
            } else {
                bounds.size.height - point.y
            };
            let scale = window.resolution.scale_factor() as f64;
            let x = point.x * scale;
            let y = y * scale;
            if x.is_finite() && y.is_finite() {
                Some(Vec2::new(x as f32, y as f32))
            } else {
                None
            }
        })
    }

    fn button_bits(buttons: NativeMouseButtons) -> u8 {
        u8::from(buttons.left) | (u8::from(buttons.right) << 1) | (u8::from(buttons.middle) << 2)
    }

    fn buttons_from_bits(bits: u8) -> NativeMouseButtons {
        NativeMouseButtons {
            left: bits & 1 != 0,
            right: bits & (1 << 1) != 0,
            middle: bits & (1 << 2) != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_coherent() {
        NativePointer::publish(
            Vec2::new(123.5, 456.25),
            NativeMouseButtons {
                left: true,
                right: false,
                middle: true,
            },
            true,
        );

        let snapshot = NativePointer::snapshot().expect("snapshot");
        assert_eq!(snapshot.position_px, Vec2::new(123.5, 456.25));
        assert_eq!(
            snapshot.buttons,
            NativeMouseButtons {
                left: true,
                right: false,
                middle: true,
            }
        );
        assert_eq!(snapshot.sequence & 1, 0);
        assert_eq!(snapshot.motion_sequence, snapshot.sequence);
    }

    #[test]
    fn button_update_preserves_position_and_motion_sequence() {
        NativePointer::publish(Vec2::new(10.0, 20.0), NativeMouseButtons::default(), true);
        let before = NativePointer::snapshot().expect("snapshot");

        NativePointer::publish_buttons(NativeMouseButtons {
            left: true,
            right: false,
            middle: false,
        });

        let after = NativePointer::snapshot().expect("snapshot");
        assert_eq!(after.position_px, before.position_px);
        assert_eq!(after.motion_sequence, before.motion_sequence);
        assert!(after.buttons.left);
    }
}
