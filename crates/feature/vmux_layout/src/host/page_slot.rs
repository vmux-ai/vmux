use bevy::prelude::*;
use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
use vmux_ecs::page::{HostsPage, PageReady};
use vmux_ecs::{PageMetadata, PageOpenId, PagePresented};
use vmux_flex::prelude::Visibility;

use crate::cef::Browser;
use crate::host::deadline::{DeadlineOwner, DeadlineScheduler};
use crate::hosted_page::{
    SurfaceHandoff, SurfaceHandoffFailed, SurfacePaintReady, SurfacePresentationReady,
    SurfaceTransitionReady,
};
use crate::presentation::Projection;

pub struct PageSlotPlugin;

impl Plugin for PageSlotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeadlineScheduler>()
            .init_resource::<ReadinessWake>()
            .add_message::<Ack>()
            .add_observer(queue_wake::<PageReady>)
            .add_observer(queue_wake::<SurfacePaintReady>)
            .add_observer(queue_wake::<SurfacePresentationReady>)
            .add_observer(queue_wake::<PagePresented>)
            .add_observer(queue_wake::<SurfaceHandoffFailed>)
            .add_systems(
                PostUpdate,
                (
                    reconcile,
                    discard_superseded,
                    prepare,
                    bevy::ecs::schedule::ApplyDeferred,
                    refresh_pending,
                    collect,
                    apply,
                    expire,
                    schedule_deadline_wake,
                )
                    .chain()
                    .in_set(PageSlotSet)
                    .before(crate::hosted_page::HandoffCommitSet),
            )
            .add_systems(Last, flush_wake);
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageSlotSet;

#[derive(Resource, Default)]
struct ReadinessWake(bool);

fn queue_wake<T: Component>(_: On<Add, T>, mut pending: ResMut<ReadinessWake>) {
    pending.0 = true;
}

fn flush_wake(mut pending: ResMut<ReadinessWake>, proxy: Option<Res<EventLoopProxyWrapper>>) {
    if pending.0
        && let Some(proxy) = proxy
    {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
    pending.0 = false;
}

#[derive(Component, Clone, Debug, Default)]
pub struct Current {
    surface: Option<Entity>,
    metadata: PageMetadata,
}

impl Current {
    pub fn new(surface: Option<Entity>, metadata: PageMetadata) -> Self {
        Self { surface, metadata }
    }

    pub fn surface(&self) -> Option<Entity> {
        self.surface
    }

    pub fn metadata(&self) -> &PageMetadata {
        &self.metadata
    }
}

#[derive(Component, Clone, Debug)]
pub struct Transition {
    request: PageOpenId,
    incoming: Entity,
    deadline: Option<std::time::Instant>,
    phase: Phase,
    readiness: Readiness,
    previous: Vec<Entity>,
}

impl Transition {
    fn new(request: PageOpenId, incoming: Entity, timeout: std::time::Duration) -> Self {
        let now = std::time::Instant::now();
        Self {
            request,
            incoming,
            deadline: now.checked_add(timeout),
            phase: Phase::Loading,
            readiness: Readiness::default(),
            previous: Vec::new(),
        }
    }

    pub fn request(&self) -> PageOpenId {
        self.request
    }

    pub fn incoming(&self) -> Entity {
        self.incoming
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn matches(&self, surface: Entity, request: PageOpenId) -> bool {
        self.incoming == surface && self.request == request
    }

    fn timed_out(&self) -> bool {
        !self.phase.is_terminal()
            && self
                .deadline
                .is_some_and(|deadline| std::time::Instant::now() >= deadline)
    }

    fn acknowledge(&mut self, kind: AckKind) {
        if self.phase.is_terminal() {
            return;
        }
        match kind {
            AckKind::Loaded => self.readiness.loaded = true,
            AckKind::Painted => self.readiness.painted = true,
            AckKind::Presented => self.readiness.presented = true,
            AckKind::Failed => {
                self.fail();
                return;
            }
        }
        if self.readiness.loaded {
            self.phase.advance_to(Phase::Loaded);
        }
        if self.readiness.loaded && self.readiness.painted {
            self.phase.advance_to(Phase::Painted);
        }
        if self.readiness.loaded && self.readiness.painted && self.readiness.presented {
            self.phase.advance_to(Phase::Presented);
        }
        if self.phase != Phase::Presented {
            return;
        }
        self.phase = Phase::Committed;
        self.deadline = None;
    }

    fn wait_for_explicit_failure(&mut self) {
        self.deadline = None;
    }

    fn fail(&mut self) {
        if self.phase.is_terminal() {
            return;
        }
        self.phase = Phase::Failed;
        self.deadline = None;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Readiness {
    loaded: bool,
    painted: bool,
    presented: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Loading,
    Loaded,
    Painted,
    Presented,
    Committed,
    Failed,
}

impl Phase {
    fn advance_to(&mut self, next: Self) {
        if *self < next && *self != Self::Failed {
            *self = next;
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::Failed)
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SurfaceOpacity(f32);

impl SurfaceOpacity {
    pub fn value(self) -> f32 {
        self.0
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct Ack {
    surface: Entity,
    request: PageOpenId,
    kind: AckKind,
}

impl Ack {
    fn new(surface: Entity, request: PageOpenId, kind: AckKind) -> Self {
        Self {
            surface,
            request,
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum AckKind {
    Loaded,
    Painted,
    Presented,
    Failed,
}

fn prepare(
    handoffs: Query<
        (
            Entity,
            &SurfaceHandoff,
            &ChildOf,
            Has<PageReady>,
            Has<SurfacePaintReady>,
            Has<SurfacePresentationReady>,
            Has<PagePresented>,
            Has<SurfaceHandoffFailed>,
        ),
        Changed<SurfaceHandoff>,
    >,
    currents: Query<&Current>,
    children: Query<&Children>,
    metadata: Query<&PageMetadata>,
    surfaces: Query<(), With<Browser>>,
    hosted: Query<(), With<HostsPage>>,
    presentation: Projection,
    navigations: Query<&vmux_ecs::PageNavigation>,
    mut commands: Commands,
) {
    for (incoming, handoff, parent, ready, painted, native_presented, presented, failed) in
        &handoffs
    {
        let stack = parent.parent();
        if !navigations
            .get(stack)
            .is_ok_and(|navigation| handoff.matches(navigation))
        {
            continue;
        }
        let previous_stack = presentation.retained(stack);
        let current = currents
            .get(stack)
            .ok()
            .and_then(Current::surface)
            .or_else(|| handoff.retain_visible().then_some(incoming))
            .or_else(|| {
                handoff
                    .previous()
                    .iter()
                    .copied()
                    .find(|surface| surfaces.contains(*surface))
            })
            .or_else(|| previous_stack.and_then(|stack| currents.get(stack).ok()?.surface()))
            .or_else(|| {
                let previous = previous_stack?;
                children
                    .get(previous)
                    .ok()?
                    .iter()
                    .find(|surface| surfaces.contains(*surface))
            });
        let pending = metadata
            .get(incoming)
            .ok()
            .cloned()
            .or_else(|| metadata.get(stack).ok().cloned())
            .unwrap_or_default();
        let committed = handoff
            .fallback()
            .cloned()
            .or_else(|| {
                current
                    .filter(|surface| *surface != incoming)
                    .and_then(|surface| metadata.get(surface).ok().cloned())
            })
            .or_else(|| {
                currents
                    .get(stack)
                    .ok()
                    .filter(|current| !current.metadata().url.is_empty())
                    .map(|current| current.metadata().clone())
            })
            .or_else(|| {
                previous_stack.and_then(|stack| {
                    currents
                        .get(stack)
                        .ok()
                        .filter(|current| !current.metadata().url.is_empty())
                        .map(|current| current.metadata().clone())
                })
            })
            .or_else(|| previous_stack.and_then(|stack| metadata.get(stack).ok().cloned()))
            .or_else(|| {
                metadata
                    .get(stack)
                    .ok()
                    .filter(|current| !current.url.is_empty() && current.url != pending.url)
                    .cloned()
            })
            .or_else(|| current.and_then(|surface| metadata.get(surface).ok().cloned()))
            .unwrap_or_default();
        let mut transition = Transition::new(
            handoff.navigation(),
            incoming,
            std::time::Duration::from_secs(10),
        );
        transition.previous = handoff.previous().to_vec();
        if let Some(current) = current
            && current != incoming
            && !transition.previous.contains(&current)
        {
            transition.previous.push(current);
        }
        if failed {
            transition.acknowledge(AckKind::Failed);
        } else {
            if ready {
                transition.acknowledge(AckKind::Loaded);
            }
            if painted {
                transition.acknowledge(AckKind::Painted);
            }
            if surface_presented(hosted.contains(incoming), native_presented, presented) {
                transition.acknowledge(AckKind::Presented);
            }
        }
        commands
            .entity(stack)
            .insert((Current::new(current, committed), transition, pending));
        commands
            .entity(incoming)
            .try_insert(SurfaceOpacity(1.0))
            .try_remove::<SurfaceTransitionReady>();
        if let Some(current) = current
            && current != incoming
        {
            commands.entity(current).try_insert(SurfaceOpacity(1.0));
        }
    }
}

fn surface_presented(hosted: bool, native_presented: bool, presented: bool) -> bool {
    native_presented || (!hosted && presented)
}

fn reconcile(
    mut removed: RemovedComponents<Browser>,
    navigations: Query<&vmux_ecs::PageNavigation>,
    mut stacks: Query<(Entity, &mut Current, Option<&Transition>)>,
    mut commands: Commands,
) {
    let removed = removed.read().collect::<bevy::ecs::entity::EntityHashSet>();
    if removed.is_empty() {
        return;
    }
    for (stack, mut current, transition) in &mut stacks {
        if current
            .surface
            .is_some_and(|surface| removed.contains(&surface))
        {
            current.surface = None;
        }
        let Some(transition) = transition else {
            continue;
        };
        if removed.contains(&transition.incoming) {
            if !navigations
                .get(stack)
                .is_ok_and(|navigation| navigation.matches(transition.request()))
            {
                continue;
            }
            let mut restore = transition.previous.clone();
            if let Some(surface) = current.surface
                && !restore.contains(&surface)
            {
                restore.push(surface);
            }
            for surface in restore {
                if let Ok(mut surface) = commands.get_entity(surface) {
                    surface
                        .try_insert((
                            Visibility::Visible,
                            crate::hosted_page::SurfaceInteractionRestore,
                        ))
                        .try_remove::<(
                            crate::hosted_page::SurfaceBlocked,
                            bevy_cef::prelude::BinIpcBlocked,
                            SurfaceOpacity,
                            crate::hosted_page::RetiredSurface,
                        )>();
                }
            }
            commands
                .entity(stack)
                .try_insert(current.metadata.clone())
                .try_remove::<(vmux_ecs::PageNavigation, Transition)>();
            continue;
        }
    }
}

fn discard_superseded(
    changed: Query<
        (Entity, &Transition, Option<&vmux_ecs::PageNavigation>),
        Or<(Changed<Transition>, Changed<vmux_ecs::PageNavigation>)>,
    >,
    transitions: Query<(), With<Transition>>,
    mut removed: RemovedComponents<vmux_ecs::PageNavigation>,
    mut commands: Commands,
) {
    for (stack, transition, navigation) in &changed {
        if navigation.is_some_and(|navigation| navigation.matches(transition.request())) {
            continue;
        }
        commands.entity(stack).remove::<Transition>();
    }
    for stack in removed.read() {
        if transitions.contains(stack) {
            commands.entity(stack).remove::<Transition>();
        }
    }
}

fn collect(
    handoffs: Query<
        (
            Entity,
            &SurfaceHandoff,
            Option<Ref<PageReady>>,
            Option<Ref<SurfacePaintReady>>,
            Option<Ref<SurfacePresentationReady>>,
            Option<Ref<PagePresented>>,
            Option<Ref<SurfaceHandoffFailed>>,
            Has<HostsPage>,
        ),
        Or<(
            Added<PageReady>,
            Added<SurfacePaintReady>,
            Added<SurfacePresentationReady>,
            Added<PagePresented>,
            Added<SurfaceHandoffFailed>,
        )>,
    >,
    mut acknowledgements: MessageWriter<Ack>,
) {
    for (surface, handoff, ready, painted, native_presented, presented, failed, hosted) in &handoffs
    {
        let failed = failed.is_some_and(|failed| failed.is_added());
        let ready = ready.is_some_and(|ready| ready.is_added());
        let painted = painted.is_some_and(|painted| painted.is_added());
        let native_presented = native_presented.is_some_and(|presented| presented.is_added());
        let presented = presented.is_some_and(|presented| presented.is_added());
        if failed {
            acknowledgements.write(Ack::new(surface, handoff.navigation(), AckKind::Failed));
            continue;
        }
        if ready {
            acknowledgements.write(Ack::new(surface, handoff.navigation(), AckKind::Loaded));
        }
        if painted {
            acknowledgements.write(Ack::new(surface, handoff.navigation(), AckKind::Painted));
        }
        if surface_presented(hosted, native_presented, presented) {
            acknowledgements.write(Ack::new(surface, handoff.navigation(), AckKind::Presented));
        }
    }
}

fn refresh_pending(
    metadata: Query<
        (Entity, &PageMetadata, &ChildOf),
        (
            With<Browser>,
            Without<crate::stack::Stack>,
            Changed<PageMetadata>,
        ),
    >,
    transitions: Query<&Transition, With<crate::stack::Stack>>,
    mut currents: Query<&mut Current, With<crate::stack::Stack>>,
    mut stacks: Query<&mut PageMetadata, With<crate::stack::Stack>>,
) {
    for (surface, pending, parent) in &metadata {
        let stack = parent.parent();
        if let Ok(transition) = transitions.get(stack) {
            if transition.incoming() != surface {
                continue;
            }
        } else {
            let Ok(mut current) = currents.get_mut(stack) else {
                continue;
            };
            if current.surface() != Some(surface) {
                continue;
            }
            if current.metadata != *pending {
                current.metadata.clone_from(pending);
            }
        }
        let Ok(mut projected) = stacks.get_mut(stack) else {
            continue;
        };
        if *projected != *pending {
            projected.clone_from(pending);
        }
    }
}

fn expire(mut transitions: Query<(&Current, &mut Transition)>) {
    for (current, mut transition) in &mut transitions {
        if !transition.timed_out() {
            continue;
        }
        if current.surface().is_none() {
            transition.wait_for_explicit_failure();
            continue;
        }
        transition.fail();
    }
}

fn apply(
    mut acknowledgements: MessageReader<Ack>,
    parents: Query<&ChildOf>,
    navigations: Query<&vmux_ecs::PageNavigation>,
    mut transitions: Query<&mut Transition>,
) {
    for acknowledgement in acknowledgements.read() {
        let Ok(parent) = parents.get(acknowledgement.surface) else {
            continue;
        };
        let stack = parent.parent();
        if !navigations
            .get(stack)
            .is_ok_and(|navigation| navigation.matches(acknowledgement.request))
        {
            continue;
        }
        let Ok(mut transition) = transitions.get_mut(stack) else {
            continue;
        };
        if !transition.matches(acknowledgement.surface, acknowledgement.request) {
            continue;
        }
        transition.acknowledge(acknowledgement.kind);
    }
}

fn schedule_deadline_wake(
    scheduler: Res<DeadlineScheduler>,
    changed: Query<(Entity, &Transition), Changed<Transition>>,
    transitions: Query<(Entity, &Transition)>,
    mut removed: RemovedComponents<Transition>,
    proxy: Option<Res<EventLoopProxyWrapper>>,
) {
    for entity in removed.read() {
        scheduler.cancel(DeadlineOwner::PageSlot(entity));
    }
    if proxy.as_ref().is_some_and(|proxy| proxy.is_added()) {
        for (entity, transition) in &transitions {
            if transition.phase.is_terminal() {
                scheduler.cancel(DeadlineOwner::PageSlot(entity));
                continue;
            }
            let Some(deadline) = transition.deadline else {
                continue;
            };
            scheduler.schedule(DeadlineOwner::PageSlot(entity), deadline, proxy.as_deref());
        }
        return;
    }
    for (entity, transition) in &changed {
        if transition.phase.is_terminal() {
            scheduler.cancel(DeadlineOwner::PageSlot(entity));
            continue;
        }
        let Some(deadline) = transition.deadline else {
            continue;
        };
        scheduler.schedule(DeadlineOwner::PageSlot(entity), deadline, proxy.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_surface_metadata_updates_the_visible_stack_without_navigation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(PageSlotPlugin);
        let metadata = PageMetadata {
            url: "git://repository".to_string(),
            ..Default::default()
        };
        let stack = app.world_mut().spawn(crate::stack::Stack::bundle()).id();
        app.world_mut().entity_mut(stack).insert(metadata.clone());
        let surface = app
            .world_mut()
            .spawn((Browser, metadata.clone(), ChildOf(stack)))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(Current::new(Some(surface), metadata));
        app.update();
        app.world_mut()
            .get_mut::<PageMetadata>(surface)
            .unwrap()
            .url = "git://repository#diff-file".to_string();
        app.update();
        assert_eq!(
            app.world().get::<PageMetadata>(stack).unwrap().url,
            "git://repository#diff-file"
        );
        let current = app.world().get::<Current>(stack).unwrap();
        assert_eq!(current.surface(), Some(surface));
        assert_eq!(current.metadata().url, "git://repository#diff-file");
        assert!(app.world().get::<Transition>(stack).is_none());
    }

    #[test]
    fn prepare_captures_the_live_surface_metadata() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, prepare);
        let request = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::bundle(),
                vmux_ecs::PageNavigation::new(request),
            ))
            .id();
        let current = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "https://current.example/".into(),
                    ..Default::default()
                },
                ChildOf(stack),
            ))
            .id();
        app.world_mut().entity_mut(stack).insert((
            Current::new(
                Some(current),
                PageMetadata {
                    url: "https://stale.example/".into(),
                    ..Default::default()
                },
            ),
            PageMetadata {
                url: "vmux://incoming/".into(),
                ..Default::default()
            },
        ));
        app.world_mut().spawn((
            Browser,
            PageMetadata {
                url: "vmux://incoming/".into(),
                ..Default::default()
            },
            SurfaceHandoff::new(vec![current], request, 0, std::time::Duration::ZERO),
            ChildOf(stack),
        ));

        app.update();

        assert_eq!(
            app.world().get::<Current>(stack).unwrap().metadata().url,
            "https://current.example/"
        );
    }

    #[test]
    fn prepare_seeds_readiness_already_held_by_a_reused_surface() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, prepare);
        let request = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::bundle(),
                vmux_ecs::PageNavigation::new(request),
            ))
            .id();
        let incoming = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://sessions/".into(),
                    ..Default::default()
                },
                PageReady {},
                SurfacePaintReady,
                SurfacePresentationReady,
                SurfaceHandoff::new(Vec::new(), request, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();

        app.update();

        let transition = app.world().get::<Transition>(stack).unwrap();
        assert_eq!(transition.incoming(), incoming);
        assert_eq!(transition.phase(), Phase::Committed);
    }

    #[test]
    fn prepare_prefers_failure_over_success_markers() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, prepare);
        let request = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::bundle(),
                vmux_ecs::PageNavigation::new(request),
            ))
            .id();
        app.world_mut().spawn((
            Browser,
            PageMetadata {
                url: "vmux://sessions/".into(),
                ..Default::default()
            },
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
            SurfaceHandoffFailed,
            SurfaceHandoff::new(Vec::new(), request, 0, std::time::Duration::ZERO),
            ChildOf(stack),
        ));

        app.update();

        assert_eq!(
            app.world().get::<Transition>(stack).unwrap().phase(),
            Phase::Failed
        );
    }

    #[test]
    fn prepare_keeps_a_deadline_before_navigation_starts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, prepare);
        let request = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::bundle(),
                vmux_ecs::PageNavigation::new(request),
            ))
            .id();
        let incoming = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://sessions/".into(),
                    ..Default::default()
                },
                SurfaceHandoff::new(Vec::new(), request, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<Transition>(stack)
                .is_some_and(|transition| transition.deadline.is_some())
        );

        app.world_mut()
            .get_mut::<SurfaceHandoff>(incoming)
            .unwrap()
            .begin_navigation(1, std::time::Duration::ZERO);
        app.update();

        assert!(
            app.world()
                .get::<Transition>(stack)
                .is_some_and(|transition| transition.deadline.is_some())
        );
    }

    #[test]
    fn hosted_surface_waits_for_native_presentation_settle() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, prepare);
        let request = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::bundle(),
                vmux_ecs::PageNavigation::new(request),
            ))
            .id();
        app.world_mut().spawn((
            Browser,
            HostsPage,
            PageMetadata {
                url: "vmux://sessions/".into(),
                ..Default::default()
            },
            PageReady {},
            SurfacePaintReady,
            PagePresented,
            SurfaceHandoff::new(Vec::new(), request, 0, std::time::Duration::ZERO),
            ChildOf(stack),
        ));

        app.update();

        assert_eq!(
            app.world().get::<Transition>(stack).unwrap().phase(),
            Phase::Painted
        );
    }

    #[test]
    fn a_ready_page_commits_in_the_same_frame() {
        let incoming = Entity::from_bits(2);
        let mut transition = Transition::new(PageOpenId::new(), incoming, std::time::Duration::MAX);

        transition.acknowledge(AckKind::Loaded);
        transition.acknowledge(AckKind::Painted);
        transition.acknowledge(AckKind::Presented);

        assert_eq!(transition.phase(), Phase::Committed);
    }

    #[test]
    fn committed_transition_ignores_a_late_failure() {
        let incoming = Entity::from_bits(2);
        let mut transition = Transition::new(PageOpenId::new(), incoming, std::time::Duration::MAX);

        transition.acknowledge(AckKind::Loaded);
        transition.acknowledge(AckKind::Painted);
        transition.acknowledge(AckKind::Presented);
        transition.acknowledge(AckKind::Failed);

        assert_eq!(transition.phase(), Phase::Committed);
    }

    #[test]
    fn failed_transition_never_commits() {
        let incoming = Entity::from_bits(2);
        let mut transition = Transition::new(PageOpenId::new(), incoming, std::time::Duration::MAX);

        transition.acknowledge(AckKind::Loaded);
        transition.acknowledge(AckKind::Failed);

        assert_eq!(transition.phase(), Phase::Failed);
    }

    #[test]
    fn first_surface_survives_a_slow_renderer_start() {
        let mut app = App::new();
        app.add_systems(Update, expire);
        let incoming = app.world_mut().spawn_empty().id();
        let stack = app
            .world_mut()
            .spawn((
                Current::default(),
                Transition::new(PageOpenId::new(), incoming, std::time::Duration::ZERO),
            ))
            .id();

        app.update();

        let transition = app.world().get::<Transition>(stack).unwrap();
        assert_eq!(transition.phase(), Phase::Loading);
        assert!(transition.deadline.is_none());
        assert!(!transition.timed_out());
    }

    #[test]
    fn first_surface_commits_when_the_renderer_becomes_ready_after_timeout() {
        let mut app = App::new();
        app.add_systems(Update, expire);
        let incoming = app.world_mut().spawn_empty().id();
        let stack = app
            .world_mut()
            .spawn((
                Current::default(),
                Transition::new(PageOpenId::new(), incoming, std::time::Duration::ZERO),
            ))
            .id();

        app.update();
        let mut transition = app.world_mut().get_mut::<Transition>(stack).unwrap();
        transition.acknowledge(AckKind::Loaded);
        transition.acknowledge(AckKind::Painted);
        transition.acknowledge(AckKind::Presented);

        assert_eq!(transition.phase(), Phase::Committed);
    }

    #[test]
    fn replacement_surface_still_fails_after_its_deadline() {
        let mut app = App::new();
        app.add_systems(Update, expire);
        let current = app.world_mut().spawn_empty().id();
        let incoming = app.world_mut().spawn_empty().id();
        let stack = app
            .world_mut()
            .spawn((
                Current::new(Some(current), PageMetadata::default()),
                Transition::new(PageOpenId::new(), incoming, std::time::Duration::ZERO),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<Transition>(stack).unwrap().phase(),
            Phase::Failed
        );
    }

    #[test]
    fn stable_slot_forgets_a_surface_removed_by_legacy_code() {
        let mut app = App::new();
        app.add_systems(Update, reconcile);
        let removed = app.world_mut().spawn(Browser).id();
        let stack = app
            .world_mut()
            .spawn(Current::new(Some(removed), PageMetadata::default()))
            .id();
        app.world_mut().entity_mut(removed).despawn();

        app.update();

        assert_eq!(app.world().get::<Current>(stack).unwrap().surface(), None);
    }

    #[test]
    fn removed_incoming_surface_rolls_the_stack_back() {
        let mut app = App::new();
        app.add_systems(Update, reconcile);
        let previous = app
            .world_mut()
            .spawn((
                Browser,
                crate::hosted_page::SurfaceBlocked,
                bevy_cef::prelude::BinIpcBlocked,
            ))
            .id();
        let incoming = app.world_mut().spawn(Browser).id();
        let request = PageOpenId::new();
        let mut transition = Transition::new(request, incoming, std::time::Duration::MAX);
        transition.previous.push(previous);
        let stack = app
            .world_mut()
            .spawn((
                Current::new(Some(previous), PageMetadata::default()),
                vmux_ecs::PageNavigation::new(request),
                transition,
            ))
            .id();
        app.world_mut().entity_mut(incoming).despawn();

        app.update();

        assert!(app.world().get::<Transition>(stack).is_none());
        assert!(app.world().get::<vmux_ecs::PageNavigation>(stack).is_none());
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceInteractionRestore>(previous)
                .is_some()
        );
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceBlocked>(previous)
                .is_none()
        );
    }

    #[test]
    fn removed_superseded_surface_cannot_rollback_the_latest_navigation() {
        let mut app = App::new();
        app.add_systems(
            Update,
            (
                reconcile,
                bevy::ecs::schedule::ApplyDeferred,
                discard_superseded,
                bevy::ecs::schedule::ApplyDeferred,
            )
                .chain(),
        );
        let previous = app
            .world_mut()
            .spawn((
                Browser,
                crate::hosted_page::SurfaceBlocked,
                bevy_cef::prelude::BinIpcBlocked,
            ))
            .id();
        let incoming = app.world_mut().spawn(Browser).id();
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let mut transition = Transition::new(stale, incoming, std::time::Duration::MAX);
        transition.previous.push(previous);
        let stack = app
            .world_mut()
            .spawn((
                Current::new(Some(previous), PageMetadata::default()),
                vmux_ecs::PageNavigation::new(latest),
                transition,
            ))
            .id();
        app.world_mut().entity_mut(incoming).despawn();

        app.update();

        assert!(app.world().get::<Transition>(stack).is_none());
        assert!(
            app.world()
                .get::<vmux_ecs::PageNavigation>(stack)
                .is_some_and(|navigation| navigation.matches(latest))
        );
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceBlocked>(previous)
                .is_some()
        );
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceInteractionRestore>(previous)
                .is_none()
        );
    }

    #[test]
    fn stale_ack_cannot_advance_a_superseded_navigation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Ack>()
            .add_systems(Update, apply);
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn(vmux_ecs::PageNavigation::new(latest))
            .id();
        let surface = app.world_mut().spawn(ChildOf(stack)).id();
        app.world_mut().entity_mut(stack).insert(Transition::new(
            stale,
            surface,
            std::time::Duration::MAX,
        ));
        app.world_mut()
            .resource_mut::<Messages<Ack>>()
            .write(Ack::new(surface, stale, AckKind::Loaded));

        app.update();

        assert_eq!(
            app.world().get::<Transition>(stack).unwrap().phase(),
            Phase::Loading
        );
    }

    #[test]
    fn ack_without_navigation_cannot_advance_a_transition() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Ack>()
            .add_systems(Update, apply);
        let request = PageOpenId::new();
        let stack = app.world_mut().spawn_empty().id();
        let surface = app.world_mut().spawn(ChildOf(stack)).id();
        app.world_mut().entity_mut(stack).insert(Transition::new(
            request,
            surface,
            std::time::Duration::MAX,
        ));
        app.world_mut()
            .resource_mut::<Messages<Ack>>()
            .write(Ack::new(surface, request, AckKind::Loaded));

        app.update();

        assert_eq!(
            app.world().get::<Transition>(stack).unwrap().phase(),
            Phase::Loading
        );
    }

    #[test]
    fn navigation_change_discards_a_superseded_transition() {
        let mut app = App::new();
        app.add_systems(
            Update,
            (discard_superseded, bevy::ecs::schedule::ApplyDeferred).chain(),
        );
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let surface = app.world_mut().spawn(Browser).id();
        let stack = app
            .world_mut()
            .spawn((
                vmux_ecs::PageNavigation::new(latest),
                Transition::new(stale, surface, std::time::Duration::MAX),
            ))
            .id();

        app.update();

        assert!(app.world().get::<Transition>(stack).is_none());
    }
}
