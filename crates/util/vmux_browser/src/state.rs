use bevy::prelude::*;
#[cfg(test)]
use bevy_cef::prelude::*;
#[cfg(test)]
use vmux_api::PageIcon;
#[cfg(test)]
use vmux_ecs::page::PageReady;
#[cfg(test)]
use vmux_ecs::{Active, PageIdentity, PageMetadata, UiStateWrite};
#[cfg(test)]
use vmux_flex::prelude::*;
#[cfg(test)]
use vmux_history::LastActivatedAt;
#[cfg(test)]
use vmux_layout::pane::Pane;
#[cfg(test)]
use vmux_layout::presentation::Retiring;
#[cfg(test)]
use vmux_layout::projection::PaneTreeProjection;
#[cfg(test)]
use vmux_layout::settings::LayoutSettings;
#[cfg(test)]
use vmux_layout::stack::Stack;
#[cfg(test)]
use vmux_layout::state::LayoutUiState;
use vmux_layout::state_source::LayoutStateSourcePlugin;
#[cfg(test)]
use vmux_layout::state_source::{StateRevision, UpdateNoticeReceipt};
#[cfg(test)]
use vmux_layout::tab::Tab;
#[cfg(test)]
use vmux_layout::{Browser, Header, LayoutCef, Loading, Open, UpdateState};
#[cfg(test)]
use vmux_setting::AppSettings;
use vmux_space::SpaceProjectProjectionPlugin;

#[cfg(test)]
pub(crate) use vmux_layout::state_source::PagePresentation;

pub(crate) struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<LayoutStateSourcePlugin>() {
            app.add_plugins(LayoutStateSourcePlugin);
        }
        if !app.is_plugin_added::<SpaceProjectProjectionPlugin>() {
            app.add_plugins(SpaceProjectProjectionPlugin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::UiStateProjection;
    use vmux_layout::state::UpdateStatus;

    #[derive(Resource, Default)]
    struct ProjectionWrites(Vec<(Entity, vmux_layout::state::LayoutUiStatePatch)>);

    impl ProjectionWrites {
        fn families(&self, entity: Entity) -> Vec<&'static str> {
            let mut families = std::collections::BTreeSet::new();
            for (target, patch) in &self.0 {
                if *target != entity {
                    continue;
                }
                for (name, present) in [
                    ("bookmarks", patch.bookmarks.is_some()),
                    ("geometry", patch.layout.is_some()),
                    ("pane_tree", patch.pane_tree.is_some()),
                    ("projects", patch.projects.is_some()),
                    ("stacks", patch.stacks.is_some()),
                    ("tabs", patch.tabs.is_some()),
                ] {
                    if present {
                        families.insert(name);
                    }
                }
            }
            families.into_iter().collect()
        }
    }

    fn record_projections(
        write: On<UiStateWrite<LayoutUiState>>,
        mut writes: ResMut<ProjectionWrites>,
    ) {
        let patch = write.update();
        if patch.bookmarks.is_some()
            || patch.layout.is_some()
            || patch.pane_tree.is_some()
            || patch.projects.is_some()
            || patch.stacks.is_some()
            || patch.tabs.is_some()
        {
            writes.0.push((write.webview(), patch.clone()));
        }
    }

    struct ProjectionPage {
        window: Entity,
        layout: Entity,
        tab: Entity,
        pane: Entity,
        stack: Entity,
    }

    struct ProjectionPages {
        app: App,
        focus: Entity,
    }

    impl ProjectionPages {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatePlugin))
                .insert_resource(LayoutSettings::default())
                .insert_resource(AppSettings::default())
                .init_non_send::<Browsers>()
                .init_resource::<ProjectionWrites>()
                .add_observer(record_projections);
            app.world_mut().spawn(UpdateState::idle());
            let focus = app
                .world_mut()
                .spawn(vmux_layout::active_pane::ActiveStack::default().local_bundle())
                .id();
            Self { app, focus }
        }

        fn page(&mut self, title: &str) -> ProjectionPage {
            let window = self.app.world_mut().spawn(Window::default()).id();
            let root = self.app.world_mut().spawn(HostWindow(window)).id();
            let layout = self
                .app
                .world_mut()
                .spawn((LayoutCef, PageReady {}, ChildOf(root)))
                .id();
            let tab = self
                .app
                .world_mut()
                .spawn((Tab::default(), LastActivatedAt(1), ChildOf(root)))
                .id();
            let pane = self
                .app
                .world_mut()
                .spawn((Pane, Active, LastActivatedAt(1), ChildOf(tab)))
                .id();
            let stack = self
                .app
                .world_mut()
                .spawn((
                    Stack::default(),
                    Active,
                    LastActivatedAt(1),
                    ChildOf(pane),
                    PageMetadata {
                        title: title.into(),
                        url: "vmux://start/".into(),
                        ..Default::default()
                    },
                ))
                .id();
            self.app
                .world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(layout);
            ProjectionPage {
                window,
                layout,
                tab,
                pane,
                stack,
            }
        }

        fn focus(&mut self, page: &ProjectionPage) {
            let world = self.app.world_mut();
            let mut windows = world.query_filtered::<Entity, (With<Window>, With<Active>)>();
            let active: Vec<_> = windows.iter(world).collect();
            for entity in active {
                world.entity_mut(entity).remove::<Active>();
            }
            world.entity_mut(page.window).insert(Active);
            let mut tabs = world.query_filtered::<Entity, (With<Tab>, With<Active>)>();
            let active_tabs: Vec<_> = tabs.iter(world).collect();
            for entity in active_tabs {
                world.entity_mut(entity).remove::<Active>();
            }
            world.entity_mut(page.tab).insert(Active);
            world
                .entity_mut(self.focus)
                .insert(vmux_layout::active_pane::ActiveStack {
                    tab: Some(page.tab),
                    pane: Some(page.pane),
                    stack: Some(page.stack),
                });
        }

        fn update(&mut self) -> ProjectionWrites {
            self.app.update();
            ProjectionWrites(std::mem::take(
                &mut self.app.world_mut().resource_mut::<ProjectionWrites>().0,
            ))
        }

        fn available(&mut self, page: &ProjectionPage, available: bool) {
            let mut browsers = self.app.world_mut().non_send_mut::<Browsers>();
            if available {
                browsers.set_externally_hosted(page.layout);
            } else {
                browsers.unset_externally_hosted(page.layout);
            }
        }
    }

    #[test]
    fn projection_receipts_keep_all_payload_families_independent() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        let writes = pages.update();
        assert_eq!(
            writes.families(page.layout),
            [
                "bookmarks",
                "geometry",
                "pane_tree",
                "projects",
                "stacks",
                "tabs"
            ]
        );
        assert!(pages.update().0.is_empty());
        pages
            .app
            .world_mut()
            .entity_mut(page.layout)
            .insert(PageReady {});
        assert_eq!(
            pages.update().families(page.layout),
            writes.families(page.layout)
        );
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_retire_before_a_readded_layout_becomes_available() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        let families = pages.update().families(page.layout);
        assert_eq!(families.len(), 6);
        pages.available(&page, false);
        pages
            .app
            .world_mut()
            .entity_mut(page.layout)
            .remove::<LayoutCef>();
        pages
            .app
            .world_mut()
            .entity_mut(page.layout)
            .insert(LayoutCef);
        assert!(pages.update().0.is_empty());
        assert!(pages.update().0.is_empty());
        pages.available(&page, true);
        assert_eq!(pages.update().families(page.layout), families);
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_preserve_each_window_when_focus_returns() {
        let mut pages = ProjectionPages::new();
        let first = pages.page("first");
        let second = pages.page("second");
        pages.focus(&first);
        assert_eq!(pages.update().families(first.layout).len(), 6);
        pages.focus(&second);
        assert_eq!(pages.update().families(second.layout).len(), 6);
        pages.focus(&first);
        assert!(pages.update().0.is_empty());
        pages
            .app
            .world_mut()
            .get_mut::<PageMetadata>(first.stack)
            .unwrap()
            .title = "renamed".into();
        let changed = pages.update();
        assert!(changed.0.iter().all(|(entity, _)| *entity == first.layout));
        let navigation = changed
            .0
            .iter()
            .find_map(|(_, patch)| patch.stacks.as_ref())
            .unwrap();
        assert_eq!(navigation.stacks[0].title, "renamed");
        let panes = changed
            .0
            .iter()
            .find_map(|(_, patch)| patch.pane_tree.as_ref())
            .unwrap();
        assert_eq!(panes.panes[0].stacks[0].title, "renamed");
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_do_not_acknowledge_an_unavailable_layout() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        pages.available(&page, false);
        assert!(pages.update().0.is_empty());
        pages.app.world_mut().entity_mut(page.tab).insert(
            vmux_layout::page_context::PendingWorkspaceSelection::new(
                "/project".into(),
                "feature".into(),
                1,
            ),
        );
        assert!(pages.update().0.is_empty());
        pages.available(&page, true);
        assert_eq!(pages.update().families(page.layout).len(), 6);
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_update_geometry_after_resize_without_focus_churn() {
        let mut pages = ProjectionPages::new();
        let page = pages.page("first");
        pages.focus(&page);
        pages.app.world_mut().spawn((
            Header,
            Open,
            ChildOf(page.layout),
            ComputedNode::from_origin(Vec2::new(100.0, 50.0)),
        ));
        let initial = pages.update();
        let right = initial
            .0
            .iter()
            .find_map(|(_, patch)| patch.layout.as_ref())
            .unwrap()
            .header_right
            .unwrap();
        assert!(pages.update().0.is_empty());
        pages
            .app
            .world_mut()
            .get_mut::<Window>(page.window)
            .unwrap()
            .focused = false;
        assert!(pages.update().0.is_empty());
        let width = pages
            .app
            .world()
            .get::<Window>(page.window)
            .unwrap()
            .resolution
            .width();
        pages
            .app
            .world_mut()
            .get_mut::<Window>(page.window)
            .unwrap()
            .resolution
            .set(width + 200.0, 900.0);
        let resized = pages.update();
        assert_eq!(resized.families(page.layout), ["geometry"]);
        let updated = resized
            .0
            .iter()
            .find_map(|(_, patch)| patch.layout.as_ref())
            .unwrap()
            .header_right
            .unwrap();
        assert_eq!(updated - right, 200.0);
        assert!(pages.update().0.is_empty());
    }

    #[test]
    fn projection_receipts_refuse_a_surface_reparented_out_of_its_stack() {
        let mut pages = ProjectionPages::new();
        let first = pages.page("first");
        let second = pages.page("second");
        pages.focus(&first);
        pages
            .app
            .world_mut()
            .get_mut::<PageMetadata>(first.stack)
            .unwrap()
            .url
            .clear();
        let browser = pages
            .app
            .world_mut()
            .spawn((
                Browser,
                ChildOf(first.stack),
                PageMetadata {
                    title: "hosted page".into(),
                    url: "https://example.test/".into(),
                    ..Default::default()
                },
            ))
            .id();
        pages
            .app
            .world_mut()
            .entity_mut(first.stack)
            .insert(vmux_layout::page_slot::Current::new(
                Some(browser),
                PageMetadata {
                    title: "hosted page".into(),
                    url: "https://example.test/".into(),
                    ..Default::default()
                },
            ));
        let initial = pages.update();
        let pane = initial
            .0
            .iter()
            .find_map(|(_, patch)| patch.pane_tree.as_ref())
            .unwrap();
        assert_eq!(pane.panes[0].stacks[0].url, "https://example.test/");
        let initial_tabs = initial
            .0
            .iter()
            .find_map(|(_, patch)| patch.tabs.as_ref())
            .unwrap();
        assert_eq!(initial_tabs.tabs[0].url, "https://example.test/");
        pages
            .app
            .world_mut()
            .entity_mut(browser)
            .insert(ChildOf(second.stack));
        let moved = pages.update();
        let pane = moved
            .0
            .iter()
            .find_map(|(_, patch)| patch.pane_tree.as_ref())
            .unwrap();
        assert!(pane.panes[0].stacks[0].url.is_empty());
        let tabs = moved
            .0
            .iter()
            .find_map(|(_, patch)| patch.tabs.as_ref())
            .unwrap();
        assert!(tabs.tabs[0].url.is_empty());
        assert!(pages.update().0.is_empty());
    }

    #[derive(Resource, Default)]
    struct PublishedUpdates(Vec<(Entity, LayoutUiState)>);

    fn record_updates(
        write: On<UiStateWrite<LayoutUiState>>,
        mut updates: ResMut<PublishedUpdates>,
    ) {
        let patch = write.update();
        if patch.update_progress.is_none()
            && patch.update_ready.is_none()
            && patch.update_cleared.is_none()
        {
            return;
        }
        let mut state = LayoutUiState::default();
        state.apply(patch.clone());
        updates.0.push((write.webview(), state));
    }

    struct UpdatePages {
        app: App,
        state: Entity,
    }

    impl UpdatePages {
        fn new(state: UpdateState) -> Self {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, StatePlugin))
                .insert_resource(LayoutSettings::default())
                .insert_resource(AppSettings::default())
                .init_non_send::<Browsers>()
                .init_resource::<PublishedUpdates>()
                .add_observer(record_updates);
            let state = app.world_mut().spawn(state).id();
            Self { app, state }
        }

        fn page(&mut self) -> Entity {
            let page = self.app.world_mut().spawn((LayoutCef, PageReady {})).id();
            self.app
                .world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(page);
            page
        }

        fn update(&mut self) -> Vec<(Entity, Option<UpdateStatus>)> {
            self.app.update();
            let mut results = Vec::new();
            for (entity, state) in self
                .app
                .world_mut()
                .resource_mut::<PublishedUpdates>()
                .0
                .drain(..)
            {
                results.push((entity, state.update));
            }
            results
        }

        fn set(&mut self, state: UpdateState) {
            self.app.world_mut().entity_mut(self.state).insert(state);
        }
    }

    #[test]
    fn update_notice_replays_on_readiness_and_clears_when_update_finishes() {
        let mut pages = UpdatePages::new(UpdateState::ready("v2".into()));
        let page = pages.page();
        let ready = vec![(
            page,
            Some(UpdateStatus::Ready {
                version: "v2".into(),
            }),
        )];
        assert_eq!(pages.update(), ready);
        assert!(pages.update().is_empty());
        pages.app.world_mut().entity_mut(page).insert(PageReady {});
        assert_eq!(pages.update(), ready);
        pages.set(UpdateState::idle());
        assert_eq!(pages.update(), vec![(page, None)]);
        pages.app.world_mut().entity_mut(page).insert(PageReady {});
        assert!(pages.update().is_empty());
    }

    #[test]
    fn update_notice_restarts_after_layout_owner_is_reattached() {
        let mut pages = UpdatePages::new(UpdateState::idle());
        let page = pages.page();
        assert_eq!(pages.update(), vec![(page, None)]);
        assert!(pages.update().is_empty());
        pages.app.world_mut().entity_mut(page).remove::<LayoutCef>();
        pages.app.world_mut().entity_mut(page).insert(LayoutCef);
        assert_eq!(pages.update(), vec![(page, None)]);
        pages.app.world_mut().entity_mut(page).remove::<LayoutCef>();
        assert!(pages.update().is_empty());
        assert!(pages.app.world().get::<UpdateNoticeReceipt>(page).is_none());
    }

    #[test]
    fn update_notice_publications_stay_independent_when_window_focus_changes() {
        let mut pages = UpdatePages::new(UpdateState::ready("v2".into()));
        let first_window = pages
            .app
            .world_mut()
            .spawn((Window::default(), Active))
            .id();
        let second_window = pages.app.world_mut().spawn(Window::default()).id();
        let first = pages.page();
        let second = pages.page();
        pages
            .app
            .world_mut()
            .entity_mut(first)
            .insert(HostWindow(first_window));
        pages
            .app
            .world_mut()
            .entity_mut(second)
            .insert(HostWindow(second_window));
        assert_eq!(
            pages.update(),
            vec![(
                first,
                Some(UpdateStatus::Ready {
                    version: "v2".into()
                })
            )]
        );
        pages
            .app
            .world_mut()
            .entity_mut(first_window)
            .remove::<Active>();
        pages
            .app
            .world_mut()
            .entity_mut(second_window)
            .insert(Active);
        assert_eq!(
            pages.update(),
            vec![(
                second,
                Some(UpdateStatus::Ready {
                    version: "v2".into()
                })
            )]
        );
        pages
            .app
            .world_mut()
            .entity_mut(second_window)
            .remove::<Active>();
        pages
            .app
            .world_mut()
            .entity_mut(first_window)
            .insert(Active);
        assert!(pages.update().is_empty());
    }

    #[test]
    fn update_notice_does_not_acknowledge_an_unavailable_browser() {
        let mut pages = UpdatePages::new(UpdateState::downloading("v2".into(), 3, 10));
        let page = pages.app.world_mut().spawn((LayoutCef, PageReady {})).id();
        assert!(pages.update().is_empty());
        pages
            .app
            .world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(page);
        assert_eq!(
            pages.update(),
            vec![(
                page,
                Some(UpdateStatus::Downloading {
                    version: "v2".into(),
                    downloaded: 3,
                    total: 10
                })
            )]
        );
        assert!(pages.update().is_empty());
        pages.set(UpdateState::installing("v2".into()));
        assert_eq!(
            pages.update(),
            vec![(
                page,
                Some(UpdateStatus::Installing {
                    version: "v2".into()
                })
            )]
        );
    }

    #[test]
    fn dormant_stack_keeps_its_favicon_without_a_loading_indicator() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatePlugin))
            .insert_resource(AppSettings::default())
            .init_non_send::<Browsers>();
        app.world_mut().spawn(UpdateState::idle());
        let layout = app.world_mut().spawn((LayoutCef, PageReady {})).id();
        app.world_mut()
            .non_send_mut::<Browsers>()
            .set_externally_hosted(layout);
        let tab = app
            .world_mut()
            .spawn((Tab::default(), LastActivatedAt(1)))
            .id();
        let pane = app
            .world_mut()
            .spawn((Pane, LastActivatedAt(1), ChildOf(tab)))
            .id();
        let metadata = PageMetadata {
            title: "Start".into(),
            url: "vmux://start/".into(),
            icon: PageIcon::Builtin(vmux_api::BuiltinIcon::Sparkles),
            ..Default::default()
        };
        let dormant = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt(1),
                metadata.clone(),
                ChildOf(pane),
            ))
            .id();
        let selected = app
            .world_mut()
            .spawn((
                Stack::default(),
                LastActivatedAt(2),
                metadata.clone(),
                Active,
                ChildOf(pane),
            ))
            .id();
        app.world_mut().spawn(
            vmux_layout::active_pane::ActiveStack {
                tab: Some(tab),
                pane: Some(pane),
                stack: Some(selected),
            }
            .local_bundle(),
        );
        app.update();
        let projection = &app.world().get::<PaneTreeProjection>(layout).unwrap().0;
        let rows = &projection.panes[0].stacks;
        let start = rows.iter().find(|row| row.id == dormant.to_bits()).unwrap();
        assert!(!start.is_loading);
        assert_eq!(start.icon, metadata.icon);
        assert!(
            rows.iter()
                .find(|row| row.id == selected.to_bits())
                .unwrap()
                .is_loading
        );

        let browser = app
            .world_mut()
            .spawn((Browser, Loading, metadata.clone(), ChildOf(dormant)))
            .id();
        app.world_mut()
            .entity_mut(dormant)
            .insert(vmux_layout::page_slot::Current::new(
                Some(browser),
                metadata,
            ));
        app.update();
        let projection = &app.world().get::<PaneTreeProjection>(layout).unwrap().0;
        assert!(
            projection.panes[0]
                .stacks
                .iter()
                .find(|row| row.id == dormant.to_bits())
                .unwrap()
                .is_loading
        );
    }

    #[test]
    fn retiring_state_invalidates_layout_projection() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatePlugin))
            .insert_resource(LayoutSettings::default())
            .insert_resource(AppSettings::default());
        app.world_mut().insert_non_send(Browsers::default());
        app.world_mut().spawn(UpdateState::idle());
        app.update();
        let revision = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        let closing = app.world_mut().spawn(Retiring).id();

        app.update();

        let added = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert!(added > revision);

        app.world_mut().entity_mut(closing).remove::<Retiring>();
        app.update();

        let removed = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert!(removed > added);
    }

    #[test]
    fn pending_stack_metadata_wins_before_the_surface_commits() {
        let presented = PageMetadata {
            url: "git://old".to_string(),
            title: "Old".to_string(),
            ..Default::default()
        };
        let pending = PageMetadata {
            url: "vmux://projects/".to_string(),
            title: "Projects".to_string(),
            ..Default::default()
        };
        let identity = PageIdentity::from("Old page identity");

        let projected = PagePresentation::metadata(Some(&pending), Some(&presented)).unwrap();

        assert_eq!(projected.url, "vmux://projects/");
        assert!(PagePresentation::identity(projected, Some(&presented), Some(&identity)).is_none());
    }

    #[test]
    fn pending_workspace_selection_does_not_invalidate_unrelated_projections() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatePlugin))
            .insert_resource(LayoutSettings::default())
            .insert_resource(AppSettings::default());
        app.world_mut().insert_non_send(Browsers::default());
        app.world_mut().spawn(UpdateState::idle());
        app.update();
        let revision = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        let tab = app
            .world_mut()
            .spawn(vmux_layout::page_context::PendingWorkspaceSelection::new(
                "/project".into(),
                "feature".into(),
                1,
            ))
            .id();

        app.update();

        let after_add = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert_eq!(after_add, revision);

        app.world_mut()
            .entity_mut(tab)
            .remove::<vmux_layout::page_context::PendingWorkspaceSelection>();
        app.update();

        let after_remove = {
            let world = app.world_mut();
            let mut revisions = world.query::<&StateRevision>();
            revisions.single(world).unwrap().0
        };
        assert_eq!(after_remove, after_add);
    }
}
