use bevy::ecs::system::SystemParam;
use bevy::prelude::{
    App, ChildOf, Commands, Component, Entity, EntityEvent, Has, IntoScheduleConfigs, Message,
    MessageReader, MessageWriter, Name, NonSend, On, Plugin, PostUpdate, Query, Remove, SystemSet,
    Vec2, Window, With, Without,
};
use bevy_cef::prelude::{BinIpcSurfaceGeneration, Browsers, HostWindow, WebviewWindowed};
use vmux_layout::LayoutCef;
use vmux_layout::window::FocusedWindow;

pub(crate) struct LayoutPlacementPlugin;

impl Plugin for LayoutPlacementPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlacementRequested>()
            .add_message::<PlacementCompleted>()
            .add_message::<RaiseRequested>()
            .add_message::<RaiseCompleted>()
            .add_systems(
                PostUpdate,
                (initialize, select, apply, confirm, raise, finish)
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
            commands.entity(attempt).try_remove::<vmux_ecs::Active>();
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

fn apply(
    browsers: Option<NonSend<Browsers>>,
    mut requested: MessageReader<PlacementRequested>,
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
        if active {
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
            ready: active && browsers.windowed_view_ready(&owner),
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
    mut requested: MessageReader<RaiseRequested>,
    attempts: Query<(&ChildOf, Has<vmux_ecs::Active>), With<Placement>>,
    mut completed: MessageWriter<RaiseCompleted>,
) {
    let Some(browsers) = browsers else {
        return;
    };
    for request in requested.read() {
        let Ok((parent, active)) = attempts.get(request.0) else {
            continue;
        };
        completed.write(RaiseCompleted {
            attempt: request.0,
            raised: active && browsers.try_raise_windowed_to_front(&parent.parent()),
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
        commands.entity(attempt).try_remove::<vmux_ecs::Active>();
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
            commands.entity(attempt).try_remove::<vmux_ecs::Active>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{MinimalPlugins, ResMut, Resource};
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
                })
                .add_systems(
                    PostUpdate,
                    (
                        respond.after(apply).before(confirm),
                        respond_raise.after(raise).before(finish),
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

    #[derive(Resource)]
    struct NativeFixture {
        calls: Vec<(Entity, Entity, Vec2, f32, bool)>,
        raises: Vec<Entity>,
        withheld: Vec<Entity>,
        raise_succeeds: bool,
        hold: bool,
        hold_raise: bool,
    }

    fn respond(
        mut requested: MessageReader<PlacementRequested>,
        attempts: Query<(&Placement, &ChildOf, Has<vmux_ecs::Active>)>,
        views: Query<&NativeReady>,
        mut native: ResMut<NativeFixture>,
        mut completed: MessageWriter<PlacementCompleted>,
    ) {
        for request in requested.read() {
            let Ok((target, parent, active)) = attempts.get(request.0) else {
                continue;
            };
            let owner = parent.parent();
            let ready = active && views.get(owner).is_ok_and(|view| view.0);
            if active {
                native
                    .calls
                    .push((owner, target.window, target.size, target.scale, false));
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
        attempts: Query<(&ChildOf, Has<vmux_ecs::Active>), With<Placement>>,
        mut native: ResMut<NativeFixture>,
        mut completed: MessageWriter<RaiseCompleted>,
    ) {
        for request in requested.read() {
            let Ok((parent, active)) = attempts.get(request.0) else {
                continue;
            };
            if active {
                native.raises.push(parent.parent());
            }
            if native.hold_raise {
                native.withheld.push(request.0);
                continue;
            }
            completed.write(RaiseCompleted {
                attempt: request.0,
                raised: active && native.raise_succeeds,
            });
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
    fn root_window_rebinding_invalidates_identical_geometry() {
        let mut fixture = PlacementFixture::new();
        fixture.step();
        let window = fixture.another_window();
        fixture
            .app
            .world_mut()
            .get_mut::<HostWindow>(fixture.owner)
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
