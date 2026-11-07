use std::collections::{HashMap, HashSet};
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
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

impl Plugin for PrewarmPagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DeadlineScheduler>()
            .add_systems(
                Update,
                handle_registered_page_open
                    .in_set(PageOpenSet::HandleKnownPages)
                    .in_set(PrewarmPageOpenSet),
            )
            .add_systems(
                Update,
                maintain_registered_page_pools
                    .after(PrewarmPageOpenSet)
                    .before(CefSystems::CreateAndResize),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrewarmPageOpenSet;

#[derive(Component)]
#[require(vmux_ecs::page::PrewarmedPage)]
pub struct WarmPageSpare {
    url: &'static str,
}

#[derive(Component)]
struct WarmPagePoolNode {
    url: &'static str,
}

#[derive(Default)]
struct WarmPageRetries(HashMap<&'static str, WarmPageRetry>);

struct WarmPageRetry {
    failures: u32,
    after: Duration,
}

impl WarmPageRetries {
    fn failed(&mut self, url: &'static str, now: Duration) {
        let retry = self.0.entry(url).or_insert(WarmPageRetry {
            failures: 0,
            after: now,
        });
        retry.failures = retry.failures.saturating_add(1);
        let shift = retry.failures.saturating_sub(1).min(4);
        retry.after = now + Duration::from_millis(100 * (1_u64 << shift));
    }

    fn waiting(&self, url: &'static str, now: Duration) -> Option<Duration> {
        let retry = self.0.get(url)?;
        (now < retry.after).then_some(retry.after)
    }

    fn any_due(&self, now: Duration) -> bool {
        self.0.values().any(|retry| now >= retry.after)
    }

    fn succeeded(&mut self, url: &'static str) {
        self.0.remove(url);
    }

    fn retain(&mut self, desired: &HashMap<&str, usize>) -> Vec<&'static str> {
        let removed = self
            .0
            .keys()
            .copied()
            .filter(|url| !desired.contains_key(*url))
            .collect::<Vec<_>>();
        self.0.retain(|url, _| desired.contains_key(*url));
        removed
    }
}

#[derive(SystemParam)]
struct WarmPagePoolRetry<'w, 's> {
    time: Res<'w, Time>,
    scheduler: Option<Res<'w, DeadlineScheduler>>,
    proxy: Option<Res<'w, EventLoopProxyWrapper>>,
    retries: Local<'s, WarmPageRetries>,
}

impl WarmPagePoolRetry<'_, '_> {
    fn now(&self) -> Duration {
        self.time.elapsed()
    }

    fn any_due(&self, now: Duration) -> bool {
        self.retries.any_due(now)
    }

    fn failed(&mut self, url: &'static str, now: Duration) {
        self.retries.failed(url, now);
        if let Some(after) = self.retries.waiting(url, now) {
            self.schedule(url, after, now);
        }
    }

    fn waiting(&self, url: &'static str, now: Duration) -> bool {
        let Some(after) = self.retries.waiting(url, now) else {
            return false;
        };
        self.schedule(url, after, now);
        true
    }

    fn succeeded(&mut self, url: &'static str) {
        self.retries.succeeded(url);
        self.cancel(url);
    }

    fn retain(&mut self, desired: &HashMap<&str, usize>) {
        for url in self.retries.retain(desired) {
            self.cancel(url);
        }
    }

    fn schedule(&self, url: &'static str, after: Duration, now: Duration) {
        let Some(scheduler) = self.scheduler.as_deref() else {
            return;
        };
        scheduler.schedule(
            DeadlineOwner::Prewarm(url),
            std::time::Instant::now() + after.saturating_sub(now),
            self.proxy.as_deref(),
        );
    }

    fn cancel(&self, url: &'static str) {
        if let Some(scheduler) = self.scheduler.as_deref() {
            scheduler.cancel(DeadlineOwner::Prewarm(url));
        }
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
    initialized: Local<'s, bool>,
}

impl WarmPagePoolChanges<'_, '_> {
    fn dirty(&mut self, retry_due: bool) -> bool {
        let page_removed = self.removed_pages.read().count() > 0;
        let spare_removed = self.removed_spares.read().count() > 0;
        let dirty = !*self.initialized
            || !self.changed_pages.is_empty()
            || !self.changed_pool_nodes.is_empty()
            || !self.changed_windows.is_empty()
            || !self.changed_layout_ready.is_empty()
            || !self.changed_spares.is_empty()
            || !self.changed_ready.is_empty()
            || !self.changed_failures.is_empty()
            || retry_due
            || page_removed
            || spare_removed;
        *self.initialized = true;
        dirty
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn handle_registered_page_open(
    pages: Query<&PrewarmPage>,
    metadata: PageMetadataCatalog,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    navigations: Query<&PageNavigation>,
    histories: Query<(), With<HostHistory>>,
    spares: Query<
        (Entity, &WarmPageSpare),
        (
            With<PageReady>,
            Without<crate::hosted_page::SurfaceHandoffFailed>,
        ),
    >,
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
    let mut available: HashMap<&str, Vec<Entity>> = HashMap::new();
    for (entity, spare) in &spares {
        available.entry(spare.url).or_default().push(entity);
    }
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
            if let Some(spare) = available.get_mut(page.url).and_then(Vec::pop) {
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

fn maintain_registered_page_pools(
    pages: Query<&PrewarmPage>,
    metadata: PageMetadataCatalog,
    pool_nodes: Query<(Entity, &WarmPagePoolNode)>,
    vmux_windows: Query<(Entity, &HostWindow), With<VmuxWindow>>,
    focused_window: crate::window::FocusedWindow,
    layout_ready: Query<(), (With<LayoutCef>, With<PageReady>)>,
    spares: Query<(
        Entity,
        &WarmPageSpare,
        Has<crate::hosted_page::SurfaceHandoffFailed>,
        Has<PageReady>,
    )>,
    mut retry: WarmPagePoolRetry,
    mut changes: WarmPagePoolChanges,
    mut commands: Commands,
) {
    let now = retry.now();
    if !changes.dirty(retry.any_due(now)) {
        return;
    }
    if layout_ready.is_empty() {
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
    let desired = pages
        .iter()
        .filter(|page| page.pool_size > 0 && !page.url.is_empty())
        .map(|page| (page.url, page.pool_size))
        .collect::<HashMap<_, _>>();
    retry.retain(&desired);
    let mut removed_urls = HashSet::new();
    for (entity, node) in &pool_nodes {
        if desired.get(node.url).copied().unwrap_or(0) == 0 {
            removed_urls.insert(node.url);
            commands.entity(entity).try_despawn();
        }
    }
    let mut retained = HashMap::<&str, usize>::new();
    let failed_urls = spares
        .iter()
        .filter_map(|(_, spare, failed, _)| failed.then_some(spare.url))
        .collect::<HashSet<_>>();
    for (entity, spare, failed, ready) in &spares {
        if failed {
            retry.failed(spare.url, now);
            commands.entity(entity).try_despawn();
            continue;
        }
        if ready && !failed_urls.contains(spare.url) {
            retry.succeeded(spare.url);
        }
        let count = retained.entry(spare.url).or_default();
        let limit = desired.get(spare.url).copied().unwrap_or(0);
        if removed_urls.contains(spare.url) || *count >= limit {
            commands.entity(entity).try_despawn();
            continue;
        }
        *count += 1;
    }
    let mut remaining = 1;
    for page in &pages {
        if page.pool_size == 0 || page.url.is_empty() {
            continue;
        }
        let node = pool_node_for(page.url, window, &pool_nodes, &mut commands);
        let count = retained.get(page.url).copied().unwrap_or(0);
        for _ in count..page.pool_size {
            if retry.waiting(page.url, now) {
                break;
            }
            if remaining == 0 {
                return;
            }
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
    }
}

fn pool_node_for(
    url: &'static str,
    window: Entity,
    pool_nodes: &Query<(Entity, &WarmPagePoolNode)>,
    commands: &mut Commands,
) -> Entity {
    pool_nodes
        .iter()
        .find_map(|(entity, node)| (node.url == url).then_some(entity))
        .unwrap_or_else(|| {
            commands
                .spawn((
                    WarmPagePoolNode { url },
                    Node {
                        width: Val::Px(0.0),
                        height: Val::Px(0.0),
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    Visibility::Hidden,
                    ChildOf(window),
                ))
                .id()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_ecs::page::HostsPage;
    use vmux_ecs::{PageOpenId, PageOpenTask};

    use crate::cef::Browser;

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
            .add_systems(Update, handle_registered_page_open);
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
            .add_systems(Update, handle_registered_page_open);
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
            .add_systems(Update, handle_registered_page_open);
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
            .add_systems(Update, handle_registered_page_open);
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
            .add_systems(Update, handle_registered_page_open);
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
            .add_systems(Update, handle_registered_page_open);
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
            .add_systems(Update, handle_registered_page_open);
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
            .add_systems(Update, maintain_registered_page_pools);
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
            .add_systems(Update, maintain_registered_page_pools);
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
            .add_systems(Update, maintain_registered_page_pools);
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
            .add_systems(Update, maintain_registered_page_pools);
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
}
