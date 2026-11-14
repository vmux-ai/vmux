use std::collections::{HashMap, HashSet};
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::prelude::{Changed, Commands};
use bevy::winit::EventLoopProxyWrapper;
use bevy_cef::prelude::{CefSystems, HostWindow, UiInput};
use vmux_ecs::KeyboardOwner;
use vmux_ecs::page::{HostHistory, PageMetadataCatalog, PageReady, PrewarmPage};
use vmux_ecs::{
    PageMetadata, PageNavigation, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask,
};

use crate::cef::LayoutCef;
use crate::host::deadline::{DeadlineOwner, DeadlineScheduler};
use crate::window::VmuxWindow;
use vmux_flex::prelude::*;

pub struct PrewarmPagesPlugin;

impl bevy::prelude::Plugin for PrewarmPagesPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_resource::<DeadlineScheduler>()
            .add_observer(cancel)
            .add_systems(
                Update,
                open.in_set(PageOpenSet::HandleKnownPages)
                    .in_set(PrewarmPageOpenSet),
            )
            .add_systems(
                Update,
                reconcile
                    .after(PageOpenSet::HandleKnownPages)
                    .after(PrewarmPageOpenSet)
                    .before(CefSystems::CreateAndResize),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrewarmPageOpenSet;

#[derive(Component)]
#[require(vmux_ecs::page::PrewarmedPage, Reservation)]
pub struct WarmPageSpare {
    url: &'static str,
}

#[derive(SystemParam)]
pub struct RendererPool<'w, 's> {
    spares: Query<
        'w,
        's,
        (Entity, &'static WarmPageSpare, &'static mut Reservation),
        (
            With<PageReady>,
            Without<crate::hosted_page::SurfaceHandoffFailed>,
        ),
    >,
    windows: crate::window::WindowHierarchy<'w, 's>,
    requests: Query<'w, 's, &'static PageOpenTask>,
    navigations: Query<'w, 's, &'static PageNavigation>,
    commands: Commands<'w, 's>,
}

impl RendererPool<'_, '_> {
    pub fn take(&mut self, url: &str, request: Entity) -> Option<Entity> {
        let task = self.requests.get(request).ok()?;
        if !self
            .navigations
            .get(task.stack)
            .is_ok_and(|navigation| navigation.matches(task.id))
        {
            return None;
        }
        let window = self.windows.get(task.stack);
        for (entity, spare, mut reservation) in &mut self.spares {
            if spare.url != url || reservation.0.is_some() || self.windows.get(entity) != window {
                continue;
            }
            reservation.0 = Some(request);
            self.commands.entity(entity).remove::<(
                WarmPageSpare,
                Reservation,
                vmux_ecs::page::PrewarmedPage,
                crate::hosted_page::SurfacePaintReady,
                crate::hosted_page::SurfacePresentationReady,
                crate::hosted_page::SurfaceTransitionReady,
                vmux_ecs::PageIdentity,
            )>();
            return Some(entity);
        }
        None
    }
}

#[derive(Component, Default)]
struct Reservation(Option<Entity>);

#[derive(Component)]
#[require(PoolRetry)]
struct WarmPagePoolNode {
    url: &'static str,
}

#[derive(Component, Default)]
struct PoolRetry {
    failures: u32,
    after: Option<Duration>,
}

impl PoolRetry {
    fn failed(&mut self, now: Duration) {
        self.failures = self.failures.saturating_add(1);
        let shift = self.failures.saturating_sub(1).min(4);
        self.after = Some(now + Duration::from_millis(100 * (1_u64 << shift)));
    }

    fn waiting(&self, now: Duration) -> Option<Duration> {
        let after = self.after?;
        (now < after).then_some(after)
    }

    fn due(&self, now: Duration) -> bool {
        self.after.is_some_and(|after| now >= after)
    }

    fn succeeded(&mut self) {
        *self = Self::default();
    }
}

#[derive(SystemParam)]
struct PoolWake<'w> {
    time: Res<'w, Time>,
    scheduler: Option<Res<'w, DeadlineScheduler>>,
    proxy: Option<Res<'w, EventLoopProxyWrapper>>,
}

impl PoolWake<'_> {
    fn now(&self) -> Duration {
        self.time.elapsed()
    }

    fn schedule(&self, pool: Entity, after: Duration, now: Duration) {
        if let Some(scheduler) = self.scheduler.as_deref() {
            scheduler.schedule(
                DeadlineOwner::Prewarm(pool),
                std::time::Instant::now() + after.saturating_sub(now),
                self.proxy.as_deref(),
            );
        }
    }

    fn cancel(&self, pool: Entity) {
        if let Some(scheduler) = self.scheduler.as_deref() {
            scheduler.cancel(DeadlineOwner::Prewarm(pool));
        }
    }
}

fn cancel(removed: On<Remove, WarmPagePoolNode>, scheduler: Option<Res<DeadlineScheduler>>) {
    if let Some(scheduler) = scheduler {
        scheduler.cancel(DeadlineOwner::Prewarm(removed.entity));
    }
}

#[derive(SystemParam)]
struct WarmPagePoolChanges<'w, 's> {
    changed_pages: Query<'w, 's, (), Or<(Added<PrewarmPage>, Changed<PrewarmPage>)>>,
    removed_pages: RemovedComponents<'w, 's, PrewarmPage>,
    changed_pool_nodes: Query<'w, 's, (), Added<WarmPagePoolNode>>,
    changed_windows: Query<'w, 's, (), Added<VmuxWindow>>,
    changed_layout_ready: Query<'w, 's, (), (With<LayoutCef>, Added<PageReady>)>,
    changed_spares: Query<'w, 's, (), Added<WarmPageSpare>>,
    changed_ready: Query<'w, 's, (), (With<WarmPageSpare>, Added<PageReady>)>,
    changed_failures: Query<
        'w,
        's,
        (),
        (
            With<WarmPageSpare>,
            Added<crate::hosted_page::SurfaceHandoffFailed>,
        ),
    >,
    removed_spares: RemovedComponents<'w, 's, WarmPageSpare>,
    removed_pools: RemovedComponents<'w, 's, WarmPagePoolNode>,
}

impl WarmPagePoolChanges<'_, '_> {
    fn dirty(&mut self, retry_due: bool) -> bool {
        let page_removed = self.removed_pages.read().count() > 0;
        let spare_removed = self.removed_spares.read().count() > 0;
        let pool_removed = self.removed_pools.read().count() > 0;
        !self.changed_pages.is_empty()
            || !self.changed_pool_nodes.is_empty()
            || !self.changed_windows.is_empty()
            || !self.changed_layout_ready.is_empty()
            || !self.changed_spares.is_empty()
            || !self.changed_ready.is_empty()
            || !self.changed_failures.is_empty()
            || retry_due
            || page_removed
            || spare_removed
            || pool_removed
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn open(
    pages: Query<&PrewarmPage>,
    metadata: PageMetadataCatalog,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    navigations: Query<&PageNavigation>,
    histories: Query<(), With<HostHistory>>,
    mut renderers: RendererPool,
    children: Query<&Children>,
    mut hosted_pages: ParamSet<(
        Query<
            (
                Entity,
                &'static PageMetadata,
                Has<crate::hosted_page::SurfaceHandoff>,
            ),
            (With<crate::cef::Browser>, With<vmux_ecs::page::HostsPage>),
        >,
        Query<&'static mut PageMetadata>,
        Query<&'static mut crate::hosted_page::SurfaceHandoff>,
    )>,
    time: Res<Time>,
    mut commands: Commands,
) {
    if tasks.is_empty() {
        return;
    }
    let pages = pages.iter().collect::<Vec<_>>();
    let mut handled_stacks = HashSet::new();

    for (entity, task) in &tasks {
        if !navigations
            .get(task.stack)
            .is_ok_and(|navigation| navigation.matches(task.id))
        {
            continue;
        }
        let Some(page) = pages
            .iter()
            .copied()
            .find(|page| page.answers_for(&task.url))
        else {
            continue;
        };
        if handled_stacks.insert((task.stack, page.url)) {
            let next = metadata.metadata_for(&task.url, &page.title);
            let existing = {
                let hosted_browsers = hosted_pages.p0();
                children.get(task.stack).ok().and_then(|children| {
                    children
                        .iter()
                        .filter_map(|child| {
                            let (_, current, pending) = hosted_browsers.get(child).ok()?;
                            page.reuses(&current.url).then_some((child, pending))
                        })
                        .max_by_key(|(_, pending)| *pending)
                })
            };
            if !histories.contains(task.stack) {
                commands.entity(task.stack).insert(HostHistory::default());
            }
            if let Some((existing, pending)) = existing {
                let previous = children
                    .get(task.stack)
                    .map(|children| {
                        children
                            .iter()
                            .filter(|child| *child != existing)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for surface in &previous {
                    commands
                        .entity(*surface)
                        .try_insert(crate::hosted_page::SurfaceBlocked);
                }
                let fallback = if pending {
                    None
                } else {
                    hosted_pages
                        .p1()
                        .get_mut(existing)
                        .ok()
                        .map(|page_metadata| page_metadata.clone())
                };
                if let Ok(mut page_metadata) = hosted_pages.p1().get_mut(existing) {
                    *page_metadata = next;
                }
                if pending {
                    if let Ok(mut handoff) = hosted_pages.p2().get_mut(existing) {
                        handoff.retarget(task.id, time.elapsed());
                    }
                } else {
                    commands.entity(existing).insert(
                        crate::hosted_page::SurfaceHandoff::retaining(
                            previous,
                            task.id,
                            time.elapsed(),
                        )
                        .with_fallback(fallback),
                    );
                }
                commands
                    .entity(existing)
                    .remove::<crate::hosted_page::SurfacePaintReady>()
                    .remove::<crate::hosted_page::SurfacePresentationReady>()
                    .remove::<crate::hosted_page::SurfaceTransitionReady>()
                    .remove::<crate::hosted_page::SurfaceHandoffFailed>()
                    .remove::<crate::PendingWebviewReveal>()
                    .remove::<vmux_ecs::PageIdentity>()
                    .remove::<(
                        crate::hosted_page::SurfaceBlocked,
                        bevy_cef::prelude::BinIpcBlocked,
                    )>();
                commands.trigger(UiInput {
                    webview: existing,
                    payload: PageReady {},
                });
                commands.entity(entity).insert(PageOpenHandled);
                continue;
            }
            let previous = children
                .get(task.stack)
                .map(|children| children.iter().collect::<Vec<_>>())
                .unwrap_or_default();
            for surface in &previous {
                commands
                    .entity(*surface)
                    .try_insert(crate::hosted_page::SurfaceBlocked);
            }
            if let Some(spare) = renderers.take(page.url, entity) {
                if let Ok(mut page_metadata) = hosted_pages.p1().get_mut(spare) {
                    *page_metadata = next.clone();
                }
                commands
                    .entity(spare)
                    .insert((
                        ChildOf(task.stack),
                        KeyboardOwner,
                        Visibility::Hidden,
                        crate::hosted_page::SurfaceHandoff::new(
                            previous,
                            task.id,
                            0,
                            time.elapsed(),
                        ),
                    ))
                    .remove::<crate::hosted_page::SurfacePaintReady>()
                    .remove::<crate::hosted_page::SurfacePresentationReady>()
                    .remove::<vmux_ecs::PageIdentity>()
                    .remove::<(
                        crate::hosted_page::SurfaceBlocked,
                        bevy_cef::prelude::BinIpcBlocked,
                    )>()
                    .remove::<(WarmPageSpare, vmux_ecs::page::PrewarmedPage)>();
                commands.trigger(UiInput {
                    webview: spare,
                    payload: PageReady {},
                });
            } else {
                let webview = commands
                    .spawn((
                        crate::cef::Browser::hosted_page_with_icon(
                            &task.url,
                            &next.title,
                            next.icon.clone(),
                        ),
                        crate::hosted_page::SurfaceHandoff::new(
                            previous,
                            task.id,
                            0,
                            time.elapsed(),
                        ),
                    ))
                    .id();
                commands.entity(webview).insert((
                    ChildOf(task.stack),
                    KeyboardOwner,
                    Visibility::Hidden,
                ));
            }
            commands.entity(entity).insert(PageOpenHandled);
        }
    }
}

fn reconcile(
    pages: Query<&PrewarmPage>,
    metadata: PageMetadataCatalog,
    mut pool_nodes: Query<(Entity, &WarmPagePoolNode, &mut PoolRetry)>,
    vmux_windows: Query<(Entity, &HostWindow), With<VmuxWindow>>,
    focused_window: crate::window::FocusedWindow,
    layout_ready: Query<(), (With<LayoutCef>, With<PageReady>)>,
    spares: Query<(
        Entity,
        &WarmPageSpare,
        Has<crate::hosted_page::SurfaceHandoffFailed>,
        Has<PageReady>,
    )>,
    wake: PoolWake,
    mut changes: WarmPagePoolChanges,
    mut commands: Commands,
) {
    let now = wake.now();
    let retry_due = pool_nodes.iter().any(|(_, _, retry)| retry.due(now));
    if !changes.dirty(retry_due) || layout_ready.is_empty() {
        return;
    }
    let Some(window) = focused_window
        .entity()
        .and_then(|focused| {
            vmux_windows
                .iter()
                .find_map(|(root, host)| (host.0 == focused).then_some(root))
        })
        .or_else(|| vmux_windows.iter().next().map(|(root, _)| root))
    else {
        return;
    };
    let mut desired = HashMap::new();
    for page in &pages {
        if page.pool_size > 0 && !page.url.is_empty() {
            desired.insert(page.url, page.pool_size);
        }
    }
    for (entity, node, _) in &pool_nodes {
        if !desired.contains_key(node.url) {
            wake.cancel(entity);
            commands.entity(entity).try_despawn();
        }
    }
    for (entity, spare, _, _) in &spares {
        if !desired.contains_key(spare.url) {
            commands.entity(entity).try_despawn();
        }
    }
    let mut processed = HashSet::new();
    let mut remaining = 1;
    for page in &pages {
        let Some(&limit) = desired.get(page.url) else {
            continue;
        };
        if !processed.insert(page.url) {
            continue;
        }
        let mut existing = pool_nodes
            .iter_mut()
            .find(|(_, node, _)| node.url == page.url);
        let fresh = existing.is_none();
        let node = match existing.as_ref() {
            Some((entity, _, _)) => *entity,
            None => commands
                .spawn((
                    WarmPagePoolNode { url: page.url },
                    Node {
                        width: Val::Px(0.0),
                        height: Val::Px(0.0),
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    Visibility::Hidden,
                    ChildOf(window),
                ))
                .id(),
        };
        let mut initial = PoolRetry::default();
        let retry = match existing.as_mut() {
            Some((_, _, retry)) => &mut **retry,
            None => &mut initial,
        };
        let failed = spares
            .iter()
            .any(|(_, spare, failed, _)| spare.url == page.url && failed);
        let mut retained = 0;
        for (entity, spare, failure, ready) in &spares {
            if spare.url != page.url {
                continue;
            }
            if failure {
                retry.failed(now);
                commands.entity(entity).try_despawn();
                continue;
            }
            if ready && !failed {
                retry.succeeded();
                wake.cancel(node);
            }
            if retained >= limit {
                commands.entity(entity).try_despawn();
            } else {
                retained += 1;
            }
        }
        if let Some(after) = retry.waiting(now) {
            wake.schedule(node, after, now);
        } else if retained < limit && remaining > 0 {
            remaining -= 1;
            let page_metadata = metadata.metadata_for(page.url, &page.title);
            let webview = commands
                .spawn(crate::cef::Browser::hosted_page_with_icon(
                    page.url,
                    &page_metadata.title,
                    page_metadata.icon,
                ))
                .id();
            commands
                .entity(webview)
                .insert((WarmPageSpare { url: page.url }, ChildOf(node)));
        }
        if fresh {
            commands.entity(node).insert(initial);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use vmux_ecs::page::HostsPage;
    use vmux_ecs::{PageOpenId, PageOpenTask};

    use crate::cef::Browser;

    #[test]
    fn renderer_pool_claims_ready_pages_once_inside_the_target_window() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let window_a = app.world_mut().spawn_empty().id();
        let window_b = app.world_mut().spawn_empty().id();
        let root_a = app.world_mut().spawn(HostWindow(window_a)).id();
        let root_b = app.world_mut().spawn(HostWindow(window_b)).id();
        let stack_a = app.world_mut().spawn(ChildOf(root_a)).id();
        let stack_b = app.world_mut().spawn(ChildOf(root_b)).id();
        let ready_a = app
            .world_mut()
            .spawn((
                WarmPageSpare { url: "file://" },
                PageReady {},
                ChildOf(root_a),
            ))
            .id();
        let ready_b = app
            .world_mut()
            .spawn((
                WarmPageSpare { url: "file://" },
                PageReady {},
                ChildOf(root_b),
            ))
            .id();
        let cold = app
            .world_mut()
            .spawn((WarmPageSpare { url: "file://" }, ChildOf(root_a)))
            .id();
        let mut requests = Vec::new();
        for stack in [stack_a, stack_b] {
            let id = PageOpenId::new();
            app.world_mut()
                .entity_mut(stack)
                .insert(PageNavigation::new(id));
            requests.push(
                app.world_mut()
                    .spawn(PageOpenTask {
                        id,
                        stack,
                        url: "file://".into(),
                        request_id: None,
                    })
                    .id(),
            );
        }
        let claims = app
            .world_mut()
            .run_system_once(move |mut pools: ParamSet<(RendererPool, RendererPool)>| {
                [
                    pools.p0().take("file://", requests[0]),
                    pools.p1().take("file://", requests[0]),
                    pools.p1().take("file://", requests[1]),
                ]
            })
            .unwrap();
        assert_eq!(claims, [Some(ready_a), None, Some(ready_b)]);
        assert!(app.world().get::<WarmPageSpare>(cold).is_some());
    }

    #[test]
    fn renderer_only_pools_leave_page_open_work_to_the_feature() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PrewarmPagesPlugin));
        app.world_mut()
            .spawn(PrewarmPage::renderer("file://", "Code", 2));
        let request = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn(vmux_ecs::PageNavigation::new(request))
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: request,
                stack,
                url: "file:///work/README.md".to_string(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_none());
    }

    #[derive(Resource, Default)]
    struct ReadyEvents(Vec<Entity>);

    fn record_ready(trigger: On<UiInput<PageReady>>, mut events: ResMut<ReadyEvents>) {
        events.0.push(trigger.event().webview);
    }

    #[test]
    fn registered_page_claims_ready_spare() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .add_plugins(MinimalPlugins)
            .add_observer(record_ready)
            .add_plugins(PrewarmPagesPlugin);
        app.world_mut().spawn(PrewarmPage::host(
            "history",
            "vmux://history/",
            "History",
            1,
        ));
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        let spare = app
            .world_mut()
            .spawn((
                WarmPageSpare {
                    url: "vmux://history/",
                },
                PageReady {},
                vmux_ecs::PageIdentity::from("stale history"),
                PageMetadata {
                    url: "vmux://history/".to_string(),
                    ..default()
                },
            ))
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: navigation,
                stack,
                url: "vmux://history/2026-10-23".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert_eq!(
            app.world()
                .get::<ChildOf>(spare)
                .map(|child| child.parent()),
            Some(stack)
        );
        assert!(app.world().get::<WarmPageSpare>(spare).is_none());
        assert!(
            app.world()
                .get::<crate::PendingWebviewReveal>(spare)
                .is_none()
        );
        assert_eq!(
            app.world().get::<Visibility>(spare),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<PageMetadata>(spare).unwrap().url,
            "vmux://history/2026-10-23"
        );
        assert!(app.world().get::<PageMetadata>(stack).is_none());
        assert_eq!(app.world().resource::<ReadyEvents>().0, [spare]);
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        assert!(app.world().get::<HostHistory>(stack).is_some());
        assert!(app.world().get::<vmux_ecs::PageIdentity>(spare).is_none());
        let now = app.world().resource::<Time>().elapsed();
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceHandoff>(spare)
                .is_some_and(|handoff| {
                    handoff.timed_out(now + std::time::Duration::from_secs(10))
                })
        );
    }

    #[test]
    fn claimed_spare_is_not_opened_again_by_the_hosted_page_handler() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_ecs::page::PagePlugin,
            PrewarmPagesPlugin,
            crate::hosted_page::HostedPagePlugin,
        ));
        app.world_mut()
            .spawn(PrewarmPage::host("git", "git://", "Git", 1));
        app.world_mut()
            .spawn(vmux_ecs::page::HostedPage::subtree("git://", "Git"));
        let navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((
                crate::stack::Stack::bundle(),
                PageNavigation::new(navigation),
            ))
            .id();
        let spare = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                crate::hosted_page::Surface::new("git://"),
                WarmPageSpare { url: "git://" },
                PageReady {},
                PageMetadata {
                    url: "git://".to_string(),
                    ..default()
                },
            ))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "git://repository".to_string(),
            request_id: None,
        });

        app.update();

        let children = app.world().get::<Children>(stack).unwrap();
        let surfaces = children
            .iter()
            .filter(|child| app.world().get::<Browser>(*child).is_some())
            .collect::<Vec<_>>();
        assert_eq!(surfaces, [spare]);
    }

    #[test]
    fn registered_page_reuses_an_existing_family_surface_before_a_spare() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .add_plugins(MinimalPlugins)
            .add_observer(record_ready)
            .add_plugins(PrewarmPagesPlugin);
        app.world_mut().spawn(PrewarmPage::host(
            "projects",
            "vmux://projects/",
            "Projects",
            1,
        ));
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        let existing = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                crate::hosted_page::Surface::new("vmux://projects/"),
                PageReady {},
                vmux_ecs::PageIdentity::from("stale project"),
                PageMetadata {
                    url: "vmux://projects/".to_string(),
                    ..default()
                },
                ChildOf(stack),
            ))
            .id();
        let spare = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                crate::hosted_page::Surface::new("vmux://projects/"),
                WarmPageSpare {
                    url: "vmux://projects/",
                },
                PageReady {},
                PageMetadata {
                    url: "vmux://projects/".to_string(),
                    ..default()
                },
            ))
            .id();
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "vmux://projects/github.com/vmux-ai/vmux".to_string(),
            request_id: None,
        });

        app.update();

        assert_eq!(
            app.world().get::<PageMetadata>(existing).unwrap().url,
            "vmux://projects/github.com/vmux-ai/vmux"
        );
        assert!(app.world().get::<WarmPageSpare>(spare).is_some());
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceHandoff>(existing)
                .is_some_and(crate::hosted_page::SurfaceHandoff::retain_visible)
        );
        assert!(
            app.world()
                .get::<vmux_ecs::PageIdentity>(existing)
                .is_none()
        );
        assert_eq!(app.world().resource::<ReadyEvents>().0, [existing]);
    }

    #[test]
    fn registered_page_retargets_a_pending_family_surface() {
        let mut app = App::new();
        app.init_resource::<ReadyEvents>()
            .add_plugins(MinimalPlugins)
            .add_observer(record_ready)
            .add_plugins(PrewarmPagesPlugin);
        app.world_mut()
            .spawn(PrewarmPage::host("git", "git://", "Git", 1));
        let previous_navigation = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn(PageNavigation::new(previous_navigation))
            .id();
        let existing = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                crate::hosted_page::Surface::new("git://"),
                crate::hosted_page::SurfaceHandoff::new(
                    Vec::new(),
                    previous_navigation,
                    0,
                    std::time::Duration::ZERO,
                ),
                crate::hosted_page::SurfacePaintReady,
                crate::hosted_page::SurfacePresentationReady,
                crate::hosted_page::SurfaceTransitionReady,
                PageMetadata {
                    url: "git://repository/main".to_string(),
                    ..default()
                },
                ChildOf(stack),
            ))
            .id();
        let navigation = PageOpenId::new();
        app.world_mut()
            .entity_mut(stack)
            .insert(PageNavigation::new(navigation));
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "git://repository/vmx-226".to_string(),
            request_id: None,
        });

        app.update();

        let browsers = app
            .world()
            .get::<Children>(stack)
            .unwrap()
            .iter()
            .filter(|child| app.world().get::<Browser>(*child).is_some())
            .collect::<Vec<_>>();
        assert_eq!(browsers, [existing]);
        assert_eq!(
            app.world().get::<PageMetadata>(existing).unwrap().url,
            "git://repository/vmx-226"
        );
        let navigation = app.world().get::<PageNavigation>(stack).unwrap();
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceHandoff>(existing)
                .is_some_and(|handoff| handoff.matches(navigation))
        );
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfacePaintReady>(existing)
                .is_none()
        );
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfacePresentationReady>(existing)
                .is_none()
        );
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceTransitionReady>(existing)
                .is_none()
        );
    }

    #[test]
    fn latest_same_frame_registered_open_is_the_only_task_claimed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        app.world_mut()
            .spawn(PrewarmPage::host("git", "git://", "Git", 0));
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
    fn registered_page_without_pool_opens_cold() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        app.world_mut().spawn(PrewarmPage::host(
            "history",
            "vmux://history/",
            "History",
            0,
        ));
        let navigation = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(navigation)).id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: navigation,
                stack,
                url: "vmux://history/2026-10-23".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        let pages = app
            .world_mut()
            .query_filtered::<(&ChildOf, &PageMetadata), (With<Browser>, With<HostsPage>)>()
            .iter(app.world())
            .filter(|(child, _)| child.parent() == stack)
            .map(|(_, page)| page.url.clone())
            .collect::<Vec<_>>();
        assert_eq!(pages, ["vmux://history/2026-10-23"]);
        let surface = app.world().get::<Children>(stack).unwrap()[0];
        assert_eq!(
            app.world().get::<Visibility>(surface),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn exact_prewarm_leaves_descendant_routes_for_their_owner() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        app.world_mut()
            .spawn(PrewarmPage::exact("vmux://sessions/", "Sessions", 1));
        let stack = app.world_mut().spawn_empty().id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "vmux://sessions/session-1".to_string(),
                request_id: None,
            })
            .id();

        app.update();

        assert!(app.world().get::<PageOpenHandled>(task).is_none());
        assert!(app.world().get::<PageMetadata>(stack).is_none());
    }

    #[test]
    fn exact_prewarm_keeps_a_descendant_surface_while_activating_its_spare() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        app.world_mut()
            .spawn(PrewarmPage::exact("vmux://sessions/", "Sessions", 1));
        let stack = app.world_mut().spawn_empty().id();
        let existing = app
            .world_mut()
            .spawn((
                Browser,
                HostsPage,
                crate::hosted_page::Surface::new("vmux://sessions/"),
                PageMetadata {
                    url: "vmux://sessions/session-1".to_string(),
                    ..default()
                },
                ChildOf(stack),
            ))
            .id();
        let spare = app
            .world_mut()
            .spawn((
                WarmPageSpare {
                    url: "vmux://sessions/",
                },
                PageReady {},
                PageMetadata {
                    url: "vmux://sessions/".to_string(),
                    ..default()
                },
            ))
            .id();
        let navigation = PageOpenId::new();
        app.world_mut()
            .entity_mut(stack)
            .insert(PageNavigation::new(navigation));
        app.world_mut().spawn(PageOpenTask {
            id: navigation,
            stack,
            url: "vmux://sessions/".to_string(),
            request_id: None,
        });

        app.update();

        assert_eq!(
            app.world().get::<PageMetadata>(existing).unwrap().url,
            "vmux://sessions/session-1"
        );
        assert!(
            app.world()
                .get::<crate::hosted_page::SurfaceHandoff>(spare)
                .is_some()
        );
        assert_eq!(app.world().get::<ChildOf>(spare).unwrap().parent(), stack);
    }

    #[test]
    fn registered_pools_fill_for_every_page() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        let window = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((VmuxWindow, HostWindow(window)));
        app.world_mut().spawn((LayoutCef, PageReady {}));
        for (host, title) in [("history", "History"), ("lsp", "Language Servers")] {
            app.world_mut().spawn(PrewarmPage::host(
                host,
                if host == "history" {
                    "vmux://history/"
                } else {
                    "vmux://lsp/"
                },
                title,
                1,
            ));
        }

        app.update();
        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<(), (With<WarmPageSpare>, With<HostsPage>)>()
                .iter(app.world())
                .count(),
            2
        );
    }

    #[test]
    fn registered_pool_removes_excess_and_unregistered_spares() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        let window = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((VmuxWindow, HostWindow(window)));
        app.world_mut().spawn((LayoutCef, PageReady {}));
        let page = app
            .world_mut()
            .spawn(PrewarmPage::host(
                "projects",
                "vmux://projects/",
                "Projects",
                2,
            ))
            .id();

        app.update();
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&WarmPageSpare>()
                .iter(app.world())
                .count(),
            2
        );

        app.world_mut().entity_mut(page).insert(PrewarmPage::host(
            "projects",
            "vmux://projects/",
            "Projects",
            1,
        ));
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&WarmPageSpare>()
                .iter(app.world())
                .count(),
            1
        );

        app.world_mut().entity_mut(page).despawn();
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&WarmPageSpare>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query::<&WarmPagePoolNode>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn failed_spare_is_removed_before_replacement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        let window = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((VmuxWindow, HostWindow(window)));
        app.world_mut().spawn((LayoutCef, PageReady {}));
        app.world_mut()
            .spawn(PrewarmPage::host("git", "git://", "Git", 1));

        app.update();
        app.update();
        let spare = app
            .world_mut()
            .query_filtered::<Entity, With<WarmPageSpare>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .entity_mut(spare)
            .insert(crate::hosted_page::SurfaceHandoffFailed);

        app.update();

        assert!(app.world().get_entity(spare).is_err());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<WarmPageSpare>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn ready_spare_does_not_cancel_a_sibling_failure_backoff() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PrewarmPagesPlugin);
        let window = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((VmuxWindow, HostWindow(window)));
        app.world_mut().spawn((LayoutCef, PageReady {}));
        app.world_mut()
            .spawn(PrewarmPage::host("git", "git://", "Git", 2));
        let ready = app
            .world_mut()
            .spawn((Browser, WarmPageSpare { url: "git://" }, PageReady {}))
            .id();
        let failed = app
            .world_mut()
            .spawn((
                Browser,
                WarmPageSpare { url: "git://" },
                crate::hosted_page::SurfaceHandoffFailed,
            ))
            .id();

        app.update();

        assert!(app.world().get_entity(ready).is_ok());
        assert!(app.world().get_entity(failed).is_err());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<WarmPageSpare>>()
                .iter(app.world())
                .count(),
            1
        );
    }
    struct PoolFixture {
        app: App,
        registration: Entity,
    }

    impl PoolFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, PrewarmPagesPlugin))
                .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                    Duration::ZERO,
                ));
            app.world_mut()
                .resource_mut::<Time<bevy::time::Virtual>>()
                .set_max_delta(Duration::from_secs(10));
            let window = app.world_mut().spawn_empty().id();
            app.world_mut().spawn((VmuxWindow, HostWindow(window)));
            app.world_mut().spawn((LayoutCef, PageReady {}));
            let registration = app
                .world_mut()
                .spawn(PrewarmPage::host("git", "git://", "Git", 1))
                .id();
            app.update();
            app.update();
            Self { app, registration }
        }

        fn spare(&mut self) -> Option<Entity> {
            self.app
                .world_mut()
                .query_filtered::<Entity, With<WarmPageSpare>>()
                .iter(self.app.world())
                .next()
        }

        fn fail(&mut self) {
            let spare = self.spare().expect("pool produced no spare");
            self.app
                .world_mut()
                .entity_mut(spare)
                .insert(crate::hosted_page::SurfaceHandoffFailed);
            self.advance(0);
            assert!(self.app.world().get_entity(spare).is_err());
        }

        fn advance(&mut self, milliseconds: u64) {
            self.app
                .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                    Duration::from_millis(milliseconds),
                ));
            self.app.update();
        }

        fn register_again(&mut self) {
            self.app.world_mut().despawn(self.registration);
            self.advance(0);
            self.registration = self
                .app
                .world_mut()
                .spawn(PrewarmPage::host("git", "git://", "Git", 1))
                .id();
            self.advance(0);
        }
    }

    #[test]
    fn pool_replacement_obeys_exponential_backoff_and_caps_the_delay() {
        let mut fixture = PoolFixture::new();
        for delay in [100, 200, 400, 800, 1600, 1600] {
            fixture.fail();
            assert!(fixture.spare().is_none());
            fixture.advance(delay - 1);
            assert!(fixture.spare().is_none());
            fixture.advance(1);
            assert!(fixture.spare().is_some());
        }
    }

    #[test]
    fn recreating_a_pool_drops_its_previous_retry_state() {
        let mut fixture = PoolFixture::new();
        fixture.fail();
        fixture.advance(100);
        fixture.fail();
        fixture.register_again();
        assert!(fixture.spare().is_some());
        fixture.fail();
        fixture.advance(99);
        assert!(fixture.spare().is_none());
        fixture.advance(1);
        assert!(fixture.spare().is_some());
    }

    #[test]
    fn a_stale_request_cannot_reserve_a_ready_renderer() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PrewarmPagesPlugin));
        let current = PageOpenId::new();
        let stack = app.world_mut().spawn(PageNavigation::new(current)).id();
        let stale = app
            .world_mut()
            .spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: "file://".into(),
                request_id: None,
            })
            .id();
        let valid = app
            .world_mut()
            .spawn(PageOpenTask {
                id: current,
                stack,
                url: "file://".into(),
                request_id: None,
            })
            .id();
        let spare = app
            .world_mut()
            .spawn((WarmPageSpare { url: "file://" }, PageReady {}))
            .id();
        let claims = app
            .world_mut()
            .run_system_once(move |mut pool: RendererPool| {
                [pool.take("file://", stale), pool.take("file://", valid)]
            })
            .unwrap();
        assert_eq!(claims, [None, Some(spare)]);
    }

    #[test]
    fn registered_open_keeps_another_windows_ready_spare() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, PrewarmPagesPlugin));
        app.world_mut().spawn(PrewarmPage::host(
            "history",
            "vmux://history/",
            "History",
            1,
        ));
        let a = app.world_mut().spawn_empty().id();
        let b = app.world_mut().spawn_empty().id();
        let root_a = app.world_mut().spawn(HostWindow(a)).id();
        let root_b = app.world_mut().spawn(HostWindow(b)).id();
        let id = PageOpenId::new();
        let stack = app
            .world_mut()
            .spawn((PageNavigation::new(id), ChildOf(root_a)))
            .id();
        let foreign = app
            .world_mut()
            .spawn((
                WarmPageSpare {
                    url: "vmux://history/",
                },
                PageReady {},
                ChildOf(root_b),
            ))
            .id();
        let task = app
            .world_mut()
            .spawn(PageOpenTask {
                id,
                stack,
                url: "vmux://history/recent".into(),
                request_id: None,
            })
            .id();
        app.update();
        assert!(app.world().get::<PageOpenHandled>(task).is_some());
        assert_eq!(
            app.world().get::<ChildOf>(foreign).unwrap().parent(),
            root_b
        );
        assert!(app.world().get::<WarmPageSpare>(foreign).is_some());
        let children = app.world().get::<Children>(stack).unwrap();
        assert!(!children.contains(&foreign));
        assert_eq!(children.len(), 1);
    }
    #[test]
    fn removing_an_empty_retry_pool_replenishes_in_the_remaining_window() {
        let mut fixture = PoolFixture::new();
        let b = fixture.app.world_mut().spawn_empty().id();
        let root_b = fixture
            .app
            .world_mut()
            .spawn((VmuxWindow, HostWindow(b)))
            .id();
        fixture.advance(0);
        fixture.fail();
        let pool = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<WarmPagePoolNode>>()
            .single(fixture.app.world())
            .unwrap();
        let root_a = fixture.app.world().get::<ChildOf>(pool).unwrap().parent();
        fixture.app.world_mut().despawn(root_a);
        fixture.advance(0);
        let replacement = fixture
            .spare()
            .expect("remaining window received no replacement renderer");
        let new_pool = fixture
            .app
            .world()
            .get::<ChildOf>(replacement)
            .unwrap()
            .parent();
        assert_eq!(
            fixture
                .app
                .world()
                .get::<ChildOf>(new_pool)
                .unwrap()
                .parent(),
            root_b
        );
        fixture.fail();
        fixture.advance(99);
        assert!(fixture.spare().is_none());
        fixture.advance(1);
        assert!(fixture.spare().is_some());
    }
    #[test]
    fn shrinking_capacity_does_not_evict_a_renderer_claimed_in_the_same_frame() {
        let mut fixture = PoolFixture::new();
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.registration)
            .insert(PrewarmPage::renderer("git://", "Git", 2));
        fixture.advance(0);
        let spares = fixture
            .app
            .world_mut()
            .query_filtered::<Entity, With<WarmPageSpare>>()
            .iter(fixture.app.world())
            .collect::<Vec<_>>();
        assert_eq!(spares.len(), 2);
        let ready = spares[1];
        let pool = fixture.app.world().get::<ChildOf>(ready).unwrap().parent();
        let root = fixture.app.world().get::<ChildOf>(pool).unwrap().parent();
        fixture
            .app
            .world_mut()
            .entity_mut(ready)
            .insert(PageReady {});
        let id = PageOpenId::new();
        let stack = fixture
            .app
            .world_mut()
            .spawn((PageNavigation::new(id), ChildOf(root)))
            .id();
        let request = fixture
            .app
            .world_mut()
            .spawn(PageOpenTask {
                id,
                stack,
                url: "git://".into(),
                request_id: None,
            })
            .id();
        fixture.app.add_systems(
            Update,
            (move |mut pool: RendererPool, mut commands: Commands| {
                if let Some(renderer) = pool.take("git://", request) {
                    commands.entity(renderer).insert(ChildOf(stack));
                }
            })
            .in_set(PageOpenSet::HandleKnownPages),
        );
        fixture
            .app
            .world_mut()
            .entity_mut(fixture.registration)
            .insert(PrewarmPage::renderer("git://", "Git", 1));
        fixture.advance(0);
        assert!(fixture.app.world().get_entity(ready).is_ok());
        assert_eq!(
            fixture.app.world().get::<ChildOf>(ready).unwrap().parent(),
            stack
        );
        assert!(fixture.app.world().get::<WarmPageSpare>(ready).is_none());
    }
}
