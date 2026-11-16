use bevy::ecs::system::SystemParam;
use bevy::prelude::{
    Add, App, ChildOf, Commands, Component, Entity, EntityEvent, Has, IntoScheduleConfigs, Message,
    MessageReader, MessageWriter, Name, NonSend, On, Plugin, PostUpdate, Query, Remove, Res,
    SystemSet, Vec2, Window, With, Without,
};
use bevy::window::WindowResized;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use bevy_cef::prelude::{Browsers, HostWindow, WebviewSize};
use vmux_ecs::overlay::WindowOverlay;
use vmux_layout::Browser;
use vmux_layout::window::FocusedWindow;

pub(crate) struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<WindowResized>()
            .add_message::<ResizeRequested>()
            .add_message::<ResizeCompleted>()
            .add_message::<WakeRequested>()
            .add_systems(
                PostUpdate,
                (initialize, observe, request, apply, confirm, wake)
                    .chain()
                    .in_set(ViewportSet)
                    .after(crate::present::RepaintSet),
            )
            .add_observer(retire::<Browser>)
            .add_observer(retire::<WebviewSize>)
            .add_observer(retire_overlay);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ViewportSet;

#[derive(Component, Default)]
struct Receipt {
    applied: Option<(Vec2, f32, Option<Entity>)>,
    pending: Option<Entity>,
    waiting: bool,
}

#[derive(Component, Default)]
struct Available(bool);

#[derive(Component, Clone, Copy)]
struct Resize {
    size: Vec2,
    scale: f32,
    window: Option<Entity>,
}

impl Resize {
    fn matches(&self, previous: (Vec2, f32, Option<Entity>)) -> bool {
        self.size == previous.0
            && (self.scale - previous.1).abs() < 0.01
            && self.window == previous.2
    }

    fn snapshot(self) -> (Vec2, f32, Option<Entity>) {
        (self.size, self.scale, self.window)
    }
}

#[derive(SystemParam)]
struct Windows<'w, 's> {
    hosts: Query<'w, 's, &'static HostWindow>,
    windows: Query<'w, 's, &'static Window>,
    focused: FocusedWindow<'w, 's>,
}

impl Windows<'_, '_> {
    fn target(&self, owner: Entity, size: Vec2) -> Resize {
        let window = self
            .hosts
            .get(owner)
            .ok()
            .map(|host| host.0)
            .or(self.focused.entity());
        let scale = window
            .and_then(|entity| self.windows.get(entity).ok())
            .map(|window| window.resolution.scale_factor())
            .filter(|scale| scale.is_finite() && *scale > 0.0)
            .unwrap_or(1.0);
        Resize {
            size,
            scale,
            window,
        }
    }
}

#[derive(Message)]
struct ResizeRequested(Entity);

#[derive(Message)]
struct ResizeCompleted {
    attempt: Entity,
    submitted: bool,
}

#[derive(Message)]
struct WakeRequested;

fn initialize(
    owners: Query<
        Entity,
        (
            With<Browser>,
            With<WebviewSize>,
            Without<WindowOverlay>,
            Without<Receipt>,
        ),
    >,
    mut commands: Commands,
) {
    for owner in &owners {
        commands
            .entity(owner)
            .insert((Receipt::default(), Available::default()));
    }
}

fn observe(
    browsers: Option<NonSend<Browsers>>,
    mut owners: Query<(Entity, &mut Available), (With<Browser>, Without<WindowOverlay>)>,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for (owner, mut available) in &mut owners {
        let present = browsers.has_browser(owner);
        if available.0 != present {
            available.0 = present;
        }
    }
}

fn request(
    mut owners: Query<
        (Entity, &WebviewSize, &Available, &mut Receipt),
        (With<Browser>, Without<WindowOverlay>),
    >,
    attempts: Query<&Resize>,
    windows: Windows,
    mut resized: MessageReader<WindowResized>,
    mut requests: MessageWriter<ResizeRequested>,
    mut wakes: MessageWriter<WakeRequested>,
    mut commands: Commands,
) {
    let force = resized.read().count() > 0;
    let mut newly_waiting = false;
    for (owner, size, available, mut receipt) in &mut owners {
        if force {
            receipt.applied = None;
            if let Some(attempt) = receipt.pending {
                receipt.pending = None;
                commands.entity(attempt).try_remove::<vmux_ecs::Active>();
            }
        }
        if !available.0 {
            newly_waiting |= !receipt.waiting;
            if !receipt.waiting {
                receipt.waiting = true;
            }
            if receipt.applied.is_some() {
                receipt.applied = None;
            }
            if let Some(attempt) = receipt.pending {
                receipt.pending = None;
                commands.entity(attempt).try_remove::<vmux_ecs::Active>();
            }
            continue;
        }
        if receipt.waiting {
            receipt.waiting = false;
        }
        let target = windows.target(owner, size.0);
        if receipt
            .applied
            .is_some_and(|previous| target.matches(previous))
        {
            continue;
        }
        if let Some(attempt) = receipt.pending {
            if attempts
                .get(attempt)
                .is_ok_and(|previous| target.matches(previous.snapshot()))
            {
                continue;
            }
            receipt.pending = None;
            commands.entity(attempt).try_remove::<vmux_ecs::Active>();
        }
        let attempt = commands
            .spawn((
                Name::new("Viewport resize"),
                target,
                vmux_ecs::Active,
                ChildOf(owner),
            ))
            .id();
        receipt.pending = Some(attempt);
        requests.write(ResizeRequested(attempt));
    }
    if newly_waiting {
        wakes.write(WakeRequested);
    }
}

fn apply(
    browsers: Option<NonSend<Browsers>>,
    mut requests: MessageReader<ResizeRequested>,
    attempts: Query<(&Resize, &ChildOf, Has<vmux_ecs::Active>)>,
    mut completed: MessageWriter<ResizeCompleted>,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for request in requests.read() {
        let Ok((target, parent, active)) = attempts.get(request.0) else {
            continue;
        };
        let owner = parent.parent();
        let submitted = active && browsers.has_browser(owner);
        if submitted {
            browsers.resize(&owner, target.size, target.scale);
        }
        completed.write(ResizeCompleted {
            attempt: request.0,
            submitted,
        });
    }
}

fn confirm(
    mut completed: MessageReader<ResizeCompleted>,
    attempts: Query<(&Resize, &ChildOf)>,
    mut owners: Query<
        (&WebviewSize, &Available, &mut Receipt),
        (With<Browser>, Without<WindowOverlay>),
    >,
    windows: Windows,
    mut wakes: MessageWriter<WakeRequested>,
    mut commands: Commands,
) {
    let mut submitted_any = false;
    for event in completed.read() {
        let Ok((target, parent)) = attempts.get(event.attempt) else {
            continue;
        };
        submitted_any |= event.submitted;
        commands.entity(event.attempt).try_despawn();
        let owner = parent.parent();
        let Ok((size, available, mut receipt)) = owners.get_mut(owner) else {
            continue;
        };
        if receipt.pending != Some(event.attempt) {
            continue;
        }
        receipt.pending = None;
        if event.submitted
            && available.0
            && windows.target(owner, size.0).matches(target.snapshot())
        {
            receipt.applied = Some(target.snapshot());
        }
    }
    if submitted_any {
        wakes.write(WakeRequested);
    }
}

fn wake(mut requests: MessageReader<WakeRequested>, proxy: Option<Res<EventLoopProxyWrapper>>) {
    if requests.read().count() == 0 {
        return;
    }
    if let Some(proxy) = proxy {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

fn retire<T: Component>(
    removed: On<Remove, T>,
    mut owners: Query<&mut Receipt>,
    mut commands: Commands,
) {
    let owner = removed.event_target();
    if let Ok(mut receipt) = owners.get_mut(owner)
        && let Some(attempt) = receipt.pending
    {
        receipt.pending = None;
        commands.entity(attempt).try_remove::<vmux_ecs::Active>();
    }
    commands.entity(owner).try_remove::<(Receipt, Available)>();
}

fn retire_overlay(
    added: On<Add, WindowOverlay>,
    mut owners: Query<&mut Receipt>,
    mut commands: Commands,
) {
    let owner = added.event_target();
    if let Ok(mut receipt) = owners.get_mut(owner)
        && let Some(attempt) = receipt.pending
    {
        receipt.pending = None;
        commands.entity(attempt).try_remove::<vmux_ecs::Active>();
    }
    commands.entity(owner).try_remove::<(Receipt, Available)>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::MessageCursor;
    use bevy::prelude::{Messages, MinimalPlugins, ResMut};

    struct ViewportFixture {
        app: App,
        wakes: MessageCursor<WakeRequested>,
        window: Entity,
        owner: Entity,
    }

    impl ViewportFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ViewportPlugin))
                .insert_resource(NativeFixture {
                    calls: Vec::new(),
                    withheld: Vec::new(),
                    submitted: true,
                    hold: false,
                })
                .add_systems(
                    PostUpdate,
                    (
                        observe_fixture.after(observe).before(request),
                        respond.after(apply).before(confirm),
                    ),
                );
            let window = app
                .world_mut()
                .spawn((Window::default(), vmux_ecs::Active))
                .id();
            let owner = app
                .world_mut()
                .spawn((
                    Browser,
                    WebviewSize(Vec2::new(640.0, 480.0)),
                    HostWindow(window),
                    NativeReady(true),
                ))
                .id();
            Self {
                app,
                wakes: MessageCursor::default(),
                window,
                owner,
            }
        }

        fn step(&mut self) -> (Vec<(Entity, Vec2, f32)>, bool) {
            self.app.update();
            let calls =
                std::mem::take(&mut self.app.world_mut().resource_mut::<NativeFixture>().calls);
            let wake = self
                .wakes
                .read(self.app.world().resource::<Messages<WakeRequested>>())
                .count()
                > 0;
            (calls, wake)
        }

        fn ready(&mut self, ready: bool) {
            self.app
                .world_mut()
                .get_mut::<NativeReady>(self.owner)
                .unwrap()
                .0 = ready;
        }

        fn scale(&mut self, scale: f32) {
            self.app
                .world_mut()
                .get_mut::<Window>(self.window)
                .unwrap()
                .resolution
                .set_scale_factor_override(Some(scale));
        }

        fn complete(&mut self, attempt: Entity, submitted: bool) {
            self.app
                .world_mut()
                .write_message(ResizeCompleted { attempt, submitted });
        }
    }

    #[derive(Component)]
    struct NativeReady(bool);

    #[derive(bevy::prelude::Resource)]
    struct NativeFixture {
        calls: Vec<(Entity, Vec2, f32)>,
        withheld: Vec<Entity>,
        submitted: bool,
        hold: bool,
    }

    fn observe_fixture(mut owners: Query<(&NativeReady, &mut Available)>) {
        for (native, mut available) in &mut owners {
            if available.0 != native.0 {
                available.0 = native.0;
            }
        }
    }

    fn respond(
        mut requested: MessageReader<ResizeRequested>,
        attempts: Query<(&Resize, &ChildOf, Has<vmux_ecs::Active>)>,
        mut native: ResMut<NativeFixture>,
        mut completed: MessageWriter<ResizeCompleted>,
    ) {
        for request in requested.read() {
            let Ok((target, parent, active)) = attempts.get(request.0) else {
                continue;
            };
            if active {
                native
                    .calls
                    .push((parent.parent(), target.size, target.scale));
            }
            if native.hold {
                native.withheld.push(request.0);
                continue;
            }
            completed.write(ResizeCompleted {
                attempt: request.0,
                submitted: active && native.submitted,
            });
        }
    }

    #[test]
    fn stable_native_sizes_do_not_repeat_resize_or_wake() {
        let mut fixture = ViewportFixture::new();
        let (calls, wake) = fixture.step();
        assert_eq!(calls, [(fixture.owner, Vec2::new(640.0, 480.0), 1.0)]);
        assert!(wake);
        assert_eq!(fixture.step(), (Vec::new(), false));
    }

    #[test]
    fn missing_native_browsers_wake_once_and_resize_when_created() {
        let mut fixture = ViewportFixture::new();
        fixture.ready(false);
        assert_eq!(fixture.step(), (Vec::new(), true));
        assert_eq!(fixture.step(), (Vec::new(), false));
        fixture.ready(true);
        assert_eq!(fixture.step().0.len(), 1);
        assert_eq!(fixture.step(), (Vec::new(), false));
        fixture.ready(false);
        assert_eq!(fixture.step(), (Vec::new(), true));
        fixture.ready(true);
        assert_eq!(fixture.step().0.len(), 1);
    }

    #[test]
    fn unavailable_submission_is_retried_without_acknowledging_success() {
        let mut fixture = ViewportFixture::new();
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .submitted = false;
        assert_eq!(fixture.step().0.len(), 1);
        assert_eq!(fixture.step().0.len(), 1);
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .submitted = true;
        assert_eq!(fixture.step().0.len(), 1);
        assert_eq!(fixture.step(), (Vec::new(), false));
    }

    #[test]
    fn resize_uses_current_dimensions_and_host_scale() {
        let mut fixture = ViewportFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .get_mut::<WebviewSize>(fixture.owner)
            .unwrap()
            .0 = Vec2::new(800.0, 600.0);
        fixture.scale(2.0);
        assert_eq!(
            fixture.step().0,
            [(fixture.owner, Vec2::new(800.0, 600.0), 2.0)]
        );
    }

    #[test]
    fn scale_noise_below_the_native_tolerance_does_not_resize() {
        let mut fixture = ViewportFixture::new();
        fixture.scale(2.0);
        fixture.step();
        fixture.scale(2.005);
        assert_eq!(fixture.step(), (Vec::new(), false));
        fixture.scale(2.02);
        assert_eq!(fixture.step().0.len(), 1);
    }

    #[test]
    fn moving_to_another_host_resizes_even_with_identical_geometry() {
        let mut fixture = ViewportFixture::new();
        fixture.step();
        let window = fixture.app.world_mut().spawn(Window::default()).id();
        fixture
            .app
            .world_mut()
            .get_mut::<HostWindow>(fixture.owner)
            .unwrap()
            .0 = window;
        assert_eq!(fixture.step().0.len(), 1);
        assert_eq!(fixture.step(), (Vec::new(), false));
    }

    #[test]
    fn focused_window_fallback_rebinds_with_its_host() {
        let mut fixture = ViewportFixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<HostWindow>();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.window)
            .remove::<vmux_ecs::Active>();
        fixture
            .app
            .world_mut()
            .spawn((Window::default(), vmux_ecs::Active));
        assert_eq!(fixture.step().0.len(), 1);
    }

    #[test]
    fn window_resize_refreshes_all_current_browser_sizes() {
        let mut fixture = ViewportFixture::new();
        let other = fixture
            .app
            .world_mut()
            .spawn((
                Browser,
                WebviewSize(Vec2::new(320.0, 240.0)),
                HostWindow(fixture.window),
                NativeReady(true),
            ))
            .id();
        assert_eq!(fixture.step().0.len(), 2);
        assert_eq!(fixture.step(), (Vec::new(), false));
        fixture.app.world_mut().write_message(WindowResized {
            window: fixture.window,
            width: 1200.0,
            height: 900.0,
        });
        let mut owners: Vec<_> = fixture.step().0.into_iter().map(|call| call.0).collect();
        owners.sort();
        let mut expected = vec![fixture.owner, other];
        expected.sort();
        assert_eq!(owners, expected);
    }

    #[test]
    fn browser_reattachment_replays_identical_native_size() {
        let mut fixture = ViewportFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<Browser>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(Browser);
        assert_eq!(fixture.step().0.len(), 1);
    }

    #[test]
    fn size_reattachment_replays_identical_native_size() {
        let mut fixture = ViewportFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<WebviewSize>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(WebviewSize(Vec2::new(640.0, 480.0)));
        assert_eq!(fixture.step().0.len(), 1);
    }

    #[test]
    fn late_completion_cannot_acknowledge_a_reattached_browser() {
        let mut fixture = ViewportFixture::new();
        fixture.app.world_mut().resource_mut::<NativeFixture>().hold = true;
        fixture.step();
        let old = fixture.app.world().resource::<NativeFixture>().withheld[0];
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<Browser>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(Browser);
        {
            let mut native = fixture.app.world_mut().resource_mut::<NativeFixture>();
            native.hold = false;
            native.submitted = false;
        }
        fixture.complete(old, true);
        assert_eq!(fixture.step().0.len(), 1);
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .submitted = true;
        assert_eq!(fixture.step().0.len(), 1);
        assert_eq!(fixture.step(), (Vec::new(), false));
    }

    #[test]
    fn completion_for_old_dimensions_cannot_suppress_current_resize() {
        let mut fixture = ViewportFixture::new();
        fixture.app.world_mut().resource_mut::<NativeFixture>().hold = true;
        fixture.step();
        let old = fixture.app.world().resource::<NativeFixture>().withheld[0];
        fixture
            .app
            .world_mut()
            .get_mut::<WebviewSize>(fixture.owner)
            .unwrap()
            .0 = Vec2::new(800.0, 600.0);
        {
            let mut native = fixture.app.world_mut().resource_mut::<NativeFixture>();
            native.hold = false;
            native.submitted = false;
        }
        fixture.complete(old, true);
        assert_eq!(
            fixture.step().0,
            [(fixture.owner, Vec2::new(800.0, 600.0), 1.0)]
        );
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .submitted = true;
        assert_eq!(fixture.step().0.len(), 1);
    }

    #[test]
    fn overlays_leave_resize_ownership_and_return_with_fresh_history() {
        let mut fixture = ViewportFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(WindowOverlay);
        assert_eq!(fixture.step(), (Vec::new(), false));
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<WindowOverlay>();
        assert_eq!(fixture.step().0.len(), 1);
    }

    #[test]
    fn retired_owner_completions_do_not_resize_a_replacement_entity() {
        let mut fixture = ViewportFixture::new();
        fixture.app.world_mut().resource_mut::<NativeFixture>().hold = true;
        fixture.step();
        let old = fixture.app.world().resource::<NativeFixture>().withheld[0];
        fixture.app.world_mut().entity_mut(fixture.owner).despawn();
        let replacement = fixture
            .app
            .world_mut()
            .spawn((
                Browser,
                WebviewSize(Vec2::new(320.0, 240.0)),
                NativeReady(true),
            ))
            .id();
        fixture.app.world_mut().resource_mut::<NativeFixture>().hold = false;
        fixture.complete(old, true);
        assert_eq!(
            fixture.step().0,
            [(replacement, Vec2::new(320.0, 240.0), 1.0)]
        );
    }
}
