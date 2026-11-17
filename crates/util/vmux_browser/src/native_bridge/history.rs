use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::prelude::{
    App, ChildOf, Commands, Entity, MessageReader, On, Or, Plugin, Query, Remove, Window, With,
    Without,
};
use std::time::{Duration, Instant};
use vmux_input::{NativeWheelInput, WheelInputSet, WheelPhase, WheelPlugin};

use crate::NavigationOperation;
use crate::present::{PaneFrame, WindowedFrameRect};

pub(crate) struct NativeHistoryPlugin;

impl Plugin for NativeHistoryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<WheelPlugin>() {
            app.add_plugins(WheelPlugin);
        }
        app.add_systems(
            PreUpdate,
            (initialize, validate, receive).chain().after(WheelInputSet),
        )
        .add_systems(
            PostUpdate,
            publish
                .after(crate::BrowserSystemSet::SyncWindowedFrames)
                .after(vmux_layout::presentation::NativePresentationSet::Sync),
        )
        .add_observer(retire_surface)
        .add_observer(retire_window);
    }
}

#[derive(Component, Clone, Copy)]
struct HistoryRegion {
    window: Entity,
    frame: WindowedFrameRect,
    invalidated_at: Option<Instant>,
}

impl HistoryRegion {
    fn contains(&self, window: Entity, position: Vec2) -> bool {
        self.window == window && self.frame.contains_position(position)
    }
}

impl WindowedFrameRect {
    fn scaled(frame: &PaneFrame, scale: f32) -> Option<Self> {
        let frame = Self {
            left: frame.left * scale,
            top: frame.top * scale,
            width: frame.width * scale,
            height: frame.height * scale,
        };
        if !scale.is_finite()
            || scale <= 0.0
            || !frame.left.is_finite()
            || !frame.top.is_finite()
            || !frame.width.is_finite()
            || !frame.height.is_finite()
            || frame.width <= 0.0
            || frame.height <= 0.0
        {
            return None;
        }
        Some(frame)
    }

    fn contains_position(self, position: Vec2) -> bool {
        position.is_finite()
            && position.x >= self.left
            && position.x < self.right()
            && position.y >= self.top
            && position.y < self.bottom()
    }
}

#[derive(Component, Default)]
struct HistoryGesture {
    input_at: Option<Instant>,
    gesture: Option<Gesture>,
}

struct Gesture {
    surface: Entity,
    frame: WindowedFrameRect,
    delta: Vec2,
    started_at: Instant,
    last_input: Instant,
    eligible_at: Option<Instant>,
    claimed: bool,
    ended: bool,
}

impl Gesture {
    fn record(&mut self, input: &NativeWheelInput) -> bool {
        if self.ended || !self.frame.contains_position(input.position) || !input.delta.is_finite() {
            return false;
        }
        self.last_input = input.at;
        if input.delta == Vec2::ZERO {
            return true;
        }
        if self.delta.x != 0.0
            && input.delta.x != 0.0
            && self.delta.x.signum() != input.delta.x.signum()
        {
            self.delta = Vec2::ZERO;
            self.eligible_at = None;
        }
        self.delta.x += input.delta.x;
        self.delta.y += input.delta.y.abs();
        if self.delta.x.abs() >= 200.0 && self.eligible_at.is_none() {
            self.eligible_at = Some(input.at);
        }
        self.delta.is_finite()
            && (self.delta.x.abs() + self.delta.y < 12.0
                || self.delta.y <= self.delta.x.abs() * 0.65)
    }

    fn matches(
        &self,
        surface: Entity,
        operation: NavigationOperation,
        at: Instant,
        processed_at: Instant,
    ) -> bool {
        if self.claimed
            || self.surface != surface
            || self.eligible_at.is_none_or(|eligible| at < eligible)
            || at.saturating_duration_since(self.last_input) > Duration::from_millis(300)
            || processed_at.saturating_duration_since(self.last_input) > Duration::from_millis(300)
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

#[derive(SystemParam)]
pub(crate) struct HistoryClaims<'w, 's> {
    regions:
        Query<'w, 's, (&'static mut HistoryRegion, &'static PaneFrame), With<vmux_layout::Browser>>,
    gestures: Query<'w, 's, (&'static Window, &'static mut HistoryGesture)>,
    hierarchy: vmux_layout::window::WindowHierarchy<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
    surfaces: vmux_layout::hosted_page::StackSurface<'w, 's>,
    blocked: Query<
        'w,
        's,
        (),
        Or<(
            With<bevy_cef::prelude::BinIpcBlocked>,
            With<vmux_layout::hosted_page::SurfaceBlocked>,
        )>,
    >,
}

impl HistoryClaims<'_, '_> {
    pub(crate) fn invalidate(&mut self, surface: Entity, at: Instant) {
        if let Ok((mut region, _)) = self.regions.get_mut(surface) {
            region.invalidated_at = Some(
                region
                    .invalidated_at
                    .map_or(at, |previous| previous.max(at)),
            );
        }
        for (_, mut state) in &mut self.gestures {
            if state
                .gesture
                .as_ref()
                .is_some_and(|gesture| gesture.surface == surface && gesture.started_at <= at)
            {
                state.gesture = None;
            }
        }
    }

    pub(crate) fn claim(
        &mut self,
        surface: Entity,
        operation: NavigationOperation,
        at: Instant,
    ) -> bool {
        let Ok((region, frame)) = self.regions.get(surface) else {
            return false;
        };
        let Ok(parent) = self.parents.get(surface) else {
            return false;
        };
        if self.blocked.contains(surface)
            || !self.surfaces.is_current(parent.parent(), surface)
            || self.hierarchy.get(surface) != Some(region.window)
        {
            return false;
        }
        let Ok((window, mut state)) = self.gestures.get_mut(region.window) else {
            return false;
        };
        let Some(gesture) = state.gesture.as_mut() else {
            return false;
        };
        if gesture.frame != region.frame
            || WindowedFrameRect::scaled(frame, window.resolution.scale_factor())
                != Some(region.frame)
            || region
                .invalidated_at
                .is_some_and(|invalidated| gesture.started_at <= invalidated)
            || !gesture.matches(surface, operation, at, Instant::now())
        {
            return false;
        }
        gesture.claimed = true;
        true
    }
}

fn initialize(
    windows: Query<Entity, (With<Window>, Without<HistoryGesture>)>,
    mut commands: Commands,
) {
    for window in &windows {
        commands.entity(window).insert(HistoryGesture::default());
    }
}

fn validate(
    regions: Query<&HistoryRegion, With<vmux_layout::Browser>>,
    mut gestures: Query<(Entity, &mut HistoryGesture)>,
) {
    for (window, mut state) in &mut gestures {
        let Some(gesture) = state.gesture.as_ref() else {
            continue;
        };
        if !regions
            .get(gesture.surface)
            .is_ok_and(|region| region.window == window && region.frame == gesture.frame)
        {
            state.gesture = None;
        }
    }
}

fn receive(
    mut inputs: MessageReader<NativeWheelInput>,
    regions: Query<(Entity, &HistoryRegion), With<vmux_layout::Browser>>,
    mut gestures: Query<&mut HistoryGesture, With<Window>>,
) {
    for input in inputs.read() {
        let Some(window) = input.window else {
            if matches!(input.phase, WheelPhase::Cancel | WheelPhase::Unsupported) {
                for mut state in &mut gestures {
                    if state.input_at.is_none_or(|at| input.at >= at) {
                        state.gesture = None;
                        state.input_at = Some(input.at);
                    }
                }
            }
            continue;
        };
        let Ok(mut state) = gestures.get_mut(window) else {
            continue;
        };
        if state.input_at.is_some_and(|at| input.at < at) {
            continue;
        }
        state.input_at = Some(input.at);
        if !input.has_native_event
            || !input.precise
            || input.modifiers.ctrl
            || input.modifiers.shift
            || input.modifiers.alt
            || input.modifiers.super_key
        {
            state.gesture = None;
            continue;
        }
        match input.phase {
            WheelPhase::Cancel | WheelPhase::Unsupported => {
                state.gesture = None;
                continue;
            }
            WheelPhase::Momentum => continue,
            WheelPhase::Begin => {
                let mut target = None;
                let mut ambiguous = false;
                for (surface, region) in &regions {
                    if !region.contains(window, input.position) {
                        continue;
                    }
                    if target.is_some() {
                        ambiguous = true;
                        break;
                    }
                    target = Some((surface, region));
                }
                state.gesture = None;
                if ambiguous {
                    continue;
                }
                let Some((surface, region)) = target else {
                    continue;
                };
                if region.invalidated_at.is_some_and(|at| input.at <= at) {
                    continue;
                }
                state.gesture = Some(Gesture {
                    surface,
                    frame: region.frame,
                    delta: Vec2::ZERO,
                    started_at: input.at,
                    last_input: input.at,
                    eligible_at: None,
                    claimed: false,
                    ended: false,
                });
            }
            WheelPhase::Changed => {}
            WheelPhase::End => {
                if let Some(gesture) = state.gesture.as_mut() {
                    if !gesture.frame.contains_position(input.position) {
                        state.gesture = None;
                        continue;
                    }
                    gesture.ended = true;
                }
                continue;
            }
        }
        if let Some(gesture) = state.gesture.as_mut()
            && !gesture.record(input)
        {
            state.gesture = None;
        }
    }
}

fn publish(
    pages: Query<(Entity, &PaneFrame), With<vmux_layout::Browser>>,
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
    mut regions: Query<(Entity, &mut HistoryRegion)>,
    mut commands: Commands,
) {
    let mut accepted = Vec::new();
    for (surface, frame) in &pages {
        if accepted.len() == 64 || blocked.contains(surface) {
            continue;
        }
        let Ok(parent) = parents.get(surface) else {
            continue;
        };
        if !surfaces.is_current(parent.parent(), surface) {
            continue;
        }
        let Some(window) = hierarchy.get(surface) else {
            continue;
        };
        let Ok(owner) = windows.get(window) else {
            continue;
        };
        let Some(frame) = WindowedFrameRect::scaled(frame, owner.resolution.scale_factor()) else {
            continue;
        };
        accepted.push(surface);
        match regions.get_mut(surface) {
            Ok((_, mut region)) => {
                if region.window != window || region.frame != frame {
                    region.window = window;
                    region.frame = frame;
                    region.invalidated_at = Some(Instant::now());
                }
            }
            Err(_) => {
                commands.entity(surface).insert(HistoryRegion {
                    window,
                    frame,
                    invalidated_at: None,
                });
            }
        }
    }
    for (surface, _) in &regions {
        if !accepted.contains(&surface) {
            commands.entity(surface).remove::<HistoryRegion>();
        }
    }
}

fn retire_surface(
    removed: On<Remove, vmux_layout::Browser>,
    mut gestures: Query<&mut HistoryGesture>,
    mut commands: Commands,
) {
    let surface = removed.event_target();
    commands.entity(surface).try_remove::<HistoryRegion>();
    for mut state in &mut gestures {
        if state
            .gesture
            .as_ref()
            .is_some_and(|gesture| gesture.surface == surface)
        {
            state.gesture = None;
        }
    }
}

fn retire_window(removed: On<Remove, Window>, mut commands: Commands) {
    commands
        .entity(removed.event_target())
        .try_remove::<HistoryGesture>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NavigationRequest;
    use bevy::prelude::{App, MessageReader, MessageWriter, Plugin};
    use vmux_ecs::PageMetadata;

    struct HistoryFixturePlugin;

    impl Plugin for HistoryFixturePlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(NativeHistoryPlugin)
                .add_message::<Claim>()
                .add_message::<Invalidate>()
                .add_message::<NavigationRequest>()
                .add_systems(Update, (invalidate, claim).chain());
        }
    }

    #[derive(Message)]
    struct Claim {
        surface: Entity,
        operation: NavigationOperation,
        at: Instant,
    }

    #[derive(Message)]
    struct Invalidate {
        surface: Entity,
        at: Instant,
    }

    fn claim(
        mut requests: MessageReader<Claim>,
        mut history: HistoryClaims,
        mut navigations: MessageWriter<NavigationRequest>,
    ) {
        for request in requests.read() {
            if history.claim(request.surface, request.operation, request.at) {
                navigations.write(NavigationRequest::surface(
                    request.surface,
                    request.operation,
                ));
            }
        }
    }

    fn invalidate(mut requests: MessageReader<Invalidate>, mut history: HistoryClaims) {
        for request in requests.read() {
            history.invalidate(request.surface, request.at);
        }
    }

    struct HistoryFixture {
        app: App,
        window: Entity,
        surface: Entity,
        requests: bevy::ecs::message::MessageCursor<NavigationRequest>,
    }

    impl HistoryFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, HistoryFixturePlugin));
            let window = app.world_mut().spawn(Window::default()).id();
            let stack = app
                .world_mut()
                .spawn(bevy_cef::prelude::HostWindow(window))
                .id();
            let surface = app
                .world_mut()
                .spawn((
                    vmux_layout::Browser,
                    PaneFrame {
                        left: 10.0,
                        top: 20.0,
                        width: 200.0,
                        height: 100.0,
                    },
                    ChildOf(stack),
                ))
                .id();
            app.world_mut()
                .entity_mut(stack)
                .insert(vmux_layout::page_slot::Current::new(
                    Some(surface),
                    PageMetadata::default(),
                ));
            app.update();
            Self {
                app,
                window,
                surface,
                requests: Default::default(),
            }
        }

        fn wheel(&mut self, phase: WheelPhase, delta: Vec2, at: Instant) {
            self.app.world_mut().write_message(NativeWheelInput {
                window: Some(self.window),
                position: Vec2::new(50.0, 50.0),
                delta,
                phase,
                at,
                has_native_event: true,
                precise: true,
                modifiers: Default::default(),
            });
        }

        fn invalidate(&mut self, at: Instant) {
            self.app.world_mut().write_message(Invalidate {
                surface: self.surface,
                at,
            });
        }

        fn claim(&mut self, operation: NavigationOperation, at: Instant) -> Vec<NavigationRequest> {
            self.claim_surface(self.surface, operation, at)
        }

        fn claim_surface(
            &mut self,
            surface: Entity,
            operation: NavigationOperation,
            at: Instant,
        ) -> Vec<NavigationRequest> {
            self.app.world_mut().write_message(Claim {
                surface,
                operation,
                at,
            });
            self.app.update();
            self.requests
                .read(self.app.world().resource::<Messages<NavigationRequest>>())
                .copied()
                .collect()
        }
    }

    #[test]
    fn axis_noise_preserves_one_horizontal_navigation_claim() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(0.0, 3.0), at);
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-120.0, 1.0),
            at + Duration::from_millis(16),
        );
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-120.0, 1.0),
            at + Duration::from_millis(32),
        );
        assert_eq!(
            fixture.claim(NavigationOperation::Back, at + Duration::from_millis(33)),
            vec![NavigationRequest::surface(
                fixture.surface,
                NavigationOperation::Back
            )]
        );
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(34))
                .is_empty()
        );
    }

    #[test]
    fn short_scrolls_and_wrong_direction_do_not_authorize_history() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-32.0, 0.0), at);
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-167.0, 0.0),
            at + Duration::from_millis(16),
        );
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(17))
                .is_empty()
        );
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-1.0, 0.0),
            at + Duration::from_millis(32),
        );
        assert!(
            fixture
                .claim(NavigationOperation::Forward, at + Duration::from_millis(33))
                .is_empty()
        );
        assert_eq!(
            fixture.claim(NavigationOperation::Back, at + Duration::from_millis(34)),
            vec![NavigationRequest::surface(
                fixture.surface,
                NavigationOperation::Back
            )]
        );
    }

    #[test]
    fn late_document_invalidation_cancels_only_gestures_started_before_it() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-1.0, 0.0),
            at + Duration::from_millis(32),
        );
        fixture.invalidate(at + Duration::from_millis(16));
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(33))
                .is_empty()
        );
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-240.0, 0.0),
            at + Duration::from_millis(48),
        );
        fixture.wheel(
            WheelPhase::Momentum,
            Vec2::new(-240.0, 0.0),
            at + Duration::from_millis(64),
        );
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(65))
                .is_empty()
        );
        fixture.wheel(
            WheelPhase::Begin,
            Vec2::new(-240.0, 0.0),
            at + Duration::from_millis(80),
        );
        fixture.invalidate(at + Duration::from_millis(72));
        assert_eq!(
            fixture.claim(NavigationOperation::Back, at + Duration::from_millis(81)),
            vec![NavigationRequest::surface(
                fixture.surface,
                NavigationOperation::Back
            )]
        );
    }

    #[test]
    fn interleaved_windows_have_independent_gestures_and_cancellation() {
        let mut fixture = HistoryFixture::new();
        let second_window = fixture.app.world_mut().spawn(Window::default()).id();
        let second_stack = fixture
            .app
            .world_mut()
            .spawn(bevy_cef::prelude::HostWindow(second_window))
            .id();
        let second_surface = fixture
            .app
            .world_mut()
            .spawn((
                vmux_layout::Browser,
                PaneFrame {
                    left: 10.0,
                    top: 20.0,
                    width: 200.0,
                    height: 100.0,
                },
                ChildOf(second_stack),
            ))
            .id();
        fixture.app.world_mut().entity_mut(second_stack).insert(
            vmux_layout::page_slot::Current::new(Some(second_surface), PageMetadata::default()),
        );
        fixture.app.update();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        fixture.app.world_mut().write_message(NativeWheelInput {
            window: Some(second_window),
            position: Vec2::new(50.0, 50.0),
            delta: Vec2::new(240.0, 0.0),
            phase: WheelPhase::Begin,
            at,
            has_native_event: true,
            precise: true,
            modifiers: Default::default(),
        });
        fixture.wheel(
            WheelPhase::Cancel,
            Vec2::ZERO,
            at + Duration::from_millis(1),
        );
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(2))
                .is_empty()
        );
        assert_eq!(
            fixture.claim_surface(
                second_surface,
                NavigationOperation::Forward,
                at + Duration::from_millis(2)
            ),
            vec![NavigationRequest::surface(
                second_surface,
                NavigationOperation::Forward
            )]
        );
    }

    #[test]
    fn changed_geometry_and_retired_owners_refuse_stale_claims() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .get_mut::<PaneFrame>(fixture.surface)
            .unwrap()
            .width = 300.0;
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(1))
                .is_empty()
        );
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), Instant::now());
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.surface)
            .remove::<vmux_layout::Browser>();
        assert!(
            fixture
                .claim(NavigationOperation::Back, Instant::now())
                .is_empty()
        );
    }

    #[test]
    fn stale_or_unsupported_samples_cannot_authorize_navigation() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(301))
                .is_empty()
        );
        for phase in [WheelPhase::Cancel, WheelPhase::Unsupported] {
            fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
            fixture.wheel(phase, Vec2::ZERO, at);
            assert!(fixture.claim(NavigationOperation::Back, at).is_empty());
        }
        fixture.wheel(WheelPhase::Begin, Vec2::new(f32::NAN, 0.0), at);
        assert!(fixture.claim(NavigationOperation::Back, at).is_empty());
    }

    #[test]
    fn raw_origin_precision_and_modifiers_are_checked_before_history_capture() {
        for refusal in 0..6 {
            let mut fixture = HistoryFixture::new();
            let at = Instant::now();
            let mut input = NativeWheelInput {
                window: Some(fixture.window),
                position: Vec2::new(50.0, 50.0),
                delta: Vec2::new(-240.0, 0.0),
                phase: WheelPhase::Begin,
                at,
                has_native_event: true,
                precise: true,
                modifiers: Default::default(),
            };
            match refusal {
                0 => input.has_native_event = false,
                1 => input.precise = false,
                2 => input.modifiers.ctrl = true,
                3 => input.modifiers.alt = true,
                4 => input.modifiers.shift = true,
                5 => input.modifiers.super_key = true,
                _ => unreachable!(),
            }
            fixture.app.world_mut().write_message(input);
            assert!(fixture.claim(NavigationOperation::Back, at).is_empty());
            let fresh = Instant::now();
            fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), fresh);
            assert_eq!(
                fixture.claim(NavigationOperation::Back, fresh),
                vec![NavigationRequest::surface(
                    fixture.surface,
                    NavigationOperation::Back
                )]
            );
        }
    }

    #[test]
    fn buffered_requests_expire_when_native_input_is_no_longer_recent() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now() - Duration::from_secs(1);
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(16))
                .is_empty()
        );
    }

    #[test]
    fn requests_observed_before_physical_eligibility_do_not_consume_that_gesture() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-40.0, 0.0), at);
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-180.0, 0.0),
            at + Duration::from_millis(32),
        );
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(16))
                .is_empty()
        );
        assert_eq!(
            fixture.claim(NavigationOperation::Back, at + Duration::from_millis(33)),
            vec![NavigationRequest::surface(
                fixture.surface,
                NavigationOperation::Back
            )]
        );
    }

    #[test]
    fn later_samples_of_the_same_gesture_preserve_an_already_observed_request() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-220.0, 0.0), at);
        fixture.wheel(
            WheelPhase::Changed,
            Vec2::new(-20.0, 0.0),
            at + Duration::from_millis(32),
        );
        assert_eq!(
            fixture.claim(NavigationOperation::Back, at + Duration::from_millis(16)),
            vec![NavigationRequest::surface(
                fixture.surface,
                NavigationOperation::Back
            )]
        );
    }

    #[test]
    fn late_input_and_old_claims_cannot_rearm_a_canceled_or_newer_gesture() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        fixture.wheel(
            WheelPhase::Cancel,
            Vec2::ZERO,
            at + Duration::from_millis(32),
        );
        fixture.wheel(
            WheelPhase::Begin,
            Vec2::new(-240.0, 0.0),
            at + Duration::from_millis(16),
        );
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(33))
                .is_empty()
        );
        fixture.wheel(
            WheelPhase::Begin,
            Vec2::new(-240.0, 0.0),
            at + Duration::from_millis(48),
        );
        assert!(
            fixture
                .claim(NavigationOperation::Back, at + Duration::from_millis(40))
                .is_empty()
        );
        assert_eq!(
            fixture.claim(NavigationOperation::Back, at + Duration::from_millis(49)),
            vec![NavigationRequest::surface(
                fixture.surface,
                NavigationOperation::Back
            )]
        );
    }

    #[test]
    fn current_surface_window_and_blocking_changes_refuse_claims_before_publication() {
        for change in 0..3 {
            let mut fixture = HistoryFixture::new();
            let at = Instant::now();
            fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
            fixture.app.update();
            let stack = fixture
                .app
                .world()
                .get::<ChildOf>(fixture.surface)
                .unwrap()
                .parent();
            match change {
                0 => {
                    fixture.app.world_mut().entity_mut(stack).insert(
                        vmux_layout::page_slot::Current::new(None, PageMetadata::default()),
                    );
                }
                1 => {
                    let other = fixture.app.world_mut().spawn(Window::default()).id();
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(stack)
                        .insert(bevy_cef::prelude::HostWindow(other));
                }
                2 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.surface)
                        .insert(vmux_layout::hosted_page::SurfaceBlocked);
                }
                _ => unreachable!(),
            }
            assert!(
                fixture
                    .claim(NavigationOperation::Back, at + Duration::from_millis(1))
                    .is_empty()
            );
        }
    }

    #[test]
    fn overlapping_current_surfaces_cannot_choose_a_history_owner() {
        let mut fixture = HistoryFixture::new();
        let stack = fixture
            .app
            .world_mut()
            .spawn(bevy_cef::prelude::HostWindow(fixture.window))
            .id();
        let other = fixture
            .app
            .world_mut()
            .spawn((
                vmux_layout::Browser,
                PaneFrame {
                    left: 10.0,
                    top: 20.0,
                    width: 200.0,
                    height: 100.0,
                },
                ChildOf(stack),
            ))
            .id();
        fixture
            .app
            .world_mut()
            .entity_mut(stack)
            .insert(vmux_layout::page_slot::Current::new(
                Some(other),
                PageMetadata::default(),
            ));
        fixture.app.update();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        assert!(fixture.claim(NavigationOperation::Back, at).is_empty());
        assert!(
            fixture
                .claim_surface(other, NavigationOperation::Back, at)
                .is_empty()
        );
    }

    #[test]
    fn native_queue_loss_invalidates_already_accumulated_input() {
        let mut fixture = HistoryFixture::new();
        let at = Instant::now();
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), at);
        fixture.app.update();
        let inbox = fixture
            .app
            .world()
            .resource::<vmux_input::WheelInbox>()
            .clone();
        for _ in 0..257 {
            inbox.record(NativeWheelInput {
                window: Some(fixture.window),
                position: Vec2::new(50.0, 50.0),
                delta: Vec2::ZERO,
                phase: WheelPhase::Changed,
                at,
                has_native_event: true,
                precise: true,
                modifiers: Default::default(),
            });
        }
        assert!(fixture.claim(NavigationOperation::Back, at).is_empty());
        fixture.wheel(WheelPhase::Begin, Vec2::new(-240.0, 0.0), Instant::now());
        assert_eq!(
            fixture.claim(NavigationOperation::Back, Instant::now()),
            vec![NavigationRequest::surface(
                fixture.surface,
                NavigationOperation::Back
            )]
        );
    }
}
