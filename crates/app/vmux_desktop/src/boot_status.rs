use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use std::time::{Duration, Instant};
use vmux_ecs::page::PageReady;
use vmux_ecs::store::WorkspaceRestore;
use vmux_ecs::time::{Clock, Deadline, DeadlinePlugin};
use vmux_layout::cef::LayoutCef;
use vmux_layout::space::Space;
use vmux_layout::stack::Stack;

pub(crate) struct BootStatusPlugin;

struct SplashTimeout;

impl Plugin for BootStatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DeadlinePlugin::<SplashTimeout>::default())
            .add_systems(Startup, spawn)
            .add_systems(Update, update.after(vmux_layout::stack::ComputeFocusSet))
            .add_systems(
                Last,
                (arm_deadline, request_dismissal)
                    .chain()
                    .in_set(SplashRequestSet)
                    .after(WindowRevealSet),
            );
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((Name::new("Boot status"), SplashStatus::default()));
}

#[derive(bevy::ecs::system::SystemParam)]
struct BootLayout<'w, 's> {
    layout: Query<'w, 's, (), (With<LayoutCef>, With<PageReady>)>,
    stacks: Query<'w, 's, (Entity, Option<&'static Children>), With<Stack>>,
    ready: Query<'w, 's, (), With<PageReady>>,
    child_of: Query<'w, 's, &'static ChildOf>,
    spaces: Query<'w, 's, Has<vmux_ecs::Active>, With<Space>>,
}

impl BootLayout<'_, '_> {
    fn ready(&self) -> bool {
        !self.layout.is_empty()
    }

    fn page_counts(&self) -> (usize, usize) {
        let mut total = 0;
        let mut ready = 0;
        for (stack, children) in &self.stacks {
            if !self.stack_is_active(stack) {
                continue;
            }
            if let Some(children) = children.filter(|children| !children.is_empty()) {
                total += 1;
                if children.iter().any(|entity| self.ready.contains(entity)) {
                    ready += 1;
                }
            }
        }
        (total, ready)
    }

    fn stack_is_active(&self, stack: Entity) -> bool {
        let mut entity = stack;
        loop {
            if let Ok(active) = self.spaces.get(entity) {
                return active;
            }
            match self.child_of.get(entity) {
                Ok(child_of) => entity = child_of.get(),
                Err(_) => return true,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    Starting,
    RestoringSpace,
    LoadingInterface,
    LoadingPages,
}

#[derive(Component)]
#[require(PageCounts)]
#[cfg_attr(target_os = "macos", require(crate::macos::Activation))]
pub struct SplashStatus {
    pub phase: BootPhase,
    #[cfg_attr(
        not(all(target_os = "macos", feature = "native-glass")),
        allow(dead_code)
    )]
    pub reveal_ready: bool,
}

impl Default for SplashStatus {
    fn default() -> Self {
        Self {
            phase: BootPhase::Starting,
            reveal_ready: false,
        }
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct SplashRequestSet;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct WindowRevealSet;

#[derive(Component)]
pub(super) struct PresentedAt(pub(super) Instant);

#[derive(Component)]
pub(super) struct Dismissal;

#[derive(Component)]
pub(super) struct Closed;

#[derive(Component, Default)]
pub(super) struct PageCounts {
    ready: usize,
    total: usize,
}

impl PageCounts {
    fn refresh(&mut self, total: usize, ready: usize) -> bool {
        let changed = self.total != total || self.ready != ready;
        self.total = total;
        self.ready = ready;
        changed
    }
}

impl SplashStatus {
    pub(super) fn display(&self, pages: &PageCounts) -> String {
        match self.phase {
            BootPhase::Starting => "Starting...".to_string(),
            BootPhase::RestoringSpace => "Restoring space...".to_string(),
            BootPhase::LoadingInterface => "Loading interface...".to_string(),
            BootPhase::LoadingPages => format!("Loading page {}/{}...", pages.ready, pages.total),
        }
    }

    fn refresh(
        &mut self,
        restore: &WorkspaceRestore,
        layout_ready: bool,
        pages: &PageCounts,
        progress_changed: bool,
    ) {
        let phase = if layout_ready && pages.total > 0 {
            BootPhase::LoadingPages
        } else if layout_ready || restore.complete {
            BootPhase::LoadingInterface
        } else if restore.store_present {
            BootPhase::RestoringSpace
        } else {
            BootPhase::Starting
        };
        let changed = self.phase != phase || progress_changed;
        self.phase = phase;
        self.reveal_ready = layout_ready;
        if changed {
            info!("boot: {}", self.display(pages));
        }
    }
}

fn update(
    state: Single<(&mut SplashStatus, &mut PageCounts)>,
    restore: Single<&WorkspaceRestore>,
    layout: BootLayout,
) {
    let (mut status, mut pages) = state.into_inner();
    let (total, ready) = layout.page_counts();
    let changed = pages.refresh(total, ready);
    status.refresh(&restore, layout.ready(), &pages, changed);
}

fn arm_deadline(
    panels: Query<
        (Entity, &PresentedAt),
        (
            With<SplashStatus>,
            Without<Deadline<SplashTimeout>>,
            Without<Dismissal>,
            Without<Closed>,
        ),
    >,
    mut commands: Commands,
) {
    for (owner, presented) in &panels {
        let deadline = presented.0 + Duration::from_secs(20);
        commands
            .entity(owner)
            .insert(Deadline::<SplashTimeout>::at(deadline));
    }
}

fn request_dismissal(
    panels: Query<
        (Entity, &PresentedAt),
        (With<SplashStatus>, Without<Dismissal>, Without<Closed>),
    >,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    clock: Res<Clock>,
    mut commands: Commands,
) {
    let visible = windows.single().is_ok_and(|window| window.visible);
    for (owner, presented) in &panels {
        let forced = !visible && clock.elapsed_since(presented.0) >= Duration::from_secs(20);
        if !visible && !forced {
            continue;
        }
        if forced {
            warn!("splash: window did not reveal within timeout; dismissing splash");
        }
        commands
            .entity(owner)
            .insert(Dismissal)
            .remove::<Deadline<SplashTimeout>>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, ChildOf, Entity, Window, With};
    use std::sync::{Arc, mpsc};
    use std::task::{Wake, Waker};

    struct WakeProbe(mpsc::SyncSender<()>);

    impl Wake for WakeProbe {
        fn wake(self: Arc<Self>) {
            let _ = self.0.try_send(());
        }
    }

    struct Fixture {
        app: App,
        wake: mpsc::Receiver<()>,
        status: Entity,
        restore: Entity,
        window: Entity,
    }

    impl Fixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins(BootStatusPlugin);
            let (sender, wake) = mpsc::sync_channel(1);
            app.insert_resource(vmux_ecs::JobWake(Waker::from(Arc::new(WakeProbe(sender)))));
            let restore = app.world_mut().spawn(WorkspaceRestore::default()).id();
            let window = app
                .world_mut()
                .spawn((
                    Window {
                        visible: false,
                        ..Default::default()
                    },
                    bevy::window::PrimaryWindow,
                ))
                .id();
            app.update();
            let mut statuses = app
                .world_mut()
                .query_filtered::<Entity, With<SplashStatus>>();
            let status = statuses.single(app.world()).unwrap();
            Self {
                app,
                wake,
                status,
                restore,
                window,
            }
        }

        fn status(&self) -> &SplashStatus {
            self.app.world().get::<SplashStatus>(self.status).unwrap()
        }

        fn counts(&self) -> (usize, usize) {
            let pages = self.app.world().get::<PageCounts>(self.status).unwrap();
            (pages.ready, pages.total)
        }

        fn requested(&self) -> bool {
            self.app.world().get::<Dismissal>(self.status).is_some()
        }

        fn acknowledge(&mut self) {
            assert!(self.requested());
            self.app
                .world_mut()
                .entity_mut(self.status)
                .insert(Closed)
                .remove::<Dismissal>();
        }
    }

    #[test]
    fn production_boot_owner_tracks_restore_layout_and_pending_pages() {
        let mut fixture = Fixture::new();
        assert_eq!(fixture.status().phase, BootPhase::Starting);
        assert!(!fixture.status().reveal_ready);
        fixture
            .app
            .world_mut()
            .get_mut::<WorkspaceRestore>(fixture.restore)
            .unwrap()
            .store_present = true;
        fixture.app.update();
        assert_eq!(fixture.status().phase, BootPhase::RestoringSpace);
        fixture
            .app
            .world_mut()
            .get_mut::<WorkspaceRestore>(fixture.restore)
            .unwrap()
            .complete = true;
        fixture.app.update();
        assert_eq!(fixture.status().phase, BootPhase::LoadingInterface);
        fixture.app.world_mut().spawn((LayoutCef, PageReady {}));
        let stack = fixture.app.world_mut().spawn(Stack::default()).id();
        let page = fixture.app.world_mut().spawn(ChildOf(stack)).id();
        fixture.app.update();
        assert_eq!(fixture.status().phase, BootPhase::LoadingPages);
        assert_eq!(fixture.counts(), (0, 1));
        assert!(fixture.status().reveal_ready);
        fixture
            .app
            .world_mut()
            .entity_mut(page)
            .insert(PageReady {});
        fixture.app.update();
        assert_eq!(fixture.status().phase, BootPhase::LoadingPages);
        assert_eq!(fixture.counts(), (1, 1));
    }

    #[test]
    fn production_counts_only_the_active_space_and_uses_its_current_pages() {
        let mut fixture = Fixture::new();
        fixture.app.world_mut().spawn((LayoutCef, PageReady {}));
        let first = fixture
            .app
            .world_mut()
            .spawn((Space, vmux_ecs::Active))
            .id();
        let second = fixture.app.world_mut().spawn(Space).id();
        let first_stack = fixture
            .app
            .world_mut()
            .spawn((Stack::default(), ChildOf(first)))
            .id();
        let second_stack = fixture
            .app
            .world_mut()
            .spawn((Stack::default(), ChildOf(second)))
            .id();
        fixture.app.world_mut().spawn(ChildOf(first_stack));
        fixture
            .app
            .world_mut()
            .spawn((PageReady {}, ChildOf(second_stack)));
        fixture.app.update();
        assert_eq!(fixture.status().phase, BootPhase::LoadingPages);
        assert_eq!(fixture.counts(), (0, 1));
        fixture
            .app
            .world_mut()
            .entity_mut(first)
            .remove::<vmux_ecs::Active>();
        fixture
            .app
            .world_mut()
            .entity_mut(second)
            .insert(vmux_ecs::Active);
        fixture.app.update();
        assert_eq!(fixture.status().phase, BootPhase::LoadingPages);
        assert_eq!(fixture.counts(), (1, 1));
    }

    #[test]
    fn visible_window_requests_one_fade_without_waiting_for_page_readiness() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.status)
            .insert(PresentedAt(Instant::now()));
        fixture.app.update();
        assert!(!fixture.requested());
        fixture
            .app
            .world_mut()
            .get_mut::<Window>(fixture.window)
            .unwrap()
            .visible = true;
        fixture.app.update();
        assert!(fixture.requested());
        fixture.app.update();
        assert!(fixture.requested());
        fixture.acknowledge();
        fixture.app.update();
        assert!(!fixture.requested());
    }

    #[test]
    fn hidden_window_requests_one_forced_dismissal_after_the_native_show_deadline() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.status)
            .insert(PresentedAt(Instant::now() - Duration::from_secs(21)));
        fixture.app.update();
        assert!(fixture.requested());
        fixture.acknowledge();
        fixture.app.update();
        assert!(!fixture.requested());
    }

    #[test]
    fn an_idle_boot_owner_wakes_at_its_native_presentation_deadline() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.status)
            .insert(PresentedAt(Instant::now() - Duration::from_secs(19)));
        fixture.app.update();
        assert!(!fixture.requested());
        fixture.wake.recv_timeout(Duration::from_secs(3)).unwrap();
        fixture.app.update();
        assert!(fixture.requested());
        fixture.acknowledge();
        fixture.app.update();
        assert!(!fixture.requested());
    }

    #[test]
    fn replacement_boot_owner_cannot_inherit_an_old_panel_timestamp() {
        let mut fixture = Fixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.status)
            .insert(PresentedAt(Instant::now() - Duration::from_secs(21)));
        fixture.app.world_mut().despawn(fixture.status);
        fixture.status = fixture.app.world_mut().spawn(SplashStatus::default()).id();
        fixture
            .app
            .world_mut()
            .get_mut::<Window>(fixture.window)
            .unwrap()
            .visible = true;
        fixture.app.update();
        assert!(!fixture.requested());
    }

    #[test]
    fn fake_clock_triggers_forced_dismissal_at_twenty_seconds_without_sleeping() {
        let clock = Clock::fake(1_000);
        let mut fixture = Fixture::new();
        fixture.app.insert_resource(clock.clone());
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.status)
            .insert(PresentedAt(clock.now()));
        fixture.app.update();
        assert!(!fixture.requested());
        clock.advance(Duration::from_secs(19));
        fixture.app.update();
        assert!(!fixture.requested());
        clock.advance(Duration::from_secs(1));
        fixture.app.update();
        assert!(fixture.requested());
    }
}
