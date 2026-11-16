use bevy::prelude::{
    App, ChildOf, Commands, Component, Entity, EntityEvent, IntoScheduleConfigs, Message,
    MessageReader, MessageWriter, Name, On, Plugin, PostUpdate, Query, Remove, SystemSet, Window,
    With, Without,
};
use vmux_layout::window::FocusedWindow;
#[cfg(target_os = "macos")]
use vmux_page::Page;
use vmux_page::PagePlacement;

use crate::host_focus::HostFocusIntent;

pub(super) struct NativeFocusPlugin;

impl Plugin for NativeFocusPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ResponderRequested>()
            .add_message::<ResponderCompleted>()
            .configure_sets(
                PostUpdate,
                (FocusSet::Select, FocusSet::Apply, FocusSet::Confirm).chain(),
            )
            .add_systems(
                PostUpdate,
                (initialize, select)
                    .chain()
                    .in_set(FocusSet::Select)
                    .after(crate::BrowserSystemSet::HostFocusApplied),
            )
            .add_systems(PostUpdate, confirm.in_set(FocusSet::Confirm))
            .add_observer(retire)
            .add_observer(retire_surface::<HostedSurface>)
            .add_observer(retire_surface::<HostedSurfaceGeneration>);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FocusSet {
    Select,
    Apply,
    Confirm,
}

#[derive(Component, Clone)]
pub(super) struct HostedSurface {
    pub(super) placement: PagePlacement,
    #[cfg(target_os = "macos")]
    pub(super) page: &'static Page,
    pub(super) window: Entity,
    #[cfg(target_os = "macos")]
    pub(super) url: String,
}

impl HostedSurface {
    fn requested(&self, entity: Entity, intent: HostFocusIntent, window: Option<Entity>) -> bool {
        match intent {
            HostFocusIntent::LayoutView => {
                self.placement == PagePlacement::Layout
                    && window.is_none_or(|window| self.window == window)
            }
            HostFocusIntent::HostedPane(target) => entity == target,
            _ => false,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HostedSurfaceGeneration(pub(super) u64);

#[derive(Component, Default)]
struct HostedFocus {
    applied: Option<(Entity, u64)>,
}

#[derive(Component)]
pub(super) struct FocusAttempt {
    pub(super) surface: Entity,
    generation: u64,
}

#[derive(Message)]
pub(super) struct ResponderRequested {
    pub(super) attempt: Entity,
}

#[derive(Message)]
pub(super) struct ResponderCompleted {
    pub(super) attempt: Entity,
    pub(super) accepted: bool,
}

fn initialize(
    windows: Query<Entity, (With<Window>, Without<HostedFocus>)>,
    mut commands: Commands,
) {
    for window in &windows {
        commands.entity(window).insert(HostedFocus::default());
    }
}

fn select(
    intent: Query<&HostFocusIntent>,
    hosted: Query<(Entity, &HostedSurface, &HostedSurfaceGeneration)>,
    focused_window: FocusedWindow,
    mut windows: Query<(Entity, &mut HostedFocus), With<Window>>,
    attempts: Query<Entity, With<FocusAttempt>>,
    mut requests: MessageWriter<ResponderRequested>,
    mut commands: Commands,
) {
    for attempt in &attempts {
        commands.entity(attempt).despawn();
    }
    let intent = intent.single().copied().unwrap_or_default();
    let active_window = focused_window.entity();
    let wanted = match intent {
        HostFocusIntent::HostedPane(entity) => match hosted.get(entity) {
            Ok((entity, surface, generation)) => Some((surface.window, entity, generation.0)),
            Err(_) => None,
        },
        HostFocusIntent::LayoutView => {
            let mut wanted = None;
            for (entity, surface, generation) in &hosted {
                if surface.requested(entity, intent, active_window) {
                    wanted = Some((surface.window, entity, generation.0));
                    break;
                }
            }
            wanted
        }
        _ => None,
    };
    for (window, mut focus) in &mut windows {
        let Some((owner, surface, generation)) = wanted else {
            if focus.applied.is_some() {
                focus.applied = None;
            }
            continue;
        };
        if owner != window {
            if focus.applied.is_some() {
                focus.applied = None;
            }
            continue;
        }
        if focus.applied == Some((surface, generation)) && !focused_window.is_changed() {
            continue;
        }
        focus.applied = None;
        let attempt = commands
            .spawn((
                Name::new("Native focus attempt"),
                ChildOf(window),
                FocusAttempt {
                    surface,
                    generation,
                },
            ))
            .id();
        requests.write(ResponderRequested { attempt });
    }
}

fn confirm(
    mut completed: MessageReader<ResponderCompleted>,
    attempts: Query<(&ChildOf, &FocusAttempt)>,
    hosted: Query<(&HostedSurface, &HostedSurfaceGeneration)>,
    intent: Query<&HostFocusIntent>,
    focused_window: FocusedWindow,
    mut windows: Query<&mut HostedFocus, With<Window>>,
    mut commands: Commands,
) {
    let intent = intent.single().copied().unwrap_or_default();
    for result in completed.read() {
        let Ok((owner, attempt)) = attempts.get(result.attempt) else {
            continue;
        };
        commands.entity(result.attempt).despawn();
        if !result.accepted {
            continue;
        }
        let Ok((surface, generation)) = hosted.get(attempt.surface) else {
            continue;
        };
        if surface.window != owner.parent()
            || generation.0 != attempt.generation
            || !surface.requested(attempt.surface, intent, focused_window.entity())
        {
            continue;
        }
        if let Ok(mut focus) = windows.get_mut(owner.parent()) {
            focus.applied = Some((attempt.surface, attempt.generation));
        }
    }
}

fn retire(
    removed: On<Remove, Window>,
    attempts: Query<(Entity, &ChildOf), With<FocusAttempt>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.event_target())
        .try_remove::<HostedFocus>();
    for (entity, owner) in &attempts {
        if owner.parent() == removed.event_target() {
            commands.entity(entity).despawn();
        }
    }
}

fn retire_surface<T: Component>(
    removed: On<Remove, T>,
    mut windows: Query<&mut HostedFocus>,
    attempts: Query<(Entity, &FocusAttempt)>,
    mut commands: Commands,
) {
    for mut focus in &mut windows {
        if focus
            .applied
            .is_some_and(|(entity, _)| entity == removed.event_target())
        {
            focus.applied = None;
        }
    }
    for (entity, attempt) in &attempts {
        if attempt.surface == removed.event_target() {
            commands.entity(entity).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{MinimalPlugins, ResMut, Resource};
    use vmux_ecs::Active;

    struct ResponderFixturePlugin;

    impl Plugin for ResponderFixturePlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<ResponderFixture>()
                .add_systems(PostUpdate, respond.in_set(FocusSet::Apply));
        }
    }

    #[derive(Resource, Default)]
    struct ResponderFixture {
        accepted: bool,
        hold: bool,
        calls: Vec<(Entity, Entity)>,
    }

    fn respond(
        mut requests: MessageReader<ResponderRequested>,
        attempts: Query<&FocusAttempt>,
        mut fixture: ResMut<ResponderFixture>,
        mut completed: MessageWriter<ResponderCompleted>,
    ) {
        for request in requests.read() {
            let Ok(attempt) = attempts.get(request.attempt) else {
                continue;
            };
            fixture.calls.push((request.attempt, attempt.surface));
            if !fixture.hold {
                completed.write(ResponderCompleted {
                    attempt: request.attempt,
                    accepted: fixture.accepted,
                });
            }
        }
    }

    struct FocusFixture {
        app: App,
        intent: Entity,
    }

    impl FocusFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, NativeFocusPlugin, ResponderFixturePlugin));
            let intent = app.world_mut().spawn(HostFocusIntent::default()).id();
            Self { app, intent }
        }

        fn window(&mut self) -> Entity {
            self.app.world_mut().spawn(Window::default()).id()
        }

        fn surface(&mut self, window: Entity, placement: PagePlacement) -> Entity {
            self.app
                .world_mut()
                .spawn((
                    HostedSurface {
                        placement,
                        #[cfg(target_os = "macos")]
                        page: &const { Page::pane("vmux://native-focus-test", vmux_layout::ui::Page) },
                        window,
                        #[cfg(target_os = "macos")]
                        url: "vmux://native-focus-test".to_string(),
                    },
                    HostedSurfaceGeneration(1),
                ))
                .id()
        }

        fn intent(&mut self, intent: HostFocusIntent) {
            self.app.world_mut().entity_mut(self.intent).insert(intent);
        }

        fn accept(&mut self) {
            self.app
                .world_mut()
                .resource_mut::<ResponderFixture>()
                .accepted = true;
        }

        fn hold(&mut self) {
            self.app.world_mut().resource_mut::<ResponderFixture>().hold = true;
        }

        fn calls(&self) -> Vec<Entity> {
            self.app
                .world()
                .resource::<ResponderFixture>()
                .calls
                .iter()
                .map(|(_, surface)| *surface)
                .collect()
        }

        fn attempt(&self) -> Entity {
            self.app
                .world()
                .resource::<ResponderFixture>()
                .calls
                .last()
                .unwrap()
                .0
        }
    }

    #[test]
    fn failed_delivery_retries_and_success_suppresses_unchanged_focus() {
        let mut fixture = FocusFixture::new();
        let window = fixture.window();
        let surface = fixture.surface(window, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.app.update();
        fixture.accept();
        fixture.app.update();
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface]);
        let mut attempts = fixture.app.world_mut().query::<&FocusAttempt>();
        assert!(attempts.iter(fixture.app.world()).next().is_none());
    }

    #[test]
    fn changing_surface_generation_requires_new_delivery() {
        let mut fixture = FocusFixture::new();
        let window = fixture.window();
        let surface = fixture.surface(window, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.accept();
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .entity_mut(surface)
            .insert(HostedSurfaceGeneration(2));
        fixture.app.update();
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface]);
    }

    #[test]
    fn window_focus_switches_do_not_reuse_another_window_receipt() {
        let mut fixture = FocusFixture::new();
        let first = fixture.window();
        let second = fixture.window();
        let first_surface = fixture.surface(first, PagePlacement::Layout);
        let second_surface = fixture.surface(second, PagePlacement::Layout);
        fixture.intent(HostFocusIntent::LayoutView);
        fixture.accept();
        fixture.app.world_mut().entity_mut(first).insert(Active);
        fixture.app.update();
        fixture.app.update();
        fixture.app.world_mut().entity_mut(first).remove::<Active>();
        fixture.app.world_mut().entity_mut(second).insert(Active);
        fixture.app.update();
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .entity_mut(second)
            .remove::<Active>();
        fixture.app.world_mut().entity_mut(first).insert(Active);
        fixture.app.update();
        fixture.app.update();
        assert_eq!(
            fixture.calls(),
            [first_surface, second_surface, first_surface]
        );
    }

    #[test]
    fn moving_a_surface_to_another_window_requires_new_delivery() {
        let mut fixture = FocusFixture::new();
        let first = fixture.window();
        let second = fixture.window();
        let surface = fixture.surface(first, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.accept();
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .get_mut::<HostedSurface>(surface)
            .unwrap()
            .window = second;
        fixture.app.update();
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface]);
    }

    #[test]
    fn leaving_hosted_focus_requires_delivery_when_the_surface_returns() {
        let mut fixture = FocusFixture::new();
        let window = fixture.window();
        let surface = fixture.surface(window, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.accept();
        fixture.app.update();
        fixture.intent(HostFocusIntent::WinitHost);
        fixture.app.update();
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface]);
    }

    #[test]
    fn same_update_window_reattachment_retires_delivery_history() {
        let mut fixture = FocusFixture::new();
        let window = fixture.window();
        let surface = fixture.surface(window, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.accept();
        fixture.app.update();
        fixture
            .app
            .world_mut()
            .entity_mut(window)
            .remove::<Window>();
        fixture
            .app
            .world_mut()
            .entity_mut(window)
            .insert(Window::default());
        fixture.app.update();
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface]);
    }

    #[test]
    fn same_update_surface_reattachment_retires_delivery_history() {
        let mut fixture = FocusFixture::new();
        let window = fixture.window();
        let surface = fixture.surface(window, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.accept();
        fixture.app.update();
        let generation = fixture
            .app
            .world_mut()
            .entity_mut(surface)
            .take::<HostedSurfaceGeneration>()
            .unwrap();
        fixture
            .app
            .world_mut()
            .entity_mut(surface)
            .insert(generation);
        fixture.app.update();
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface]);
    }

    #[test]
    fn a_retired_window_attempt_cannot_acknowledge_its_replacement() {
        let mut fixture = FocusFixture::new();
        let window = fixture.window();
        let surface = fixture.surface(window, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.hold();
        fixture.app.update();
        let attempt = fixture.attempt();
        fixture
            .app
            .world_mut()
            .entity_mut(window)
            .remove::<Window>();
        fixture
            .app
            .world_mut()
            .entity_mut(window)
            .insert(Window::default());
        fixture.app.world_mut().write_message(ResponderCompleted {
            attempt,
            accepted: true,
        });
        fixture.app.update();
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface, surface]);
    }

    #[test]
    fn a_retired_surface_attempt_cannot_acknowledge_a_new_generation() {
        let mut fixture = FocusFixture::new();
        let window = fixture.window();
        let surface = fixture.surface(window, PagePlacement::Pane);
        fixture.intent(HostFocusIntent::HostedPane(surface));
        fixture.hold();
        fixture.app.update();
        let attempt = fixture.attempt();
        fixture
            .app
            .world_mut()
            .entity_mut(surface)
            .insert(HostedSurfaceGeneration(2));
        fixture.app.world_mut().write_message(ResponderCompleted {
            attempt,
            accepted: true,
        });
        fixture.app.update();
        fixture.app.update();
        assert_eq!(fixture.calls(), [surface, surface, surface]);
    }
}
