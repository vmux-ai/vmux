use bevy::prelude::*;
use vmux_ecs::page::{
    HostHistory, HostHistoryObserve, HostHistorySet, HostHistoryTraversed, HostsPage, PageManifest,
};
use vmux_ecs::{
    PageMetadata, PageOpenError, PageOpenHandled, PageOpenRequest, PageOpenSet, PageOpenTarget,
    PageOpenTask,
};

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

#[derive(Component)]
pub(crate) struct Surface(String);

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
        app.add_message::<PageOpenRequest>()
            .add_systems(
                Update,
                open.in_set(PageOpenSet::HandleKnownPages)
                    .after(PageOpenSet::ResolveTarget),
            )
            .add_systems(Update, open_traversed_history.in_set(HostHistorySet::Apply))
            .add_systems(Update, record_history.in_set(HostHistorySet::Record));
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn open(
    pages: Query<(&HostedPage, Option<&PageManifest>)>,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
    histories: Query<(), With<HostHistory>>,
    children: Query<&Children>,
    hosted_browsers: Query<&Surface, (With<Browser>, With<HostsPage>)>,
    mut commands: Commands,
) {
    let mut opened = std::collections::HashSet::new();

    for (task_entity, task) in &tasks {
        let Some((page, manifest)) = pages
            .iter()
            .filter_map(|candidate @ (page, _)| {
                page.specificity(&task.url)
                    .map(|specificity| (specificity, candidate))
            })
            .max_by_key(|(specificity, _)| *specificity)
            .map(|(_, candidate)| candidate)
        else {
            continue;
        };
        if opened.insert(task.stack) {
            let metadata = manifest.map_or_else(
                || PageMetadata {
                    url: task.url.clone(),
                    title: page.title.to_string(),
                    ..default()
                },
                |manifest| manifest.metadata_for(&task.url),
            );
            commands.entity(task.stack).insert(metadata.clone());
            if !histories.contains(task.stack) {
                commands.entity(task.stack).insert(HostHistory::default());
            }
            let existing = children.get(task.stack).ok().and_then(|children| {
                children.iter().find(|child| {
                    hosted_browsers
                        .get(*child)
                        .is_ok_and(|surface| surface.belongs_to(page))
                })
            });
            if let Some(browser) = existing {
                commands.entity(browser).insert(metadata);
            } else {
                commands.entity(task.stack).despawn_children();
                commands.spawn((
                    Browser::hosted_page(&task.url, page.title),
                    ChildOf(task.stack),
                ));
            }
        }
        commands.entity(task_entity).insert(PageOpenHandled);
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
    mut observations: MessageWriter<HostHistoryObserve>,
) {
    for (entity, metadata, history) in &stacks {
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
    use vmux_api::{BuiltinIcon, PageIcon};
    use vmux_ecs::PageOpenId;
    use vmux_ecs::page::{HostHistoryDelta, HostHistoryStep};

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
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://tools/extensions".into(),
            request_id: None,
        });

        app.update();

        assert_eq!(
            app.world().get::<PageMetadata>(stack).unwrap().title,
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
        let stack = app.world_mut().spawn_empty().id();
        app.world_mut().spawn(PageOpenTask {
            id: PageOpenId::new(),
            stack,
            url: "vmux://simulator/ios/27.0/iPhone%2017%20Pro".into(),
            request_id: None,
        });

        app.update();

        let metadata = app.world().get::<PageMetadata>(stack).unwrap();
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
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: url.to_string(),
                request_id: None,
            });
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
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
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
                .find(|child| app.world().entity(*child).contains::<Browser>())
                .unwrap()
        };

        let first = open(&mut app, "vmux://sessions/");
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
            app.world_mut().spawn(PageOpenTask {
                id: PageOpenId::new(),
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
                .find(|child| app.world().entity(*child).contains::<Browser>())
                .unwrap()
        };

        let sessions = open(&mut app, "vmux://sessions/");
        let projects = open(&mut app, "vmux://projects/");

        assert_ne!(sessions, projects);
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
}
