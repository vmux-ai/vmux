use bevy::prelude::*;
use vmux_ecs::page::PageManifest;
use vmux_ecs::{PageMetadata, PageOpenError, PageOpenHandled, PageOpenSet, PageOpenTask};

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
    views: Query<
        (Entity, &PageMetadata, Has<M>),
        (With<vmux_ecs::page::HostsPage>, Changed<PageMetadata>),
    >,
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

impl Plugin for HostedPagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, open.in_set(PageOpenSet::HandleKnownPages));
    }
}

type PendingPageOpen = (Without<PageOpenHandled>, Without<PageOpenError>);

fn open(
    pages: Query<(&HostedPage, Option<&PageManifest>)>,
    tasks: Query<(Entity, &PageOpenTask), PendingPageOpen>,
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
            commands.entity(task.stack).despawn_children();
            let metadata = manifest.map_or_else(
                || PageMetadata {
                    url: task.url.clone(),
                    title: page.title.to_string(),
                    ..default()
                },
                |manifest| manifest.metadata_for(&task.url),
            );
            commands.entity(task.stack).insert(metadata);
            commands.spawn((
                Browser::hosted_page(&task.url, page.title),
                ChildOf(task.stack),
            ));
        }
        commands.entity(task_entity).insert(PageOpenHandled);
    }
}

#[cfg(test)]
mod tests {
    use vmux_api::{BuiltinIcon, PageIcon};
    use vmux_ecs::PageOpenId;

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
                vmux_ecs::page::HostsPage,
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
}
