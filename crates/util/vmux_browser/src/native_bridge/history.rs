use bevy::prelude::*;
#[cfg(target_os = "macos")]
use bevy::winit::WINIT_WINDOWS;
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::NativeBridge;
use crate::NavigationOperation;
use crate::present::WindowedFrameRect;

pub(crate) struct NativeHistoryPlugin;

#[cfg(target_os = "macos")]
impl Plugin for NativeHistoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            publish
                .after(crate::BrowserSystemSet::SyncWindowedFrames)
                .after(vmux_layout::presentation::NativePresentationSet::Sync),
        );
    }
}

#[cfg(not(target_os = "macos"))]
impl Plugin for NativeHistoryPlugin {
    fn build(&self, _app: &mut App) {
        NativeBridge::replace_history_regions(Vec::new());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeHistoryPhase {
    Begin,
    Changed,
    End,
    Cancel,
    Momentum,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HistoryRegion {
    webview: Entity,
    window: isize,
    frame: WindowedFrameRect,
}

impl HistoryRegion {
    fn contains(self, window: isize, position: Vec2) -> bool {
        self.window == window
            && position.is_finite()
            && position.x >= self.frame.left
            && position.x < self.frame.right()
            && position.y >= self.frame.top
            && position.y < self.frame.bottom()
    }
}

struct ScrollInput {
    window: isize,
    position: Vec2,
    delta: Vec2,
    phase: NativeHistoryPhase,
    at: Instant,
}

struct Gesture {
    region: HistoryRegion,
    delta: Vec2,
    last_input: Instant,
    claimed: bool,
    ended: bool,
}

impl Gesture {
    fn record(&mut self, input: &ScrollInput) -> bool {
        if self.ended
            || !self.region.contains(input.window, input.position)
            || !input.delta.is_finite()
        {
            return false;
        }
        self.last_input = input.at;
        if input.delta == Vec2::ZERO {
            return true;
        }
        if input.delta.y.abs() > input.delta.x.abs() * 0.65 {
            return false;
        }
        if self.delta.x != 0.0
            && input.delta.x != 0.0
            && self.delta.x.signum() != input.delta.x.signum()
        {
            self.delta = Vec2::ZERO;
        }
        self.delta += input.delta;
        self.delta.is_finite() && self.delta.y.abs() <= self.delta.x.abs() * 0.65
    }

    fn matches(&self, webview: Entity, operation: NavigationOperation, at: Instant) -> bool {
        if self.claimed
            || self.region.webview != webview
            || at.saturating_duration_since(self.last_input) > Duration::from_millis(300)
            || self.delta.x.abs() < 200.0
        {
            return false;
        }
        match operation {
            NavigationOperation::Back => self.delta.x < 0.0,
            NavigationOperation::Forward => self.delta.x > 0.0,
            _ => false,
        }
    }
}

#[derive(Default)]
struct HistoryWitness {
    regions: Vec<HistoryRegion>,
    gesture: Option<Gesture>,
    invalidation: u64,
}

impl HistoryWitness {
    fn sync_invalidation(&mut self, invalidation: u64) {
        if self.invalidation != invalidation {
            self.gesture = None;
            self.invalidation = invalidation;
        }
    }

    fn invalidate(&mut self, webview: Entity) {
        if self
            .gesture
            .as_ref()
            .is_some_and(|gesture| gesture.region.webview == webview)
        {
            self.gesture = None;
        }
    }

    fn replace(&mut self, mut regions: Vec<HistoryRegion>) {
        regions.truncate(64);
        if self
            .gesture
            .as_ref()
            .is_some_and(|gesture| !regions.contains(&gesture.region))
        {
            self.gesture = None;
        }
        self.regions = regions;
    }

    fn record(&mut self, input: ScrollInput) {
        match input.phase {
            NativeHistoryPhase::Cancel | NativeHistoryPhase::Unsupported => {
                self.gesture = None;
                return;
            }
            NativeHistoryPhase::Momentum => return,
            NativeHistoryPhase::Begin => {
                let mut region = None;
                for candidate in &self.regions {
                    if candidate.contains(input.window, input.position) {
                        if region.is_some() {
                            self.gesture = None;
                            return;
                        }
                        region = Some(*candidate);
                    }
                }
                self.gesture = region.map(|region| Gesture {
                    region,
                    delta: Vec2::ZERO,
                    last_input: input.at,
                    claimed: false,
                    ended: false,
                });
            }
            NativeHistoryPhase::Changed => {}
            NativeHistoryPhase::End => {
                if let Some(gesture) = self.gesture.as_mut() {
                    if !gesture.region.contains(input.window, input.position) {
                        self.gesture = None;
                        return;
                    }
                    gesture.ended = true;
                }
                return;
            }
        }
        if let Some(gesture) = self.gesture.as_mut()
            && !gesture.record(&input)
        {
            self.gesture = None;
        }
    }

    fn claim(&mut self, webview: Entity, operation: NavigationOperation, at: Instant) -> bool {
        let Some(gesture) = self.gesture.as_mut() else {
            return false;
        };
        if !self.regions.contains(&gesture.region) || !gesture.matches(webview, operation, at) {
            return false;
        }
        gesture.claimed = true;
        true
    }
}

static HISTORY: LazyLock<Mutex<HistoryWitness>> = LazyLock::new(Default::default);
static INVALIDATIONS: AtomicU64 = AtomicU64::new(0);

impl NativeBridge {
    pub(crate) fn invalidate_history_input(webview: Entity) {
        match HISTORY.try_lock() {
            Ok(mut history) => {
                history.sync_invalidation(INVALIDATIONS.load(Ordering::SeqCst));
                history.invalidate(webview);
            }
            Err(_) => {
                INVALIDATIONS.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    pub fn record_history_wheel(
        window: isize,
        position: Vec2,
        delta: Vec2,
        phase: NativeHistoryPhase,
    ) {
        if let Ok(mut history) = HISTORY.try_lock() {
            history.sync_invalidation(INVALIDATIONS.load(Ordering::SeqCst));
            history.record(ScrollInput {
                window,
                position,
                delta,
                phase,
                at: Instant::now(),
            });
            history.sync_invalidation(INVALIDATIONS.load(Ordering::SeqCst));
        }
    }

    pub(crate) fn replace_history_regions(regions: Vec<HistoryRegion>) {
        if let Ok(mut history) = HISTORY.try_lock() {
            history.sync_invalidation(INVALIDATIONS.load(Ordering::SeqCst));
            history.replace(regions);
        }
    }

    pub(crate) fn claim_history_input(webview: Entity, operation: NavigationOperation) -> bool {
        let Ok(mut history) = HISTORY.try_lock() else {
            return false;
        };
        let invalidation = INVALIDATIONS.load(Ordering::SeqCst);
        history.sync_invalidation(invalidation);
        let accepted = history.claim(webview, operation, Instant::now());
        if INVALIDATIONS.load(Ordering::SeqCst) != invalidation {
            history.sync_invalidation(INVALIDATIONS.load(Ordering::SeqCst));
            return false;
        }
        accepted
    }
}

#[cfg(target_os = "macos")]
fn publish(
    _native_thread: bevy::ecs::system::NonSendMarker,
    pages: Query<(Entity, &crate::present::PaneFrame), With<vmux_layout::Browser>>,
    windows: Query<&Window>,
    hierarchy: vmux_layout::window::WindowHierarchy,
    parents: Query<&ChildOf>,
    surfaces: vmux_layout::hosted_page::StackSurface,
    blocked: Query<
        (),
        Or<(
            With<bevy_cef::prelude::BinIpcBlocked>,
            With<vmux_layout::hosted_page::SurfaceBlocked>,
        )>,
    >,
) {
    let mut regions = Vec::new();
    WINIT_WINDOWS.with(|winit_windows| {
        let winit_windows = winit_windows.borrow();
        for (webview, frame) in &pages {
            if regions.len() == 64 || blocked.contains(webview) {
                continue;
            }
            let Ok(parent) = parents.get(webview) else {
                continue;
            };
            if !surfaces.is_current(parent.parent(), webview) {
                continue;
            }
            let Some(host) = hierarchy.get(webview) else {
                continue;
            };
            let Ok(window) = windows.get(host) else {
                continue;
            };
            let Some(native) = winit_windows.get_window(host) else {
                continue;
            };
            let Ok(handle) = native.window_handle() else {
                continue;
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                continue;
            };
            let view: &objc2_app_kit::NSView = unsafe { &*handle.ns_view.as_ptr().cast() };
            let Some(native_window) = view.window() else {
                continue;
            };
            let scale = window.resolution.scale_factor();
            if !scale.is_finite() || scale <= 0.0 {
                continue;
            }
            let frame = WindowedFrameRect {
                left: frame.left * scale,
                top: frame.top * scale,
                width: frame.width * scale,
                height: frame.height * scale,
            };
            if !frame.left.is_finite()
                || !frame.top.is_finite()
                || !frame.width.is_finite()
                || !frame.height.is_finite()
                || frame.width <= 0.0
                || frame.height <= 0.0
            {
                continue;
            }
            regions.push(HistoryRegion {
                webview,
                window: native_window.windowNumber(),
                frame,
            });
        }
    });
    NativeBridge::replace_history_regions(regions);
}

#[cfg(test)]
mod tests {
    use super::*;

    impl HistoryWitness {
        fn over(webview: Entity, window: isize, frame: WindowedFrameRect) -> Self {
            Self {
                regions: vec![HistoryRegion {
                    webview,
                    window,
                    frame,
                }],
                gesture: None,
                invalidation: 0,
            }
        }

        fn scrolling(
            &mut self,
            window: isize,
            position: Vec2,
            delta: Vec2,
            phase: NativeHistoryPhase,
            at: Instant,
        ) {
            self.record(ScrollInput {
                window,
                position,
                delta,
                phase,
                at,
            });
        }
    }

    #[test]
    fn short_horizontal_scrolls_do_not_authorize_fabricated_history_callbacks() {
        let at = Instant::now();
        let view = Entity::from_bits(100);
        let frame = WindowedFrameRect {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let mut witness = HistoryWitness::over(view, 7, frame);
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-32.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        assert!(!witness.claim(view, NavigationOperation::Back, at));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-167.0, 0.0),
            NativeHistoryPhase::Changed,
            at + Duration::from_millis(16),
        );
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(17)
        ));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-1.0, 0.0),
            NativeHistoryPhase::Changed,
            at + Duration::from_millis(32),
        );
        assert!(witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(33)
        ));
    }

    #[test]
    fn reloading_the_same_surface_and_frame_discards_unclaimed_input_from_the_previous_document() {
        let at = Instant::now();
        let view = Entity::from_bits(100);
        let frame = WindowedFrameRect {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let mut witness = HistoryWitness::over(view, 7, frame);
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        witness.invalidate(view);
        witness.replace(vec![HistoryRegion {
            webview: view,
            window: 7,
            frame,
        }]);
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(1)
        ));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Changed,
            at + Duration::from_millis(16),
        );
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(17)
        ));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Momentum,
            at + Duration::from_millis(32),
        );
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(33)
        ));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at + Duration::from_millis(48),
        );
        assert!(witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(49)
        ));
    }

    #[test]
    fn a_contended_document_invalidation_also_cancels_the_previous_input() {
        let at = Instant::now();
        let view = Entity::from_bits(100);
        let frame = WindowedFrameRect {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let mut witness = HistoryWitness::over(view, 7, frame);
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        witness.sync_invalidation(1);
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(1)
        ));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Changed,
            at + Duration::from_millis(16),
        );
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(17)
        ));
    }

    #[test]
    fn changing_the_root_window_binding_discards_input_even_when_the_frame_is_reused() {
        let at = Instant::now();
        let view = Entity::from_bits(100);
        let frame = WindowedFrameRect {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let mut witness = HistoryWitness::over(view, 7, frame);
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        witness.replace(vec![HistoryRegion {
            webview: view,
            window: 8,
            frame,
        }]);
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(1)
        ));
    }

    #[test]
    fn only_a_real_horizontal_burst_can_be_claimed_once_by_its_surface_and_direction() {
        let at = Instant::now();
        let view = Entity::from_bits(100);
        let other = Entity::from_bits(101);
        let frame = WindowedFrameRect {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let mut witness = HistoryWitness::over(view, 7, frame);
        assert!(!witness.claim(view, NavigationOperation::Back, at));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-105.0, 1.0),
            NativeHistoryPhase::Begin,
            at,
        );
        assert!(!witness.claim(view, NavigationOperation::Back, at));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-105.0, 1.0),
            NativeHistoryPhase::Changed,
            at + Duration::from_millis(16),
        );
        assert!(!witness.claim(
            other,
            NavigationOperation::Back,
            at + Duration::from_millis(17)
        ));
        assert!(!witness.claim(
            view,
            NavigationOperation::Forward,
            at + Duration::from_millis(17)
        ));
        assert!(witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(17)
        ));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-400.0, 0.0),
            NativeHistoryPhase::Momentum,
            at + Duration::from_millis(30),
        );
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(31)
        ));
    }

    #[test]
    fn a_witness_expires_and_cannot_follow_a_changed_frame_or_replacement_surface() {
        let at = Instant::now();
        let view = Entity::from_bits(100);
        let frame = WindowedFrameRect {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let mut witness = HistoryWitness::over(view, 7, frame);
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        assert!(!witness.claim(
            view,
            NavigationOperation::Back,
            at + Duration::from_millis(301)
        ));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        witness.replace(vec![HistoryRegion {
            webview: view,
            window: 7,
            frame: WindowedFrameRect {
                left: 250.0,
                ..frame
            },
        }]);
        assert!(!witness.claim(view, NavigationOperation::Back, at));
        witness.replace(vec![HistoryRegion {
            webview: view,
            window: 7,
            frame,
        }]);
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        witness.replace(vec![HistoryRegion {
            webview: Entity::from_bits(101),
            window: 7,
            frame,
        }]);
        assert!(!witness.claim(view, NavigationOperation::Back, at));
    }

    #[test]
    fn other_windows_vertical_scrolls_cancelled_phases_and_unphased_wheels_cannot_authorize_history()
     {
        let at = Instant::now();
        let view = Entity::from_bits(100);
        let frame = WindowedFrameRect {
            left: 10.0,
            top: 20.0,
            width: 200.0,
            height: 100.0,
        };
        let mut witness = HistoryWitness::over(view, 7, frame);
        witness.scrolling(
            8,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        assert!(!witness.claim(view, NavigationOperation::Back, at));
        witness.scrolling(
            7,
            Vec2::new(350.0, 50.0),
            Vec2::new(-240.0, 0.0),
            NativeHistoryPhase::Begin,
            at,
        );
        assert!(!witness.claim(view, NavigationOperation::Back, at));
        witness.scrolling(
            7,
            Vec2::new(50.0, 50.0),
            Vec2::new(-240.0, 600.0),
            NativeHistoryPhase::Begin,
            at,
        );
        assert!(!witness.claim(view, NavigationOperation::Back, at));
        for phase in [NativeHistoryPhase::Cancel, NativeHistoryPhase::Unsupported] {
            witness.scrolling(
                7,
                Vec2::new(50.0, 50.0),
                Vec2::new(-240.0, 0.0),
                NativeHistoryPhase::Begin,
                at,
            );
            witness.scrolling(7, Vec2::new(50.0, 50.0), Vec2::ZERO, phase, at);
            assert!(!witness.claim(view, NavigationOperation::Back, at));
        }
    }
}
