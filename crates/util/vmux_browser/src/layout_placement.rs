use bevy::ecs::system::{NonSendMarker, SystemParam};
use bevy::prelude::{
    App, ChildOf, Commands, Component, Entity, EntityEvent, Has, IntoScheduleConfigs, Message,
    MessageReader, MessageWriter, Name, NonSend, On, Plugin, PostUpdate, Query, Remove, SystemSet,
    Vec2, Window, With, Without,
};
#[cfg(target_os = "macos")]
use bevy::winit::WINIT_WINDOWS;
use bevy_cef::prelude::{BinIpcSurfaceGeneration, Browsers, HostWindow, WebviewWindowed};
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use vmux_layout::LayoutCef;
use vmux_layout::window::FocusedWindow;

pub(crate) struct LayoutPlacementPlugin;

impl Plugin for LayoutPlacementPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlacementRequested>()
            .add_message::<WindowMatched>()
            .add_message::<FrameRequested>()
            .add_message::<PlacementCompleted>()
            .add_message::<RaiseRequested>()
            .add_message::<RaiseCompleted>()
            .add_systems(
                PostUpdate,
                (
                    initialize,
                    select,
                    match_window,
                    place,
                    apply,
                    confirm,
                    raise,
                    finish,
                )
                    .chain()
                    .in_set(PlacementSet)
                    .after(crate::present::LayoutChildrenSet)
                    .before(crate::BrowserSystemSet::SyncWindowedFrames),
            )
            .add_observer(retire::<LayoutCef>)
            .add_observer(retire::<WebviewWindowed>)
            .add_observer(retire_window);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PlacementSet;

#[derive(Component, Default)]
struct History {
    raised: Option<(Entity, Vec2, f32, u64)>,
    pending: Option<Entity>,
}

#[derive(Component, Clone, Copy)]
struct Placement {
    window: Entity,
    size: Vec2,
    scale: f32,
    generation: u64,
}

impl Placement {
    fn geometry(self) -> (Entity, Vec2, f32, u64) {
        (self.window, self.size, self.scale, self.generation)
    }

    fn matches(self, previous: (Entity, Vec2, f32, u64)) -> bool {
        self.geometry() == previous
    }
}

#[derive(SystemParam)]
struct Windows<'w, 's> {
    hosts: Query<'w, 's, &'static HostWindow>,
    generations: Query<'w, 's, &'static BinIpcSurfaceGeneration>,
    windows: Query<'w, 's, &'static Window>,
    focused: FocusedWindow<'w, 's>,
}

impl Windows<'_, '_> {
    fn target(&self, owner: Entity) -> Option<Placement> {
        let window = self
            .hosts
            .get(owner)
            .ok()
            .map(|host| host.0)
            .or(self.focused.entity())?;
        let native = self.windows.get(window).ok()?;
        let size = Vec2::new(
            native.resolution.physical_width() as f32,
            native.resolution.physical_height() as f32,
        );
        let scale = native.resolution.scale_factor();
        if size.x <= 0.0 || size.y <= 0.0 || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        Some(Placement {
            window,
            size,
            scale,
            generation: self
                .generations
                .get(owner)
                .map_or(0, |generation| generation.0),
        })
    }
}

#[derive(SystemParam)]
struct NativeWindows {
    _thread: NonSendMarker,
}

impl NativeWindows {
    #[cfg(target_os = "macos")]
    fn matches(&self, browsers: &Browsers, owner: Entity, host: Entity) -> bool {
        WINIT_WINDOWS.with_borrow(|windows| {
            let Some(window) = windows.get_window(host) else {
                return false;
            };
            let Ok(handle) = window.window_handle() else {
                return false;
            };
            let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
                return false;
            };
            let view: &objc2_app_kit::NSView = unsafe { &*handle.ns_view.as_ptr().cast() };
            let Some(window) = view.window() else {
                return false;
            };
            let identity = (&*window as *const objc2_app_kit::NSWindow) as isize;
            browsers.windowed_view_matches_window(&owner, identity)
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn matches(&self, _: &Browsers, _: Entity, _: Entity) -> bool {
        false
    }
}

#[derive(Message)]
struct WindowMatched {
    attempt: Entity,
    matched: bool,
}

#[derive(Message)]
struct FrameRequested(Entity);

#[derive(Message)]
struct PlacementRequested(Entity);

#[derive(Message)]
struct PlacementCompleted {
    attempt: Entity,
    ready: bool,
}

#[derive(Message)]
struct RaiseRequested(Entity);

#[derive(Message)]
struct RaiseCompleted {
    attempt: Entity,
    raised: bool,
}

fn initialize(
    owners: Query<Entity, (With<LayoutCef>, With<WebviewWindowed>, Without<History>)>,
    mut commands: Commands,
) {
    for owner in &owners {
        commands.entity(owner).insert(History::default());
    }
}

fn select(
    mut owners: Query<(Entity, &mut History), (With<LayoutCef>, With<WebviewWindowed>)>,
    windows: Windows,
    mut requested: MessageWriter<PlacementRequested>,
    mut commands: Commands,
) {
    for (owner, mut history) in &mut owners {
        if let Some(attempt) = history.pending.take() {
            commands.entity(attempt).try_despawn();
        }
        let Some(target) = windows.target(owner) else {
            if history.raised.is_some() {
                history.raised = None;
            }
            continue;
        };
        let attempt = commands
            .spawn((
                Name::new("Layout native placement"),
                target,
                vmux_ecs::Active,
                ChildOf(owner),
            ))
            .id();
        history.pending = Some(attempt);
        requested.write(PlacementRequested(attempt));
    }
}

fn match_window(
    browsers: Option<NonSend<Browsers>>,
    native: NativeWindows,
    mut requested: MessageReader<PlacementRequested>,
    attempts: Query<(&Placement, &ChildOf, Has<vmux_ecs::Active>)>,
    mut matched: MessageWriter<WindowMatched>,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for request in requested.read() {
        let Ok((target, parent, active)) = attempts.get(request.0) else {
            continue;
        };
        matched.write(WindowMatched {
            attempt: request.0,
            matched: active && native.matches(&browsers, parent.parent(), target.window),
        });
    }
}

fn place(
    mut matched: MessageReader<WindowMatched>,
    attempts: Query<(&Placement, &ChildOf)>,
    mut owners: Query<&mut History, (With<LayoutCef>, With<WebviewWindowed>)>,
    windows: Windows,
    mut requested: MessageWriter<FrameRequested>,
    mut commands: Commands,
) {
    for event in matched.read() {
        let Ok((target, parent)) = attempts.get(event.attempt) else {
            continue;
        };
        let owner = parent.parent();
        let Ok(mut history) = owners.get_mut(owner) else {
            commands.entity(event.attempt).try_despawn();
            continue;
        };
        if history.pending != Some(event.attempt) {
            commands.entity(event.attempt).try_despawn();
            continue;
        }
        if !event.matched
            || windows
                .target(owner)
                .is_none_or(|current| !current.matches(target.geometry()))
        {
            history.pending = None;
            history.raised = None;
            commands.entity(event.attempt).try_despawn();
            continue;
        }
        requested.write(FrameRequested(event.attempt));
    }
}

fn apply(
    browsers: Option<NonSend<Browsers>>,
    native: NativeWindows,
    windows: Windows,
    mut requested: MessageReader<FrameRequested>,
    attempts: Query<(&Placement, &ChildOf, Has<vmux_ecs::Active>)>,
    mut completed: MessageWriter<PlacementCompleted>,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for request in requested.read() {
        let Ok((target, parent, active)) = attempts.get(request.0) else {
            continue;
        };
        let owner = parent.parent();
        let current = windows.target(owner);
        let valid = active && current.is_some_and(|current| current.matches(target.geometry()));
        let bound = valid && native.matches(&browsers, owner, target.window);
        if bound {
            browsers.set_windowed_hidden(&owner, false);
            browsers.set_windowed_frame(
                &owner,
                0.0,
                0.0,
                target.size.x,
                target.size.y,
                target.scale,
            );
        }
        completed.write(PlacementCompleted {
            attempt: request.0,
            ready: bound
                && native.matches(&browsers, owner, target.window)
                && browsers.windowed_view_ready(&owner),
        });
    }
}

fn confirm(
    mut completed: MessageReader<PlacementCompleted>,
    attempts: Query<(&Placement, &ChildOf)>,
    mut owners: Query<&mut History, (With<LayoutCef>, With<WebviewWindowed>)>,
    windows: Windows,
    mut requested: MessageWriter<RaiseRequested>,
    mut commands: Commands,
) {
    for event in completed.read() {
        let Ok((target, parent)) = attempts.get(event.attempt) else {
            continue;
        };
        let owner = parent.parent();
        let Ok(mut history) = owners.get_mut(owner) else {
            commands.entity(event.attempt).try_despawn();
            continue;
        };
        if history.pending != Some(event.attempt) {
            commands.entity(event.attempt).try_despawn();
            continue;
        }
        let current = windows.target(owner);
        if current.is_none_or(|current| !current.matches(target.geometry())) {
            history.pending = None;
            commands.entity(event.attempt).try_despawn();
            continue;
        }
        if !event.ready
            || history
                .raised
                .is_some_and(|previous| target.matches(previous))
        {
            if !event.ready {
                history.raised = None;
            }
            history.pending = None;
            commands.entity(event.attempt).try_despawn();
            continue;
        }
        requested.write(RaiseRequested(event.attempt));
    }
}

fn raise(
    browsers: Option<NonSend<Browsers>>,
    native: NativeWindows,
    windows: Windows,
    mut requested: MessageReader<RaiseRequested>,
    attempts: Query<(&Placement, &ChildOf, Has<vmux_ecs::Active>)>,
    mut completed: MessageWriter<RaiseCompleted>,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for request in requested.read() {
        let Ok((target, parent, active)) = attempts.get(request.0) else {
            continue;
        };
        let current = windows.target(parent.parent());
        let valid = active && current.is_some_and(|current| current.matches(target.geometry()));
        completed.write(RaiseCompleted {
            attempt: request.0,
            raised: valid
                && native.matches(&browsers, parent.parent(), target.window)
                && browsers.try_raise_windowed_to_front(&parent.parent())
                && native.matches(&browsers, parent.parent(), target.window),
        });
    }
}

fn finish(
    mut completed: MessageReader<RaiseCompleted>,
    attempts: Query<(&Placement, &ChildOf)>,
    mut owners: Query<&mut History, (With<LayoutCef>, With<WebviewWindowed>)>,
    windows: Windows,
    mut commands: Commands,
) {
    for event in completed.read() {
        let Ok((target, parent)) = attempts.get(event.attempt) else {
            continue;
        };
        commands.entity(event.attempt).try_despawn();
        let owner = parent.parent();
        let Ok(mut history) = owners.get_mut(owner) else {
            continue;
        };
        if history.pending != Some(event.attempt) {
            continue;
        }
        history.pending = None;
        if event.raised
            && windows
                .target(owner)
                .is_some_and(|current| current.matches(target.geometry()))
        {
            history.raised = Some(target.geometry());
        }
    }
}

fn retire<T: Component>(
    removed: On<Remove, T>,
    mut owners: Query<&mut History>,
    mut commands: Commands,
) {
    let owner = removed.event_target();
    if let Ok(mut history) = owners.get_mut(owner)
        && let Some(attempt) = history.pending.take()
    {
        commands.entity(attempt).try_despawn();
    }
    commands.entity(owner).try_remove::<History>();
}

fn retire_window(
    removed: On<Remove, Window>,
    mut owners: Query<&mut History>,
    attempts: Query<&Placement>,
    mut commands: Commands,
) {
    let window = removed.event_target();
    for mut history in &mut owners {
        let raised_here = history.raised.is_some_and(|previous| previous.0 == window);
        let pending_here = history.pending.is_some_and(|attempt| {
            attempts
                .get(attempt)
                .is_ok_and(|target| target.window == window)
        });
        if !raised_here && !pending_here {
            continue;
        }
        history.raised = None;
        if let Some(attempt) = history.pending.take() {
            commands.entity(attempt).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{MinimalPlugins, ResMut, Resource, resource_exists};
    use bevy::window::WindowResolution;

    struct PlacementFixture {
        app: App,
        window: Entity,
        owner: Entity,
    }

    impl PlacementFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, LayoutPlacementPlugin))
                .insert_resource(NativeFixture {
                    calls: Vec::new(),
                    raises: Vec::new(),
                    withheld: Vec::new(),
                    raise_succeeds: true,
                    hold: false,
                    hold_raise: false,
                    raise_host: None,
                })
                .add_systems(
                    PostUpdate,
                    (
                        match_fixture
                            .after(match_window)
                            .before(place)
                            .run_if(resource_exists::<NativeFixture>),
                        respond
                            .after(apply)
                            .before(confirm)
                            .run_if(resource_exists::<NativeFixture>),
                        respond_raise
                            .after(raise)
                            .before(finish)
                            .run_if(resource_exists::<NativeFixture>),
                    ),
                );
            let window = app
                .world_mut()
                .spawn((
                    Window {
                        resolution: WindowResolution::new(800, 600),
                        ..Default::default()
                    },
                    vmux_ecs::Active,
                ))
                .id();
            let owner = app
                .world_mut()
                .spawn((
                    LayoutCef,
                    WebviewWindowed,
                    HostWindow(window),
                    NativeReady(true),
                    NativeHost(window),
                ))
                .id();
            Self { app, window, owner }
        }

        fn step(&mut self) -> Vec<(Entity, Entity, Vec2, f32, bool)> {
            self.app.update();
            let mut native = self.app.world_mut().resource_mut::<NativeFixture>();
            let mut calls = std::mem::take(&mut native.calls);
            let raises = std::mem::take(&mut native.raises);
            for call in &mut calls {
                call.4 = raises.contains(&call.0);
            }
            calls
        }

        fn ready(&mut self, ready: bool) {
            self.app
                .world_mut()
                .get_mut::<NativeReady>(self.owner)
                .unwrap()
                .0 = ready;
        }

        fn live_entities(&mut self) -> usize {
            let world = self.app.world_mut();
            let mut entities = world.query::<Entity>();
            entities.iter(world).count()
        }

        fn another_window(&mut self) -> Entity {
            self.app
                .world_mut()
                .spawn(Window {
                    resolution: WindowResolution::new(800, 600),
                    ..Default::default()
                })
                .id()
        }

        fn complete_frame(&mut self, attempt: Entity, ready: bool) {
            self.app
                .world_mut()
                .write_message(PlacementCompleted { attempt, ready });
        }
    }

    #[derive(Component)]
    struct NativeReady(bool);

    #[derive(Component)]
    struct NativeHost(Entity);

    #[derive(Resource)]
    struct NativeFixture {
        calls: Vec<(Entity, Entity, Vec2, f32, bool)>,
        raises: Vec<Entity>,
        withheld: Vec<Entity>,
        raise_succeeds: bool,
        hold: bool,
        hold_raise: bool,
        raise_host: Option<Entity>,
    }

    fn match_fixture(
        mut requested: MessageReader<PlacementRequested>,
        attempts: Query<(&Placement, &ChildOf)>,
        views: Query<&NativeHost>,
        mut matched: MessageWriter<WindowMatched>,
    ) {
        for request in requested.read() {
            let Ok((target, parent)) = attempts.get(request.0) else {
                continue;
            };
            matched.write(WindowMatched {
                attempt: request.0,
                matched: views.get(parent.parent()).unwrap().0 == target.window,
            });
        }
    }

    fn respond(
        mut requested: MessageReader<FrameRequested>,
        attempts: Query<(&Placement, &ChildOf, Has<vmux_ecs::Active>)>,
        views: Query<(&NativeReady, &NativeHost)>,
        mut native: ResMut<NativeFixture>,
        mut completed: MessageWriter<PlacementCompleted>,
    ) {
        for request in requested.read() {
            let Ok((target, parent, active)) = attempts.get(request.0) else {
                continue;
            };
            let owner = parent.parent();
            let (view, native_host) = views.get(owner).unwrap();
            let ready = active && view.0;
            if active {
                native
                    .calls
                    .push((owner, native_host.0, target.size, target.scale, false));
            }
            if native.hold {
                native.withheld.push(request.0);
                continue;
            }
            completed.write(PlacementCompleted {
                attempt: request.0,
                ready,
            });
        }
    }

    fn respond_raise(
        mut requested: MessageReader<RaiseRequested>,
        attempts: Query<(&Placement, &ChildOf, Has<vmux_ecs::Active>)>,
        mut native: ResMut<NativeFixture>,
        mut views: Query<&mut NativeHost>,
        mut completed: MessageWriter<RaiseCompleted>,
    ) {
        for request in requested.read() {
            let Ok((target, parent, active)) = attempts.get(request.0) else {
                continue;
            };
            if let Some(host) = native.raise_host.take() {
                views.get_mut(parent.parent()).unwrap().0 = host;
            }
            let bound = active && views.get(parent.parent()).unwrap().0 == target.window;
            if bound {
                native.raises.push(parent.parent());
            }
            if native.hold_raise {
                native.withheld.push(request.0);
                continue;
            }
            completed.write(RaiseCompleted {
                attempt: request.0,
                raised: bound && native.raise_succeeds,
            });
        }
    }

    #[test]
    fn native_adapter_absence_does_not_accumulate_work_and_recovery_drains_it() {
        let mut fixture = PlacementFixture::new();
        let native = fixture
            .app
            .world_mut()
            .remove_resource::<NativeFixture>()
            .unwrap();
        let baseline = fixture.live_entities();
        fixture.app.update();
        let pending = fixture.live_entities();
        for _ in 0..32 {
            fixture.app.update();
            assert_eq!(fixture.live_entities(), pending);
        }
        fixture.app.insert_resource(native);
        fixture.app.update();
        assert_eq!(fixture.live_entities(), baseline);
    }

    #[test]
    fn native_adapter_absence_does_not_retain_retired_owner_or_window_work() {
        for retirement in 0..3 {
            let mut fixture = PlacementFixture::new();
            fixture
                .app
                .world_mut()
                .remove_resource::<NativeFixture>()
                .unwrap();
            let baseline = fixture.live_entities();
            fixture.app.update();
            match retirement {
                0 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.owner)
                        .remove::<LayoutCef>();
                }
                1 => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.owner)
                        .remove::<WebviewWindowed>();
                }
                _ => {
                    fixture
                        .app
                        .world_mut()
                        .entity_mut(fixture.window)
                        .remove::<Window>();
                }
            }
            fixture.app.update();
            assert_eq!(fixture.live_entities(), baseline);
        }
    }

    #[test]
    fn stable_layouts_keep_frame_updates_without_repeating_a_successful_raise() {
        let mut fixture = PlacementFixture::new();
        assert_eq!(
            fixture.step(),
            [(
                fixture.owner,
                fixture.window,
                Vec2::new(800.0, 600.0),
                1.0,
                true
            )]
        );
        assert_eq!(
            fixture.step(),
            [(
                fixture.owner,
                fixture.window,
                Vec2::new(800.0, 600.0),
                1.0,
                false
            )]
        );
    }

    #[test]
    fn a_view_that_becomes_ready_requests_its_first_raise() {
        let mut fixture = PlacementFixture::new();
        fixture.ready(false);
        assert!(!fixture.step()[0].4);
        fixture.ready(true);
        assert!(fixture.step()[0].4);
        assert!(!fixture.step()[0].4);
        fixture.ready(false);
        assert!(!fixture.step()[0].4);
        fixture.ready(true);
        assert!(fixture.step()[0].4);
    }

    #[test]
    fn failed_raise_submission_does_not_suppress_the_next_attempt() {
        let mut fixture = PlacementFixture::new();
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .raise_succeeds = false;
        assert!(fixture.step()[0].4);
        assert!(fixture.step()[0].4);
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .raise_succeeds = true;
        assert!(fixture.step()[0].4);
        assert!(!fixture.step()[0].4);
    }

    #[test]
    fn native_generation_replacement_cannot_reuse_a_raise_receipt() {
        let mut fixture = PlacementFixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(BinIpcSurfaceGeneration(1));
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(BinIpcSurfaceGeneration(2));
        assert!(fixture.step()[0].4);
        assert!(!fixture.step()[0].4);
    }

    #[test]
    fn a_native_window_change_before_raise_cannot_acknowledge_the_old_binding() {
        let mut fixture = PlacementFixture::new();
        let window = fixture.another_window();
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .raise_host = Some(window);
        let calls = fixture.step();
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].4);
        assert!(fixture.step().is_empty());
        fixture
            .app
            .world_mut()
            .get_mut::<NativeHost>(fixture.owner)
            .unwrap()
            .0 = fixture.window;
        assert!(fixture.step()[0].4);
        assert!(!fixture.step()[0].4);
    }

    #[test]
    fn root_window_rebinding_waits_for_the_actual_native_binding() {
        let mut fixture = PlacementFixture::new();
        fixture.step();
        let window = fixture.another_window();
        fixture
            .app
            .world_mut()
            .get_mut::<HostWindow>(fixture.owner)
            .unwrap()
            .0 = window;
        assert!(fixture.step().is_empty());
        assert!(fixture.step().is_empty());
        fixture
            .app
            .world_mut()
            .get_mut::<NativeHost>(fixture.owner)
            .unwrap()
            .0 = window;
        assert_eq!(
            fixture.step(),
            [(fixture.owner, window, Vec2::new(800.0, 600.0), 1.0, true)]
        );
        assert!(!fixture.step()[0].4);
    }

    #[test]
    fn changed_physical_dimensions_and_scale_refresh_native_placement() {
        let mut fixture = PlacementFixture::new();
        fixture.step();
        {
            let mut window = fixture
                .app
                .world_mut()
                .get_mut::<Window>(fixture.window)
                .unwrap();
            window.resolution = WindowResolution::new(1200, 900);
            window.resolution.set_scale_factor_override(Some(2.0));
        }
        assert_eq!(
            fixture.step(),
            [(
                fixture.owner,
                fixture.window,
                Vec2::new(1200.0, 900.0),
                2.0,
                true
            )]
        );
    }

    #[test]
    fn layout_reattachment_cannot_reuse_the_previous_raise_receipt() {
        let mut fixture = PlacementFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<LayoutCef>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(LayoutCef);
        assert!(fixture.step()[0].4);
    }

    #[test]
    fn windowed_view_reattachment_cannot_reuse_the_previous_raise_receipt() {
        let mut fixture = PlacementFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<WebviewWindowed>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(WebviewWindowed);
        assert!(fixture.step()[0].4);
    }

    #[test]
    fn root_window_reattachment_reconciles_a_fresh_raise() {
        let mut fixture = PlacementFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.window)
            .remove::<Window>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.window)
            .insert(Window {
                resolution: WindowResolution::new(800, 600),
                ..Default::default()
            });
        assert!(fixture.step()[0].4);
    }

    #[test]
    fn late_completion_cannot_acknowledge_a_reattached_layout() {
        let mut fixture = PlacementFixture::new();
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .hold_raise = true;
        fixture.step();
        let old = fixture.app.world().resource::<NativeFixture>().withheld[0];
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<LayoutCef>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(LayoutCef);
        {
            let mut native = fixture.app.world_mut().resource_mut::<NativeFixture>();
            native.hold_raise = false;
            native.raise_succeeds = false;
        }
        fixture.app.world_mut().write_message(RaiseCompleted {
            attempt: old,
            raised: true,
        });
        assert!(fixture.step()[0].4);
        fixture
            .app
            .world_mut()
            .resource_mut::<NativeFixture>()
            .raise_succeeds = true;
        assert!(fixture.step()[0].4);
        assert!(!fixture.step()[0].4);
    }

    #[test]
    fn retired_native_completion_cannot_clear_a_replacement_window_receipt() {
        let mut fixture = PlacementFixture::new();
        fixture.app.world_mut().resource_mut::<NativeFixture>().hold = true;
        fixture.step();
        let old = fixture.app.world().resource::<NativeFixture>().withheld[0];
        let window = fixture.another_window();
        fixture
            .app
            .world_mut()
            .get_mut::<HostWindow>(fixture.owner)
            .unwrap()
            .0 = window;
        fixture
            .app
            .world_mut()
            .get_mut::<NativeHost>(fixture.owner)
            .unwrap()
            .0 = window;
        fixture.app.world_mut().resource_mut::<NativeFixture>().hold = false;
        assert!(fixture.step()[0].4);
        fixture.complete_frame(old, false);
        assert!(!fixture.step()[0].4);
        assert!(!fixture.step()[0].4);
    }

    #[test]
    fn a_missing_window_does_not_receive_native_placement() {
        let mut fixture = PlacementFixture::new();
        fixture.step();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.window)
            .remove::<Window>();
        assert!(fixture.step().is_empty());
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.window)
            .insert(Window {
                resolution: WindowResolution::new(800, 600),
                ..Default::default()
            });
        assert!(fixture.step()[0].4);
    }

    #[test]
    fn independent_layouts_do_not_share_raise_suppression() {
        let mut fixture = PlacementFixture::new();
        let window = fixture.another_window();
        let other = fixture
            .app
            .world_mut()
            .spawn((
                LayoutCef,
                WebviewWindowed,
                HostWindow(window),
                NativeReady(true),
                NativeHost(window),
            ))
            .id();
        assert_eq!(fixture.step().iter().filter(|call| call.4).count(), 2);
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .remove::<LayoutCef>();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.owner)
            .insert(LayoutCef);
        let calls = fixture.step();
        assert!(calls.iter().any(|call| call.0 == fixture.owner && call.4));
        assert!(calls.iter().any(|call| call.0 == other && !call.4));
    }
}
