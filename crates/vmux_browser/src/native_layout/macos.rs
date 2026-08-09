//! macOS layout forwarding. The layout shell is offscreen-rendered into a `CALayer`
//! above the panes, and a `CALayer` is not in AppKit's hit-test chain, so the AppKit
//! event monitors hand pointer, scroll and click events here instead of through winit.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use bevy::prelude::*;
use bevy_cef_core::prelude::{NativeMouseButtons, NativeMouseMovePresenter};

use crate::{
    CefPointerHitRect, NATIVE_LAYOUT_ACTIVITY, NATIVE_LAYOUT_POINTER_INSIDE,
    cef_pointer_hit_rect_contains, native_layout_pointer_is_inside,
};

#[derive(Default)]
struct NativeLayoutPointerState {
    regions: Vec<CefPointerHitRect>,
    pointer_inside: bool,
    position_px: Option<Vec2>,
    buttons: NativeMouseButtons,
    pending: bool,
}

static NATIVE_LAYOUT_POINTER_STATE: LazyLock<Mutex<NativeLayoutPointerState>> =
    LazyLock::new(|| Mutex::new(NativeLayoutPointerState::default()));

#[derive(Default)]
struct NativeLayoutMousePresenterState {
    scale: f32,
    presenter: Option<NativeMouseMovePresenter>,
}

thread_local! {
    static NATIVE_LAYOUT_MOUSE_PRESENTER: RefCell<NativeLayoutMousePresenterState> =
        RefCell::new(NativeLayoutMousePresenterState::default());
}

/// Whether a press was handed to the layout webview and still owes it a release.
static NATIVE_LAYOUT_CLICK_LATCH: AtomicBool = AtomicBool::new(false);

/// When the AppKit monitor last handed the layout webview a wheel event.
///
/// Swallowing the `NSEvent` keeps it out of winit, so `MouseWheel` never reaches Bevy and the
/// frame-rate governor would leave the layout at its idle rate for the whole scroll.
static NATIVE_LAYOUT_SCROLL_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeLayoutPointerMoveResult {
    pub owns_pointer: bool,
    pub presenter_active: bool,
    pub region_changed: bool,
    pub pending: bool,
}

impl super::NativeLayout {
    pub(crate) fn last_scroll_at() -> Option<std::time::Instant> {
        *NATIVE_LAYOUT_SCROLL_AT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub(crate) fn physical_cef_pointer_hit_rect(
    mut rect: CefPointerHitRect,
    scale: f32,
) -> CefPointerHitRect {
    rect.center *= scale;
    rect.size *= scale;
    rect
}

pub(crate) fn set_native_layout_pointer_regions(
    regions: impl IntoIterator<Item = CefPointerHitRect>,
) {
    let mut state = NATIVE_LAYOUT_POINTER_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.regions.clear();
    state.regions.extend(regions);
}

pub(crate) fn set_native_layout_mouse_presenter(
    scale: f32,
    presenter: Option<NativeMouseMovePresenter>,
) {
    NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow_mut(|state| {
        state.scale = scale;
        let same_browser = state
            .presenter
            .as_ref()
            .map(NativeMouseMovePresenter::browser_id)
            == presenter.as_ref().map(NativeMouseMovePresenter::browser_id);
        if !same_browser {
            state.presenter = presenter;
        }
    });
}

fn native_layout_mouse_presenter_active() -> bool {
    NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| state.presenter.is_some())
}

pub(crate) fn clear_native_layout_pointer_state() {
    let should_flush = {
        let mut state = NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.regions.clear();
        let should_flush = state.pointer_inside && state.position_px.is_some();
        state.pointer_inside = false;
        state.pending |= should_flush;
        should_flush
    };
    NATIVE_LAYOUT_POINTER_INSIDE.store(false, Ordering::Relaxed);
    NATIVE_LAYOUT_ACTIVITY.store(false, Ordering::Relaxed);
    if should_flush {
        flush_native_layout_pointer_move();
    }
    set_native_layout_mouse_presenter(1.0, None);
}

fn queue_native_layout_pointer_sample(
    state: &mut NativeLayoutPointerState,
    position: Vec2,
    buttons: NativeMouseButtons,
) -> NativeLayoutPointerMoveResult {
    let inside = state
        .regions
        .iter()
        .copied()
        .any(|rect| cef_pointer_hit_rect_contains(rect, position));
    let was_inside = state.pointer_inside;
    let sample_changed =
        state.position_px != Some(position) || state.buttons != buttons || was_inside != inside;
    state.pointer_inside = inside;
    state.position_px = Some(position);
    state.buttons = buttons;
    if (was_inside || inside) && sample_changed {
        state.pending = true;
    }
    NativeLayoutPointerMoveResult {
        owns_pointer: was_inside || inside,
        presenter_active: false,
        region_changed: was_inside != inside,
        pending: state.pending,
    }
}

pub fn queue_native_layout_pointer_move(
    x_px: f32,
    y_px: f32,
    buttons: NativeMouseButtons,
) -> NativeLayoutPointerMoveResult {
    if !x_px.is_finite() || !y_px.is_finite() {
        return NativeLayoutPointerMoveResult::default();
    }
    let (mut result, inside) = {
        let mut state = NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let position = Vec2::new(x_px, y_px);
        let result = queue_native_layout_pointer_sample(&mut state, position, buttons);
        (result, state.pointer_inside)
    };
    NATIVE_LAYOUT_POINTER_INSIDE.store(inside, Ordering::Relaxed);
    result.presenter_active = native_layout_mouse_presenter_active();
    result
}

pub fn flush_native_layout_pointer_move() -> bool {
    let Some((position_px, buttons, inside)) = ({
        let mut state = NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.pending {
            None
        } else {
            state.pending = false;
            state
                .position_px
                .map(|position| (position, state.buttons, state.pointer_inside))
        }
    }) else {
        return false;
    };
    let forwarded = NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| {
        let Some(presenter) = state.presenter.as_ref() else {
            return false;
        };
        if !state.scale.is_finite() || state.scale <= 0.0 {
            return false;
        }
        presenter.send(position_px / state.scale, buttons, !inside);
        true
    });
    if !forwarded {
        NATIVE_LAYOUT_POINTER_STATE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pending = true;
    }
    forwarded
}

/// Hand a wheel event to the layout webview, reporting whether it took it.
///
/// The layout shell is offscreen-rendered into a `CALayer` above the panes, but a `CALayer` is not
/// in AppKit's hit-test chain: a native windowed page underneath wins every `scrollWheel` aimed at
/// whatever the layout is drawing on top. The monitor swallows the event when this returns true.
pub fn forward_native_layout_scroll(position_px: Vec2, delta: Vec2) -> bool {
    if !native_layout_pointer_is_inside() {
        return false;
    }
    let forwarded = NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| {
        let Some(presenter) = state.presenter.as_ref() else {
            return false;
        };
        if !state.scale.is_finite() || state.scale <= 0.0 {
            return false;
        }
        presenter.send_wheel(position_px / state.scale, delta);
        true
    });
    if forwarded {
        NATIVE_LAYOUT_SCROLL_AT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .replace(std::time::Instant::now());
    }
    forwarded
}

/// Hand a click to the layout webview, reporting whether it took it.
///
/// Only for clicks AppKit would otherwise deliver to a native windowed pane. Everywhere else the
/// winit content view is the hit-test target, winit raises `MouseButtonInput`, and the ordinary
/// Bevy pointer path already works — swallowing those would trade a working route for this one.
pub fn forward_native_layout_click(
    position_px: Vec2,
    button: bevy::picking::pointer::PointerButton,
    mouse_up: bool,
) -> bool {
    // The release has to follow the press that was forwarded, even if the pointer has since left
    // the hit region — the regions track the cursor, and a press delivered without its release
    // leaves the DOM latched in a pressed state.
    let latched = NATIVE_LAYOUT_CLICK_LATCH.load(Ordering::Relaxed);
    let owed_release = mouse_up && latched;
    if !native_layout_pointer_is_inside() && !owed_release {
        return false;
    }
    let forwarded = NATIVE_LAYOUT_MOUSE_PRESENTER.with_borrow(|state| {
        let Some(presenter) = state.presenter.as_ref() else {
            return false;
        };
        if !state.scale.is_finite() || state.scale <= 0.0 {
            return false;
        }
        presenter.send_click(position_px / state.scale, button, mouse_up);
        true
    });
    if forwarded {
        NATIVE_LAYOUT_CLICK_LATCH.store(!mouse_up, Ordering::Relaxed);
    }
    forwarded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_layout_pointer_queue_skips_identical_sample() {
        let mut state = NativeLayoutPointerState {
            regions: vec![CefPointerHitRect {
                center: Vec2::new(50.0, 25.0),
                size: Vec2::new(20.0, 10.0),
                interactive: true,
            }],
            ..Default::default()
        };
        let buttons = NativeMouseButtons::default();

        let entered =
            queue_native_layout_pointer_sample(&mut state, Vec2::new(50.0, 25.0), buttons);
        assert!(entered.owns_pointer);
        assert!(entered.region_changed);
        assert!(entered.pending);
        state.pending = false;
        let duplicate =
            queue_native_layout_pointer_sample(&mut state, Vec2::new(50.0, 25.0), buttons);
        assert!(duplicate.owns_pointer);
        assert!(!duplicate.region_changed);
        assert!(!duplicate.pending);
        let moved = queue_native_layout_pointer_sample(&mut state, Vec2::new(51.0, 25.0), buttons);
        assert!(moved.owns_pointer);
        assert!(!moved.region_changed);
        assert!(moved.pending);
    }
}
