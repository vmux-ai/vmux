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
                mark_hosted_view::<M>.after(PageOpenSet::HandleKnownPages),
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

fn mark_hosted_view<M: Component + Default>(
    manifest: Res<HostedUiManifest<M>>,
    views: Query<(Entity, &PageMetadata), (With<vmux_ecs::page::HostsPage>, Without<M>)>,
    mut commands: Commands,
) {
    for (entity, page) in &views {
        if manifest.page.answers_for(&page.url) {
            commands.entity(entity).try_insert(M::default());
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
        let Some((page, manifest)) = pages.iter().find(|(page, _)| page.answers_for(&task.url))
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
