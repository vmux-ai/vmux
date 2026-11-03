use bevy::prelude::*;
use vmux_ecs::page::{
    HostHistory, HostHistoryObserve, HostHistorySet, HostHistoryTraversed, HostsPage, PageManifest,
    PageMetadataCatalog, PageReady,
};
use vmux_ecs::{
    PageMetadata, PageNavigation, PageOpenError, PageOpenHandled, PageOpenId, PageOpenRequest,
    PageOpenSet, PageOpenTarget, PageOpenTask,
};
use vmux_flex::prelude::Visibility;

use vmux_ecs::page::HostedPage;

use crate::cef::Browser;

pub struct HostedUiPlugin<M: Component + Default> {
    manifest: PageManifest,
    marker: std::marker::PhantomData<fn() -> M>,
}

impl<M: Component + Default> HostedUiPlugin<M> {
    pub const fn new(manifest: PageManifest) -> Self {
        Self {
            manifest,
            marker: std::marker::PhantomData,
        }
    }
}

impl<M: Component + Default> Plugin for HostedUiPlugin<M> {
    fn build(&self, app: &mut App) {
        app.insert_resource(HostedUiManifest::<M>::new(self.manifest))
            .add_plugins(
                self.manifest
                    .plugin()
                    .hosted(HostedPage::page(self.manifest.url, self.manifest.title)),
            )
            .add_systems(
                Update,
                sync_hosted_view::<M>.after(PageOpenSet::HandleKnownPages),
            );
    }
}

#[derive(Resource)]
struct HostedUiManifest<M> {
    page: PageManifest,
    marker: std::marker::PhantomData<fn() -> M>,
}

impl<M> HostedUiManifest<M> {
    const fn new(page: PageManifest) -> Self {
        Self {
            page,
            marker: std::marker::PhantomData,
        }
    }
}

fn sync_hosted_view<M: Component + Default>(
    manifest: Res<HostedUiManifest<M>>,
    views: Query<(Entity, &PageMetadata, Has<M>), (With<HostsPage>, Changed<PageMetadata>)>,
    mut commands: Commands,
) {
    for (entity, page, marked) in &views {
        match (manifest.page.answers_for(&page.url), marked) {
            (true, false) => {
                commands.entity(entity).try_insert(M::default());
            }
            (false, true) => {
                commands.entity(entity).remove::<M>();
            }
            _ => {}
        }
    }
}

pub struct HostedPagePlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandoffCommitSet;

#[derive(Component)]
pub(crate) struct Surface(String);

#[derive(Component)]
#[require(vmux_ecs::PageTransition)]
pub struct SurfaceHandoff {
    previous: Vec<Entity>,
    navigation: PageOpenId,
    baseline_navigation_revision: u64,
    navigation_started: bool,
    retain_visible: bool,
    fallback: Option<PageMetadata>,
    started_at: std::time::Duration,
    rollback: bool,
    error_fallback: bool,
}

#[derive(Component)]
pub struct SurfacePaintReady;

#[derive(Component)]
pub struct SurfacePresentationReady;

#[derive(Component)]
pub struct SurfaceTransitionReady;

#[derive(Component)]
pub struct SurfaceHandoffFailed;

#[derive(Component)]
#[require(bevy_cef::prelude::BinIpcBlocked)]
pub struct SurfaceBlocked;

#[derive(Component)]
pub struct SurfaceInteractionRestore;

#[derive(bevy::ecs::system::SystemParam)]
pub struct StackSurface<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    surfaces: Query<'w, 's, (), With<Browser>>,
    handoffs: Query<'w, 's, &'static SurfaceHandoff>,
    navigations: Query<'w, 's, &'static PageNavigation>,
    currents: Query<'w, 's, &'static crate::page_slot::Current>,
    transitions: Query<'w, 's, &'static crate::page_slot::Transition>,
}

impl StackSurface<'_, '_> {
    pub fn current(&self, stack: Entity) -> Option<Entity> {
        let current = self.currents.get(stack).ok()?.surface()?;
        self.surfaces.contains(current).then_some(current)
    }

    pub fn incoming(&self, stack: Entity) -> Option<Entity> {
        let request = self.navigations.get(stack).ok()?.id();
        self.incoming_for(stack, request)
    }

    pub fn incoming_for(&self, stack: Entity, request: PageOpenId) -> Option<Entity> {
        if let Ok(transition) = self.transitions.get(stack) {
            let incoming = transition.incoming();
            if transition.request() == request && self.surfaces.contains(incoming) {
                return Some(incoming);
            }
        }
        let children = self.children.get(stack).ok()?;
        children.iter().find(|child| {
            self.surfaces.contains(*child)
                && self
                    .handoffs
                    .get(*child)
                    .is_ok_and(|handoff| handoff.navigation() == request)
        })
    }

    pub fn is_current(&self, stack: Entity, surface: Entity) -> bool {
        self.current(stack) == Some(surface)
    }

    pub fn is_settled(&self, stack: Entity, surface: Entity) -> bool {
        self.currents.get(stack).is_ok_and(|current| {
            !self.transitions.contains(stack) && current.surface() == Some(surface)
        }) || !self.handoffs.contains(surface) && self.is_current(stack, surface)
    }
}

impl SurfaceHandoff {
    pub fn new(
        previous: Vec<Entity>,
        navigation: PageOpenId,
        baseline_navigation_revision: u64,
        now: std::time::Duration,
    ) -> Self {
        Self {
            previous,
            navigation,
            baseline_navigation_revision,
            navigation_started: false,
            retain_visible: false,
            fallback: None,
            started_at: now,
            rollback: false,
            error_fallback: false,
        }
    }

    pub(crate) fn retaining(
        previous: Vec<Entity>,
        navigation: PageOpenId,
        now: std::time::Duration,
    ) -> Self {
        Self {
            previous,
            navigation,
            baseline_navigation_revision: 0,
            navigation_started: false,
            retain_visible: true,
            fallback: None,
            started_at: now,
            rollback: false,
            error_fallback: false,
        }
    }

    fn rollback(previous: Vec<Entity>, navigation: PageOpenId, now: std::time::Duration) -> Self {
        Self {
            previous,
            navigation,
            baseline_navigation_revision: 0,
            navigation_started: false,
            retain_visible: true,
            fallback: None,
            started_at: now,
            rollback: true,
            error_fallback: false,
        }
    }

    pub fn with_fallback(mut self, fallback: Option<PageMetadata>) -> Self {
        self.fallback = fallback;
        self
    }

    fn with_error_fallback(mut self) -> Self {
        self.error_fallback = true;
        self
    }

    pub fn baseline_navigation_revision(&self) -> u64 {
        self.baseline_navigation_revision
    }

    pub fn navigation_started(&self) -> bool {
        self.navigation_started
    }

    pub fn retain_visible(&self) -> bool {
        self.retain_visible
    }

    pub fn previous(&self) -> &[Entity] {
        &self.previous
    }

    pub fn navigation(&self) -> PageOpenId {
        self.navigation
    }

    pub fn fallback(&self) -> Option<&PageMetadata> {
        self.fallback.as_ref()
    }

    pub fn timed_out(&self, now: std::time::Duration) -> bool {
        now >= self.deadline()
    }

    pub fn deadline(&self) -> std::time::Duration {
        self.started_at
            .saturating_add(std::time::Duration::from_secs(10))
    }

    pub fn defer(&mut self, now: std::time::Duration) {
        if !self.navigation_started {
            self.started_at = now;
        }
    }

    pub fn can_finish(&self, navigation: &PageNavigation, painted_after: bool) -> bool {
        self.navigation_started && self.matches(navigation) && painted_after
    }

    pub fn matches(&self, navigation: &PageNavigation) -> bool {
        navigation.matches(self.navigation)
    }

    pub fn begin_navigation(
        &mut self,
        baseline_navigation_revision: u64,
        now: std::time::Duration,
    ) {
        self.baseline_navigation_revision = baseline_navigation_revision;
        self.navigation_started = true;
        self.started_at = now;
    }

    pub fn retarget(&mut self, navigation: PageOpenId, now: std::time::Duration) {
        self.navigation = navigation;
        self.navigation_started = false;
        self.started_at = now;
    }
}

#[derive(Component)]
pub(crate) struct RetiredSurface;

impl Surface {
    pub(crate) fn new(url: &str) -> Self {
        Self(url.to_string())
    }

    fn belongs_to(&self, page: &HostedPage) -> bool {
        page.answers_for(&self.0)
    }
}

impl Plugin for HostedPagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::page::PagePlugin>() {
            app.add_plugins(vmux_ecs::page::PagePlugin);
        }
        if !app.is_plugin_added::<crate::page_slot::PageSlotPlugin>() {
            app.add_plugins(crate::page_slot::PageSlotPlugin);
        }
        app.add_message::<PageOpenRequest>()
            .add_systems(First, retire_surfaces)
            .add_systems(
                Update,
                (bevy::ecs::schedule::ApplyDeferred, open)
                    .chain()
                    .in_set(PageOpenSet::HandleKnownPages)
                    .after(PageOpenSet::ResolveTarget)
                    .after(crate::warm_page::PrewarmPageOpenSet),
            )
            .add_systems(
                PostUpdate,
                finish_handoffs
                    .in_set(HandoffCommitSet)
                    .run_if(handoff_state_changed)
                    .after(crate::page_slot::PageSlotSet),
            )
            .add_systems(
                PostUpdate,
                bevy::ecs::schedule::ApplyDeferred
                    .after(HandoffCommitSet)
                    .before(crate::presentation::PresentationSettleSet),
            )
            .add_systems(Update, open_traversed_history.in_set(HostHistorySet::Apply))
            .add_systems(Update, record_history.in_set(HostHistorySet::Record));
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handoff_state_changed(
    transitions: Query<(), Changed<crate::page_slot::Transition>>,
    handoffs: Query<(), Changed<SurfaceHandoff>>,
    navigations: Query<(), Changed<PageNavigation>>,
    mut removed_transitions: RemovedComponents<crate::page_slot::Transition>,
    mut removed_navigations: RemovedComponents<PageNavigation>,
) -> bool {
    let transition_removed = removed_transitions.read().count() > 0;
    let navigation_removed = removed_navigations.read().count() > 0;
    !transitions.is_empty()
        || !handoffs.is_empty()
        || !navigations.is_empty()
        || transition_removed
        || navigation_removed
}

fn open(
    pages: Query<&HostedPage>,
    metadata: PageMetadataCatalog,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    navigations: Query<&PageNavigation>,
    histories: Query<(), With<HostHistory>>,
    children: Query<&Children>,
    mut hosted_pages: ParamSet<(
        Query<(&Surface, Has<SurfaceHandoff>), (With<Browser>, With<HostsPage>)>,
        Query<&mut PageMetadata>,
        Query<&mut SurfaceHandoff>,
    )>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let mut opened = std::collections::HashSet::new();

    for (task_entity, task) in &tasks {
        if !navigations
            .get(task.stack)
            .is_ok_and(|navigation| navigation.matches(task.id))
        {
            continue;
        }
        let Some(page) = pages
            .iter()
            .filter_map(|page| {
                page.specificity(&task.url)
                    .map(|specificity| (specificity, page))
            })
            .max_by_key(|(specificity, _)| *specificity)
            .map(|(_, page)| page)
        else {
            continue;
        };
        if opened.insert(task.stack) {
            let next = metadata.metadata_for(&task.url, page.title);
            if !histories.contains(task.stack) {
                commands.entity(task.stack).insert(HostHistory::default());
            }
            let existing = {
                let hosted_browsers = hosted_pages.p0();
                children.get(task.stack).ok().and_then(|children| {
                    children
                        .iter()
                        .filter_map(|child| {
                            let (surface, pending) = hosted_browsers.get(child).ok()?;
                            surface.belongs_to(page).then_some((child, pending))
                        })
                        .max_by_key(|(_, pending)| *pending)
                })
            };
            if let Some((browser, pending)) = existing {
                let previous = children
                    .get(task.stack)
                    .map(|children| {
                        children
                            .iter()
                            .filter(|child| *child != browser)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for surface in &previous {
                    commands.entity(*surface).try_insert(SurfaceBlocked);
                }
                let fallback = if pending {
                    None
                } else {
                    hosted_pages
                        .p1()
                        .get_mut(browser)
                        .ok()
                        .map(|current| current.clone())
                };
                if let Ok(mut current) = hosted_pages.p1().get_mut(browser) {
                    *current = next.clone();
                } else {
                    commands.entity(browser).insert(next);
                }
                if pending {
                    if let Ok(mut handoff) = hosted_pages.p2().get_mut(browser) {
                        handoff.retarget(task.id, time.elapsed());
                    }
                } else {
                    commands.entity(browser).insert(
                        SurfaceHandoff::retaining(previous, task.id, time.elapsed())
                            .with_fallback(fallback),
                    );
                }
                commands
                    .entity(browser)
                    .remove::<SurfacePaintReady>()
                    .remove::<SurfacePresentationReady>()
                    .remove::<SurfaceTransitionReady>()
                    .remove::<SurfaceHandoffFailed>()
                    .remove::<crate::PendingWebviewReveal>()
                    .remove::<vmux_ecs::PageIdentity>()
                    .remove::<(SurfaceBlocked, bevy_cef::prelude::BinIpcBlocked)>();
                commands.trigger(bevy_cef::prelude::UiInput {
                    webview: browser,
                    payload: PageReady {},
                });
            } else {
                let previous = children
                    .get(task.stack)
                    .map(|children| children.iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                for surface in &previous {
                    commands.entity(*surface).try_insert(SurfaceBlocked);
                }
                commands
                    .spawn((
                        Browser::hosted_page_with_icon(&task.url, &next.title, next.icon.clone()),
                        SurfaceHandoff::new(previous, task.id, 0, time.elapsed()),
                        ChildOf(task.stack),
                    ))
                    .insert(Visibility::Hidden);
            }
            commands.entity(task_entity).insert(PageOpenHandled);
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct PreviousSurfaces<'w, 's> {
    transitions: Query<'w, 's, (), With<vmux_ecs::PageTransition>>,
}

impl PreviousSurfaces<'_, '_> {
    fn restore_previous(
        &self,
        handoff: &SurfaceHandoff,
        current: Option<Entity>,
        commands: &mut Commands,
    ) -> bool {
        let mut restored = false;
        for previous in &handoff.previous {
            if self.transitions.contains(*previous) {
                if current != Some(*previous) {
                    commands.entity(*previous).try_despawn();
                }
                continue;
            }
            if let Ok(mut previous) = commands.get_entity(*previous) {
                previous
                    .try_insert((Visibility::Visible, SurfaceInteractionRestore))
                    .try_remove::<(
                        RetiredSurface,
                        SurfaceBlocked,
                        bevy_cef::prelude::BinIpcBlocked,
                    )>();
                restored = true;
            }
        }
        restored
    }
}

fn finish_handoffs(
    handoffs: Query<(Entity, &SurfaceHandoff, &ChildOf)>,
    currents: Query<&crate::page_slot::Current>,
    page_transitions: Query<(Entity, &crate::page_slot::Transition)>,
    navigations: Query<&PageNavigation>,
    metadata: Query<&PageMetadata>,
    identities: Query<&vmux_ecs::PageIdentity>,
    histories: Query<&HostHistory>,
    previous_surfaces: PreviousSurfaces,
    time: Res<Time>,
    mut history_observations: MessageWriter<HostHistoryObserve>,
    mut commands: Commands,
) {
    let mut retained_by_current = std::collections::HashSet::new();
    for (stack, transition) in &page_transitions {
        let current = navigations
            .get(stack)
            .is_ok_and(|navigation| navigation.matches(transition.request()));
        if !current {
            continue;
        }
        let incoming = transition.incoming();
        retained_by_current.insert(incoming);
        if let Ok((_, handoff, _)) = handoffs.get(incoming) {
            retained_by_current.extend(handoff.previous.iter().copied());
        }
    }
    for (entity, handoff, parent) in &handoffs {
        let stack = parent.parent();
        let transition = page_transitions
            .get(stack)
            .ok()
            .map(|(_, transition)| transition);
        let slot_matches =
            transition.is_some_and(|transition| transition.matches(entity, handoff.navigation()));
        let navigation_matches = navigations
            .get(stack)
            .is_ok_and(|navigation| handoff.matches(navigation));
        if slot_matches && !navigation_matches {
            continue;
        }
        let is_current = slot_matches && navigation_matches;
        if !is_current {
            if retained_by_current.contains(&entity) {
                continue;
            }
            for previous in &handoff.previous {
                if retained_by_current.contains(previous) {
                    continue;
                }
                commands
                    .entity(*previous)
                    .try_insert(SurfaceInteractionRestore)
                    .try_remove::<(SurfaceBlocked, bevy_cef::prelude::BinIpcBlocked)>();
            }
            commands.entity(entity).try_despawn();
            continue;
        }
        let Some(transition) = transition else {
            continue;
        };
        let current = currents.get(stack).ok();
        let intent = metadata.get(stack).ok();
        if transition.phase() == crate::page_slot::Phase::Failed {
            if handoff.error_fallback {
                previous_surfaces.restore_previous(
                    handoff,
                    current.and_then(crate::page_slot::Current::surface),
                    &mut commands,
                );
                let mut stack_commands = commands.entity(stack);
                if let Some(current) = current {
                    stack_commands.try_insert(current.metadata().clone());
                }
                stack_commands.try_remove::<(PageNavigation, crate::page_slot::Transition)>();
                commands
                    .entity(entity)
                    .try_remove::<crate::page_slot::SurfaceOpacity>()
                    .try_despawn();
                continue;
            }
            let fallback = current
                .map(|current| current.metadata().clone())
                .filter(|metadata| !metadata.url.is_empty())
                .or_else(|| {
                    handoff
                        .previous
                        .iter()
                        .filter(|previous| !previous_surfaces.transitions.contains(**previous))
                        .find_map(|previous| metadata.get(*previous).ok().cloned())
                });
            if handoff.rollback {
                let failed_url = intent.map(|intent| intent.url.clone()).unwrap_or_default();
                let mut previous = vec![entity];
                for candidate in &handoff.previous {
                    if !previous.contains(candidate) {
                        previous.push(*candidate);
                    }
                }
                spawn_error(&mut commands, stack, previous, failed_url, time.elapsed());
                continue;
            }
            let rollback = if handoff.retain_visible() {
                fallback
                    .clone()
                    .map(|fallback| (entity, fallback, handoff.previous.clone()))
            } else {
                handoff.previous.iter().find_map(|previous| {
                    let (_, previous_handoff, previous_parent) = handoffs.get(*previous).ok()?;
                    if previous_parent.parent() != stack || !previous_handoff.retain_visible() {
                        return None;
                    }
                    let fallback = previous_handoff.fallback.clone()?;
                    let mut rollback_previous = handoff
                        .previous
                        .iter()
                        .copied()
                        .filter(|candidate| candidate != previous)
                        .collect::<Vec<_>>();
                    for candidate in &previous_handoff.previous {
                        if *candidate != *previous && !rollback_previous.contains(candidate) {
                            rollback_previous.push(*candidate);
                        }
                    }
                    Some((*previous, fallback, rollback_previous))
                })
            };
            if let Some((rollback_surface, fallback, rollback_previous)) = rollback {
                let rollback = PageOpenId::new();
                commands.entity(stack).try_insert((
                    fallback.clone(),
                    PageNavigation::new(rollback),
                    crate::page_slot::Current::new(Some(rollback_surface), fallback.clone()),
                ));
                commands
                    .entity(stack)
                    .try_remove::<crate::page_slot::Transition>();
                commands
                    .entity(rollback_surface)
                    .try_insert((
                        fallback,
                        Visibility::Visible,
                        SurfaceHandoff::rollback(rollback_previous, rollback, time.elapsed()),
                    ))
                    .try_remove::<(
                        SurfacePaintReady,
                        SurfacePresentationReady,
                        SurfaceTransitionReady,
                        SurfaceHandoffFailed,
                        crate::PendingWebviewReveal,
                        RetiredSurface,
                        SurfaceBlocked,
                        bevy_cef::prelude::BinIpcBlocked,
                        crate::page_slot::SurfaceOpacity,
                    )>();
                commands.queue_silenced(bevy::ecs::system::command::trigger(
                    bevy_cef::prelude::UiInput {
                        webview: rollback_surface,
                        payload: PageReady {},
                    },
                ));
                if rollback_surface != entity {
                    commands.entity(entity).try_despawn();
                }
                continue;
            }
            let restored = previous_surfaces.restore_previous(
                handoff,
                current.and_then(crate::page_slot::Current::surface),
                &mut commands,
            );
            if restored {
                if let Some(fallback) = fallback {
                    let surface = current.and_then(crate::page_slot::Current::surface);
                    commands.entity(stack).try_insert((
                        fallback.clone(),
                        crate::page_slot::Current::new(surface, fallback),
                    ));
                }
                commands
                    .entity(stack)
                    .try_remove::<(PageNavigation, crate::page_slot::Transition)>();
                commands
                    .entity(entity)
                    .try_remove::<crate::page_slot::SurfaceOpacity>()
                    .try_despawn();
            } else {
                let failed_url = intent.map(|intent| intent.url.clone()).unwrap_or_default();
                let mut previous = vec![entity];
                for candidate in &handoff.previous {
                    if !previous.contains(candidate) {
                        previous.push(*candidate);
                    }
                }
                spawn_error(&mut commands, stack, previous, failed_url, time.elapsed());
            }
            continue;
        }
        if transition.phase() != crate::page_slot::Phase::Committed {
            continue;
        }
        for previous in &handoff.previous {
            if let Ok(mut previous) = commands.get_entity(*previous) {
                previous
                    .try_insert((Visibility::Hidden, RetiredSurface))
                    .try_remove::<crate::page_slot::SurfaceOpacity>();
            }
        }
        if let Some(previous) = current.and_then(crate::page_slot::Current::surface)
            && previous != entity
            && !handoff.previous.contains(&previous)
        {
            commands
                .entity(previous)
                .try_remove::<crate::page_slot::SurfaceOpacity>();
        }
        let pending = intent
            .cloned()
            .or_else(|| metadata.get(entity).ok().cloned())
            .unwrap_or_default();
        commands.entity(stack).try_insert((
            pending.clone(),
            crate::page_slot::Current::new(Some(entity), pending.clone()),
        ));
        commands
            .entity(stack)
            .try_remove::<crate::page_slot::Transition>();
        match identities.get(entity) {
            Ok(identity) => {
                commands.entity(stack).try_insert(identity.clone());
            }
            Err(_) => {
                commands
                    .entity(stack)
                    .try_remove::<vmux_ecs::PageIdentity>();
            }
        }
        if let Ok(mut current) = commands.get_entity(entity) {
            current
                .try_insert((
                    Visibility::Visible,
                    pending.clone(),
                    SurfaceInteractionRestore,
                ))
                .try_remove::<(
                    SurfaceHandoff,
                    SurfacePaintReady,
                    SurfaceTransitionReady,
                    SurfaceHandoffFailed,
                    SurfaceBlocked,
                    bevy_cef::prelude::BinIpcBlocked,
                    crate::PendingWebviewReveal,
                    crate::page_slot::SurfaceOpacity,
                    vmux_ecs::PageTransition,
                )>();
            commands.queue_silenced(bevy::ecs::system::command::trigger(
                vmux_ecs::page::PageActivated::new(entity),
            ));
            if histories
                .get(stack)
                .is_ok_and(|history| !history.showing(&pending.url, 0))
            {
                history_observations.write(HostHistoryObserve {
                    webview: stack,
                    url: pending.url.clone(),
                    top_line: 0,
                });
            }
        }
    }
}

fn spawn_error(
    commands: &mut Commands,
    stack: Entity,
    previous: Vec<Entity>,
    failed_url: String,
    now: std::time::Duration,
) {
    let navigation = PageOpenId::new();
    let title = vmux_ui::i18n::translate(vmux_api::error::FAILED_TO_LOAD);
    for surface in &previous {
        commands.entity(*surface).try_insert(SurfaceBlocked);
    }
    commands
        .entity(stack)
        .insert(PageNavigation::new(navigation));
    commands
        .spawn((
            Browser::hosted_page(crate::ErrorPage::URL, &title),
            vmux_api::error::ErrorPageData {
                title_message_id: vmux_api::error::FAILED_TO_LOAD.to_string(),
                message: String::new(),
                url: failed_url.clone(),
            },
            SurfaceHandoff::new(previous, navigation, 0, now).with_error_fallback(),
            ChildOf(stack),
        ))
        .insert((
            Visibility::Hidden,
            PageMetadata {
                url: failed_url,
                title,
                ..Default::default()
            },
        ));
}

fn retire_surfaces(retired: Query<Entity, With<RetiredSurface>>, mut commands: Commands) {
    for entity in &retired {
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.try_despawn();
        }
    }
}

fn open_traversed_history(
    mut traversed: MessageReader<HostHistoryTraversed>,
    stacks: Query<(), With<crate::stack::Stack>>,
    mut requests: MessageWriter<PageOpenRequest>,
) {
    for event in traversed.read() {
        if !stacks.contains(event.webview) {
            continue;
        }
        requests.write(PageOpenRequest {
            target: PageOpenTarget::Stack(event.webview),
            url: event.entry.url.clone(),
            request_id: None,
        });
    }
}

fn record_history(
    stacks: Query<
        (Entity, &PageMetadata, &HostHistory),
        (With<crate::stack::Stack>, Changed<PageMetadata>),
    >,
    surfaces: StackSurface,
    mut observations: MessageWriter<HostHistoryObserve>,
) {
    for (entity, metadata, history) in &stacks {
        if let Some(surface) = surfaces.current(entity)
            && !surfaces.is_settled(entity, surface)
        {
            continue;
        }
        if vmux_api::VmuxRoute::parse(&metadata.url).is_none() || history.showing(&metadata.url, 0)
        {
            continue;
        }
        observations.write(HostHistoryObserve {
            webview: entity,
            url: metadata.url.clone(),
            top_line: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_api::{BuiltinIcon, PageIcon};
    use vmux_ecs::page::{HostHistoryDelta, HostHistoryStep};
    use vmux_ecs::{PageOpenId, PagePresented};

    use super::*;
    use vmux_ecs::page::HostedPage;

    #[derive(Component, Default)]
    struct HostedView;

    #[test]
    fn hosted_marker_tracks_the_page_that_the_view_currently_hosts() {
        let page = PageManifest {
            route: "vmux://services/",
            url: "vmux://services/",
            asset_host: "services",
            owns_subtree: false,
            title: "Services",
            title_message_id: None,
            replaces_command: None,
            keywords: &[],
            icon: None,
            command_bar: true,
            startup: false,
            reports_title: false,
            placement: vmux_ecs::page::PagePlacement::DEFAULT,
        };
        let mut app = App::new();
        app.insert_resource(HostedUiManifest::<HostedView>::new(page))
            .add_systems(Update, sync_hosted_view::<HostedView>);
        let view = app
            .world_mut()
            .spawn((
                HostsPage,
                PageMetadata {
                    url: "vmux://services/".into(),
                    ..Default::default()
                },
            ))
            .id();

        app.update();
        assert!(app.world().get::<HostedView>(view).is_some());

        app.world_mut().get_mut::<PageMetadata>(view).unwrap().url = "git://".into();
        app.update();
        assert!(app.world().get::<HostedView>(view).is_none());
    }

    #[test]
    fn a_trailing_slash_does_not_decide_which_page_was_asked_for() {
        let page = HostedPage::page("vmux://debug/", "Debug");

        assert!(page.answers_for("vmux://debug/"));
        assert!(page.answers_for("vmux://debug"));
    }

    #[test]
    fn a_longer_url_is_a_different_page() {
        let page = HostedPage::page("vmux://debug/", "Debug");

        assert!(!page.answers_for("vmux://debugger/"));
        assert!(!page.answers_for("vmux://debug/panel"));
    }

    #[test]
    fn a_subtree_page_claims_descendants_without_claiming_a_sibling() {
        let page = HostedPage::subtree("vmux://debug/", "Debug");

        assert!(page.answers_for("vmux://debug/panel"));
        assert!(!page.answers_for("vmux://debugger/"));
    }

    #[test]
    fn matching_ignores_query_and_fragment_without_widening_the_subtree() {
        let page = HostedPage::subtree("vmux://debug/", "Debug");

        assert!(page.answers_for("vmux://debug/?section=input#keyboard"));
        assert!(page.answers_for("vmux://debug/panel?section=input#keyboard"));
        assert!(!page.answers_for("vmux://debugger/?section=input"));
    }

    #[test]
    fn an_exact_page_wins_over_a_parent_subtree() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, open);
        app.world_mut()
            .spawn(HostedPage::subtree("vmux://tools/", "Tools"));
        app.world_mut()
            .spawn(HostedPage::page("vmux://tools/extensions", "Extensions"));
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "vmux://tools/extensions".into(),
            request_id: None,
        });

        app.update();

        let surface = app.world().get::<Children>(stack).unwrap()[0];
        assert_eq!(
            app.world().get::<PageMetadata>(surface).unwrap().title,
            "Extensions"
        );
    }

    #[test]
    fn page_manifest_icon_reaches_opened_stack() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, open);
        app.world_mut().spawn((
            HostedPage::subtree("vmux://simulator/", "Simulator"),
            PageManifest {
                route: "vmux://simulator/",
                url: "vmux://simulator/",
                asset_host: "simulator",
                owns_subtree: true,
                title: "Simulator",
                title_message_id: None,
                replaces_command: None,
                keywords: &[],
                icon: Some(BuiltinIcon::Smartphone),
                command_bar: true,
                startup: false,
                reports_title: false,
                placement: vmux_ecs::page::PagePlacement::DEFAULT,
            },
        ));
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "vmux://simulator/ios/27.0/iPhone%2017%20Pro".into(),
            request_id: None,
        });

        app.update();

        let surface = app.world().get::<Children>(stack).unwrap()[0];
        let metadata = app.world().get::<PageMetadata>(surface).unwrap();
        assert_eq!(metadata.title, "Simulator");
        assert_eq!(metadata.icon, PageIcon::Builtin(BuiltinIcon::Smartphone));
        assert_eq!(metadata.url, "vmux://simulator/ios/27.0/iPhone%2017%20Pro");
    }

    #[test]
    fn hosted_pages_keep_back_forward_history_on_the_stack() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        app.world_mut()
            .spawn(HostedPage::subtree("vmux://projects/", "Projects"));
        let stack = app.world_mut().spawn(crate::stack::Stack::default()).id();
        for url in ["vmux://projects/", "vmux://projects/vmux-cloud"] {
            let navigation = PageOpenId::new();
            app.world_mut()
                .entity_mut(stack)
                .insert(PageNavigation::new(navigation));
            app.world_mut().spawn(PageOpenTask {
                id: navigation,
                stack,
                url: url.to_string(),
                request_id: None,
            });
            app.update();
            let surface = app
                .world()
                .get::<Children>(stack)
                .unwrap()
                .iter()
                .find(|child| app.world().get::<SurfaceHandoff>(*child).is_some())
                .unwrap();
            app.world_mut().entity_mut(surface).insert((
                PageReady {},
                SurfacePaintReady,
                SurfacePresentationReady,
            ));
            app.update();
            app.update();
        }

        let history = app.world().get::<HostHistory>(stack).unwrap();
        assert!(history.can_go_back());
        app.world_mut().write_message(HostHistoryStep {
            webview: stack,
            delta: HostHistoryDelta::Back,
        });
        app.update();

        let requests = app
            .world_mut()
            .resource_mut::<Messages<PageOpenRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "vmux://projects/");
        assert!(matches!(requests[0].target, PageOpenTarget::Stack(entity) if entity == stack));
    }

    #[test]
    fn hosted_navigation_reuses_the_rendering_surface() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        app.world_mut()
            .spawn(HostedPage::subtree("vmux://sessions/", "Sessions"));
        let stack = app.world_mut().spawn(crate::stack::Stack::default()).id();

        let open = |app: &mut App, url: &str| {
            let navigation = PageOpenId::new();
            app.world_mut()
                .entity_mut(stack)
                .insert(PageNavigation::new(navigation));
            app.world_mut().spawn(PageOpenTask {
                id: navigation,
                stack,
                url: url.to_string(),
                request_id: None,
            });
            app.update();
            app.update();
            app.world()
                .get::<Children>(stack)
                .unwrap()
                .iter()
                .find(|child| {
                    app.world().entity(*child).contains::<Browser>()
                        && app
                            .world()
                            .get::<PageMetadata>(*child)
                            .is_some_and(|metadata| metadata.url == url)
                })
                .unwrap()
        };

        let first = open(&mut app, "vmux://sessions/");
        app.world_mut().entity_mut(first).insert((
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
            vmux_ecs::PageIdentity::from("old session"),
        ));
        app.update();
        app.update();
        let second = open(&mut app, "vmux://sessions/codex/session-1");

        let browsers = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .filter(|child| app.world().entity(*child).contains::<Browser>())
            .collect::<Vec<_>>();
        assert_eq!(browsers.len(), 1);
        assert_eq!(first, second);
        assert_eq!(
            app.world().get::<PageMetadata>(browsers[0]).unwrap().url,
            "vmux://sessions/codex/session-1"
        );
        assert!(app.world().get::<vmux_ecs::PageIdentity>(second).is_none());
    }

    #[test]
    fn hosted_navigation_retargets_a_pending_rendering_surface() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        app.world_mut().spawn(HostedPage::subtree("git://", "Git"));
        let stack = app.world_mut().spawn(crate::stack::Stack::bundle()).id();
        for url in ["git://repository/main", "git://repository/vmx-226"] {
            let navigation = PageOpenId::new();
            app.world_mut()
                .entity_mut(stack)
                .insert(PageNavigation::new(navigation));
            app.world_mut().spawn(PageOpenTask {
                id: navigation,
                stack,
                url: url.to_string(),
                request_id: None,
            });
            app.update();
        }

        let browsers = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .filter(|child| app.world().entity(*child).contains::<Browser>())
            .collect::<Vec<_>>();
        assert_eq!(browsers.len(), 1);
        let browser = browsers[0];
        assert_eq!(
            app.world().get::<PageMetadata>(browser).unwrap().url,
            "git://repository/vmx-226"
        );
        let navigation = app.world().get::<PageNavigation>(stack).unwrap();
        assert!(
            app.world()
                .get::<SurfaceHandoff>(browser)
                .is_some_and(|handoff| handoff.matches(navigation))
        );
    }

    #[test]
    fn latest_same_frame_hosted_open_is_the_only_task_claimed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(Update, open);
        app.world_mut().spawn(HostedPage::subtree("git://", "Git"));
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(latest)).id();
        let stale_task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: stale,
                stack,
                url: "git://repository/main".into(),
                request_id: None,
            })
            .id();
        let latest_task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: latest,
                stack,
                url: "git://repository/vmx-226".into(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenHandled>(stale_task).is_none());
        assert!(app.world().get::<PageOpenHandled>(latest_task).is_some());
        let surface = app.world().get::<Children>(stack).unwrap()[0];
        assert_eq!(
            app.world().get::<PageMetadata>(surface).unwrap().url,
            "git://repository/vmx-226"
        );
    }

    #[test]
    fn navigation_between_page_families_replaces_the_rendering_surface() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        app.world_mut()
            .spawn(HostedPage::subtree("vmux://sessions/", "Sessions"));
        app.world_mut()
            .spawn(HostedPage::subtree("vmux://projects/", "Projects"));
        let stack = app.world_mut().spawn(crate::stack::Stack::default()).id();

        let open = |app: &mut App, url: &str| {
            let navigation = PageOpenId::new();
            app.world_mut()
                .entity_mut(stack)
                .insert(PageNavigation::new(navigation));
            app.world_mut().spawn(PageOpenTask {
                id: navigation,
                stack,
                url: url.to_string(),
                request_id: None,
            });
            app.update();
            app.update();
            app.world()
                .get::<Children>(stack)
                .unwrap()
                .iter()
                .find(|child| {
                    app.world().entity(*child).contains::<Browser>()
                        && app
                            .world()
                            .get::<PageMetadata>(*child)
                            .is_some_and(|metadata| metadata.url == url)
                })
                .unwrap()
        };

        let sessions = open(&mut app, "vmux://sessions/");
        app.world_mut().entity_mut(sessions).insert((
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
        ));
        app.update();
        app.update();
        let projects = open(&mut app, "vmux://projects/");

        assert!(app.world().get_entity(sessions).is_ok());
        assert!(app.world().get::<SurfaceHandoff>(projects).is_some());
        app.world_mut()
            .entity_mut(projects)
            .insert((PageReady {}, crate::PendingWebviewReveal::pending()));
        app.update();

        assert!(app.world().get_entity(sessions).is_ok());
        assert!(app.world().get::<SurfaceHandoff>(projects).is_some());

        app.world_mut()
            .entity_mut(projects)
            .insert(SurfacePaintReady);
        app.update();

        assert!(app.world().get_entity(sessions).is_ok());
        assert!(app.world().get::<SurfaceHandoff>(projects).is_some());

        app.world_mut()
            .entity_mut(projects)
            .insert(SurfacePresentationReady);
        app.update();
        app.update();

        assert_ne!(sessions, projects);
        assert!(
            app.world()
                .get::<SurfacePresentationReady>(projects)
                .is_some()
        );
        assert_eq!(
            app.world()
                .get::<Children>(stack)
                .unwrap()
                .iter()
                .filter(|child| app.world().entity(*child).contains::<Browser>())
                .count(),
            1
        );
    }

    #[test]
    fn a_stale_handoff_cannot_replace_the_latest_navigation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((crate::stack::Stack::default(), PageNavigation::new(latest)))
            .id();
        let previous = app.world_mut().spawn(ChildOf(stack)).id();
        let incoming = app
            .world_mut()
            .spawn((
                SurfaceHandoff::new(vec![previous], stale, 0, std::time::Duration::ZERO),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(previous).is_ok());
        assert!(app.world().get_entity(incoming).is_err());
    }

    #[test]
    fn a_superseded_ready_handoff_cannot_commit_over_the_latest_navigation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((crate::stack::Stack::bundle(), PageNavigation::new(stale)))
            .id();
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "vmux://projects/old".into(),
            ..Default::default()
        });
        let previous = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                PageReady {},
                PagePresented,
                PageMetadata {
                    url: "vmux://projects/old".into(),
                    ..Default::default()
                },
                ChildOf(stack),
            ))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(crate::page_slot::Current::new(
                Some(previous),
                PageMetadata {
                    url: "vmux://projects/old".into(),
                    ..Default::default()
                },
            ));
        let incoming = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                PageReady {},
                SurfacePaintReady,
                PageMetadata {
                    url: "vmux://projects/stale".into(),
                    ..Default::default()
                },
                SurfaceHandoff::new(vec![previous], stale, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();

        app.update();
        app.world_mut()
            .entity_mut(stack)
            .insert(PageNavigation::new(latest));
        app.world_mut()
            .entity_mut(incoming)
            .insert(SurfacePresentationReady);
        app.update();

        assert_eq!(
            app.world().get::<PageMetadata>(stack).unwrap().url,
            "vmux://projects/stale"
        );
        assert!(app.world().get_entity(previous).is_ok());
        assert!(app.world().get_entity(incoming).is_err());
    }

    #[test]
    fn a_failed_first_surface_falls_back_to_the_error_page() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageNavigation::new(navigation),
                PageMetadata {
                    url: "vmux://projects/missing".into(),
                    title: "Missing".into(),
                    ..Default::default()
                },
            ))
            .id();
        let failed = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::new(Vec::new(), navigation, 0, std::time::Duration::ZERO),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();
        app.update();

        assert!(app.world().get_entity(failed).is_ok());
        let error = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .find(|child| {
                app.world()
                    .get::<vmux_api::error::ErrorPageData>(*child)
                    .is_some()
            })
            .unwrap();
        assert_eq!(
            app.world()
                .get::<vmux_api::error::ErrorPageData>(error)
                .unwrap()
                .url,
            "vmux://projects/missing"
        );
        assert!(app.world().get::<PageNavigation>(stack).is_some());
    }

    #[test]
    fn stack_surface_keeps_current_and_incoming_distinct() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::page_slot::PageSlotPlugin));
        let latest = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((PageNavigation::new(latest), PageMetadata::default()))
            .id();
        let previous = app.world_mut().spawn((Browser, ChildOf(stack))).id();
        app.world_mut()
            .entity_mut(stack)
            .insert(crate::page_slot::Current::new(
                Some(previous),
                PageMetadata::default(),
            ));
        let incoming = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://projects/".into(),
                    ..Default::default()
                },
                SurfaceHandoff::new(Vec::new(), latest, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();
        app.update();

        let (current, pending, previous_is_current, incoming_is_settled) = app
            .world_mut()
            .run_system_once(move |surfaces: StackSurface| {
                (
                    surfaces.current(stack),
                    surfaces.incoming(stack),
                    surfaces.is_current(stack, previous),
                    surfaces.is_settled(stack, incoming),
                )
            })
            .unwrap();

        assert_eq!(current, Some(previous));
        assert_eq!(pending, Some(incoming));
        assert!(previous_is_current);
        assert!(!incoming_is_settled);
    }

    #[test]
    fn first_surface_does_not_wait_for_a_replacement_handoff() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageNavigation::new(navigation),
            ))
            .id();
        let surface = app
            .world_mut()
            .spawn((
                Browser,
                PageReady {},
                SurfacePaintReady,
                SurfacePresentationReady,
                SurfaceHandoff::new(Vec::new(), navigation, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get::<SurfaceHandoff>(surface).is_none());
    }

    #[test]
    fn stack_metadata_updates_before_surface_presentation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        app.world_mut()
            .spawn(HostedPage::subtree("vmux://projects/", "Projects"));
        let previous = PageMetadata {
            url: "vmux://sessions/".into(),
            title: "Sessions".into(),
            ..Default::default()
        };
        let visible_stack = app.world_mut().spawn_empty().id();
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                crate::presentation::Transition::retain(visible_stack),
                previous.clone(),
                PageNavigation::new(navigation),
            ))
            .id();
        app.world_mut().spawn((
            Browser,
            HostsPage,
            Surface::new("vmux://sessions/"),
            previous.clone(),
            ChildOf(stack),
        ));
        app.world_mut().entity_mut(stack).insert(PageMetadata {
            url: "vmux://projects/vmux-cloud".into(),
            title: "Projects".into(),
            ..Default::default()
        });
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "vmux://projects/vmux-cloud".into(),
            request_id: None,
        });

        app.update();

        assert_eq!(
            app.world().get::<PageMetadata>(stack).unwrap().url,
            "vmux://projects/vmux-cloud"
        );
        let current = app.world().get::<crate::page_slot::Current>(stack).unwrap();
        assert_eq!(current.metadata(), &previous);
        let incoming = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .find(|child| app.world().get::<SurfaceHandoff>(*child).is_some())
            .unwrap();
        assert_eq!(
            app.world().get::<PageMetadata>(incoming).unwrap().url,
            "vmux://projects/vmux-cloud"
        );

        app.world_mut().entity_mut(incoming).insert((
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
        ));
        app.update();
        app.update();

        assert_eq!(
            app.world().get::<PageMetadata>(stack).unwrap().url,
            "vmux://projects/vmux-cloud"
        );
        assert_eq!(
            app.world().get::<crate::presentation::Transition>(stack),
            Some(&crate::presentation::Transition::retain(visible_stack))
        );
    }

    #[test]
    fn a_failed_retained_handoff_repaints_the_previous_page_before_release() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let navigation = PageOpenId::new();
        let previous = PageMetadata {
            url: "vmux://sessions/old".into(),
            title: "Old".into(),
            ..Default::default()
        };
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageMetadata {
                    url: "vmux://projects/new".into(),
                    title: "New".into(),
                    ..Default::default()
                },
                PageNavigation::new(navigation),
            ))
            .id();
        let surface = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://projects/new".into(),
                    title: "New".into(),
                    ..Default::default()
                },
                SurfaceHandoff::retaining(Vec::new(), navigation, std::time::Duration::ZERO)
                    .with_fallback(Some(previous.clone())),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert_eq!(app.world().get::<PageMetadata>(stack), Some(&previous));
        assert_eq!(app.world().get::<PageMetadata>(surface), Some(&previous));
        assert!(app.world().get::<SurfaceHandoff>(surface).is_some());
        assert!(
            app.world()
                .get::<vmux_ecs::PageTransition>(surface)
                .is_some()
        );

        app.world_mut().entity_mut(surface).insert((
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
        ));
        app.update();
        app.update();

        assert!(app.world().get::<SurfaceHandoff>(surface).is_none());
        assert!(
            app.world()
                .get::<vmux_ecs::PageTransition>(surface)
                .is_none()
        );
    }

    #[test]
    fn a_handoff_finishes_only_after_its_current_navigation_paints() {
        let current = PageOpenId::new();
        let stale = PageOpenId::new();
        let navigation = PageNavigation::new(current);
        let mut handoff = SurfaceHandoff::new(Vec::new(), current, 1, std::time::Duration::ZERO);

        assert!(!handoff.can_finish(&navigation, true));
        handoff.begin_navigation(1, std::time::Duration::ZERO);
        assert!(!handoff.can_finish(&navigation, false));
        assert!(handoff.can_finish(&navigation, true));

        let mut stale_handoff =
            SurfaceHandoff::new(Vec::new(), stale, 1, std::time::Duration::ZERO);
        stale_handoff.begin_navigation(1, std::time::Duration::ZERO);
        assert!(!stale_handoff.can_finish(&navigation, true));
    }

    #[test]
    fn a_handoff_deadline_starts_immediately_and_resets_with_navigation() {
        let navigation = PageOpenId::new();
        let mut handoff = SurfaceHandoff::new(
            Vec::new(),
            navigation,
            0,
            std::time::Duration::from_secs(30),
        );

        assert!(!handoff.timed_out(std::time::Duration::from_secs(39)));
        assert!(handoff.timed_out(std::time::Duration::from_secs(40)));

        handoff.begin_navigation(0, std::time::Duration::from_secs(50));

        assert!(!handoff.timed_out(std::time::Duration::from_secs(59)));
        assert!(handoff.timed_out(std::time::Duration::from_secs(60)));
    }

    #[test]
    fn a_deferred_handoff_gets_a_fresh_startup_deadline() {
        let navigation = PageOpenId::new();
        let mut handoff = SurfaceHandoff::new(Vec::new(), navigation, 0, std::time::Duration::ZERO);

        handoff.defer(std::time::Duration::from_secs(30));

        assert!(!handoff.timed_out(std::time::Duration::from_secs(39)));
        assert!(handoff.timed_out(std::time::Duration::from_secs(40)));
    }

    #[test]
    fn a_retargeted_handoff_waits_for_the_latest_navigation() {
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let mut handoff = SurfaceHandoff::new(Vec::new(), stale, 0, std::time::Duration::ZERO);
        handoff.begin_navigation(2, std::time::Duration::from_secs(1));

        handoff.retarget(latest, std::time::Duration::from_secs(2));

        assert!(!handoff.navigation_started());
        assert!(!handoff.matches(&PageNavigation::new(stale)));
        assert!(handoff.matches(&PageNavigation::new(latest)));
        assert!(!handoff.timed_out(std::time::Duration::from_secs(11)));
    }

    #[test]
    fn a_stale_retained_surface_stays_owned_until_the_latest_navigation_paints() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((crate::stack::Stack::default(), PageNavigation::new(latest)))
            .id();
        let retained = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::retaining(Vec::new(), stale, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();
        let incoming = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::new(vec![retained], latest, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(retained).is_ok());
        assert!(app.world().get::<SurfaceHandoff>(retained).is_some());
        assert!(app.world().get::<SurfaceHandoff>(incoming).is_some());

        app.world_mut().entity_mut(incoming).insert((
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
        ));
        app.update();
        app.update();

        assert!(app.world().get_entity(retained).is_err());
        assert!(app.world().get_entity(incoming).is_ok());
        assert!(app.world().get::<SurfaceHandoff>(incoming).is_none());
    }

    #[test]
    fn stale_handoff_cleanup_does_not_unblock_the_current_retained_surface() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((crate::stack::Stack::default(), PageNavigation::new(latest)))
            .id();
        let retained = app
            .world_mut()
            .spawn((
                SurfaceHandoff::retaining(Vec::new(), latest, std::time::Duration::ZERO),
                SurfaceBlocked,
                bevy_cef::prelude::BinIpcBlocked,
                ChildOf(stack),
            ))
            .id();
        let stale_surface = app
            .world_mut()
            .spawn((
                SurfaceHandoff::new(vec![retained], stale, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(stale_surface).is_err());
        assert!(app.world().get::<SurfaceBlocked>(retained).is_some());
        assert!(
            app.world()
                .get::<bevy_cef::prelude::BinIpcBlocked>(retained)
                .is_some()
        );
    }

    #[test]
    fn a_failed_latest_navigation_rolls_the_original_retained_surface_back() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let stale = PageOpenId::new();
        let latest = PageOpenId::new();
        let original = PageMetadata {
            url: "vmux://sessions/old".into(),
            title: "Old".into(),
            ..Default::default()
        };
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageMetadata {
                    url: "vmux://projects/new".into(),
                    title: "New".into(),
                    ..Default::default()
                },
                PageNavigation::new(latest),
            ))
            .id();
        let retained = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://sessions/pending".into(),
                    title: "Pending".into(),
                    ..Default::default()
                },
                SurfaceHandoff::retaining(Vec::new(), stale, std::time::Duration::ZERO)
                    .with_fallback(Some(original.clone())),
                ChildOf(stack),
            ))
            .id();
        let incoming = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::new(vec![retained], latest, 0, std::time::Duration::ZERO),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(incoming).is_err());
        assert_eq!(app.world().get::<PageMetadata>(stack), Some(&original));
        assert_eq!(app.world().get::<PageMetadata>(retained), Some(&original));
        let rollback = app.world().get::<PageNavigation>(stack).unwrap();
        assert!(
            app.world()
                .get::<SurfaceHandoff>(retained)
                .is_some_and(|handoff| rollback.matches(handoff.navigation))
        );

        app.world_mut().entity_mut(retained).insert((
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
        ));
        app.update();
        app.update();

        assert!(app.world().get::<SurfaceHandoff>(retained).is_none());
    }

    #[test]
    fn a_failed_rollback_falls_back_to_the_error_page() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageNavigation::new(navigation),
            ))
            .id();
        let surface = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::rollback(Vec::new(), navigation, std::time::Duration::ZERO),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(surface).is_ok());
        assert!(app.world().get::<PageNavigation>(stack).is_some());
        let error = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .find(|child| {
                app.world()
                    .get::<vmux_api::error::ErrorPageData>(*child)
                    .is_some()
            });
        assert!(error.is_some());
        assert!(app.world().get::<SurfaceHandoff>(error.unwrap()).is_some());
    }

    #[test]
    fn a_failed_error_page_does_not_spawn_another_error_page() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageNavigation::new(navigation),
            ))
            .id();
        let error = app
            .world_mut()
            .spawn((
                Browser,
                vmux_api::error::ErrorPageData {
                    title_message_id: vmux_api::error::FAILED_TO_LOAD.to_string(),
                    message: String::new(),
                    url: "vmux://projects/missing".into(),
                },
                SurfaceHandoff::new(Vec::new(), navigation, 0, std::time::Duration::ZERO)
                    .with_error_fallback(),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();
        app.update();

        assert!(app.world().get_entity(error).is_err());
        assert!(app.world().get::<PageNavigation>(stack).is_none());
        assert_eq!(
            app.world()
                .get::<Children>(stack)
                .map(|children| {
                    children
                        .iter()
                        .filter(|child| {
                            app.world()
                                .get::<vmux_api::error::ErrorPageData>(*child)
                                .is_some()
                        })
                        .count()
                })
                .unwrap_or_default(),
            0
        );
    }

    #[test]
    fn removing_slot_state_releases_a_stale_handoff() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageNavigation::new(navigation),
            ))
            .id();
        let surface = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::new(Vec::new(), navigation, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<crate::page_slot::Transition>(stack)
                .is_some()
        );
        app.world_mut()
            .entity_mut(stack)
            .remove::<(PageNavigation, crate::page_slot::Transition)>();
        app.update();

        assert!(app.world().get_entity(surface).is_err());
    }

    #[test]
    fn failed_third_navigation_discards_the_superseded_second_surface() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let second_navigation = PageOpenId::new();
        let third_navigation = PageOpenId::new();
        let first_metadata = PageMetadata {
            url: "vmux://sessions/first".into(),
            title: "First".into(),
            ..Default::default()
        };
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageMetadata {
                    url: "vmux://projects/third".into(),
                    title: "Third".into(),
                    ..Default::default()
                },
                PageNavigation::new(third_navigation),
            ))
            .id();
        let first = app
            .world_mut()
            .spawn((
                Browser,
                first_metadata.clone(),
                Visibility::Visible,
                ChildOf(stack),
            ))
            .id();
        app.world_mut()
            .entity_mut(stack)
            .insert(crate::page_slot::Current::new(Some(first), first_metadata));
        let second = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://projects/second".into(),
                    title: "Second".into(),
                    ..Default::default()
                },
                SurfaceHandoff::new(vec![first], second_navigation, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();
        let third = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::new(
                    vec![first, second],
                    third_navigation,
                    0,
                    std::time::Duration::ZERO,
                ),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(first).is_ok());
        assert!(app.world().get_entity(second).is_err());
        assert!(app.world().get_entity(third).is_err());
        assert!(app.world().get::<PageNavigation>(stack).is_none());
        assert!(
            app.world()
                .get::<crate::page_slot::Transition>(stack)
                .is_none()
        );
    }

    #[test]
    fn consecutive_failed_navigation_keeps_a_surface_until_error_paints() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_ecs::page::PagePlugin, HostedPagePlugin))
            .add_message::<PageOpenRequest>();
        let first_navigation = PageOpenId::new();
        let second_navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::default(),
                PageNavigation::new(second_navigation),
                PageMetadata {
                    url: "vmux://projects/second".into(),
                    title: "Second".into(),
                    ..Default::default()
                },
            ))
            .id();
        let first = app
            .world_mut()
            .spawn((
                Browser,
                PageMetadata {
                    url: "vmux://projects/first".into(),
                    title: "First".into(),
                    ..Default::default()
                },
                SurfaceHandoff::new(Vec::new(), first_navigation, 0, std::time::Duration::ZERO),
                ChildOf(stack),
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                Browser,
                SurfaceHandoff::new(vec![first], second_navigation, 0, std::time::Duration::ZERO),
                SurfaceHandoffFailed,
                ChildOf(stack),
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(first).is_ok());
        assert!(app.world().get_entity(second).is_ok());
        let error = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .find(|child| {
                app.world()
                    .get::<vmux_api::error::ErrorPageData>(*child)
                    .is_some()
            })
            .unwrap();

        app.world_mut().entity_mut(error).insert((
            PageReady {},
            SurfacePaintReady,
            SurfacePresentationReady,
        ));
        app.update();
        app.update();

        assert!(app.world().get_entity(first).is_err());
        assert!(app.world().get_entity(second).is_err());
        assert!(app.world().get_entity(error).is_ok());
    }
}
