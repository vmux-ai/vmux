use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_cef::prelude::HostWindow;
use vmux_core::host::persistence::{PageRestore, WorkspaceStoreValidator};
use vmux_core::{CreatedAt, Order, PageMetadata, PageOpenId, PageOpenTask};
use vmux_flex::prelude::*;

use crate::LayoutStartupSet;
use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, pane_split_gaps};
use crate::space::Space;
use crate::stack::Stack;
use crate::tab::Tab;
use crate::window::Main;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutPersistenceSet {
    Restore,
}

pub(crate) struct LayoutPersistencePlugin;

impl Plugin for LayoutPersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, spawn_store_validator)
            .add_systems(Startup, restore_views.in_set(LayoutStartupSet::Post))
            .add_systems(Update, restore_views.in_set(LayoutPersistenceSet::Restore));
    }
}

fn spawn_store_validator(mut commands: Commands) {
    commands.spawn((
        Name::new("Layout workspace-store validator"),
        WorkspaceStoreValidator {
            name: "empty page metadata",
            rejects: persisted_store_has_only_empty_page_urls,
        },
    ));
}

fn persisted_store_has_only_empty_page_urls(body: &str) -> bool {
    let mut has_url = false;
    let mut has_nonempty_url = false;
    let mut in_page_metadata = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"vmux_header::system::PageMetadata\":") {
            in_page_metadata = true;
            continue;
        }
        if !in_page_metadata {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("url: \"")
            && let Some((url, _)) = rest.split_once('"')
        {
            has_url = true;
            has_nonempty_url |= !url.trim().is_empty();
        }
        if trimmed == ")," {
            in_page_metadata = false;
        }
    }
    has_url && !has_nonempty_url
}

#[derive(bevy::ecs::system::SystemParam)]
struct PersistedLayout<'w, 's> {
    main: Query<'w, 's, Entity, With<Main>>,
    primary_window: Query<'w, 's, Entity, With<PrimaryWindow>>,
    tabs: Query<
        'w,
        's,
        (Entity, Option<&'static Order>, Option<&'static CreatedAt>),
        (With<Tab>, Without<Node>),
    >,
    spaces: Query<'w, 's, Entity, (With<Space>, Without<Node>)>,
    splits: Query<'w, 's, (Entity, &'static PaneSplit), Without<Node>>,
    panes: Query<'w, 's, Entity, (With<Pane>, Without<PaneSplit>, Without<Node>)>,
    stacks: Query<'w, 's, (Entity, &'static PageMetadata), (With<Stack>, Without<Node>)>,
    pane_sizes: Query<'w, 's, &'static PaneSize>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
}

impl PersistedLayout<'_, '_> {
    fn has_pending_views(&self) -> bool {
        !self.tabs.is_empty()
            || !self.spaces.is_empty()
            || !self.splits.is_empty()
            || !self.panes.is_empty()
            || !self.stacks.is_empty()
    }
}

fn restore_views(layout: PersistedLayout, mut commands: Commands) {
    if !layout.has_pending_views() {
        return;
    }
    let Ok(main) = layout.main.single() else {
        return;
    };
    let Ok(primary_window) = layout.primary_window.single() else {
        return;
    };

    for space in &layout.spaces {
        commands
            .entity(space)
            .insert((Space::bundle(), ChildOf(main)));
    }

    let mut tabs = layout
        .tabs
        .iter()
        .map(|(entity, order, created)| (entity, order.map(|order| order.0), created.map(|v| v.0)))
        .collect::<Vec<_>>();
    tabs.sort_by_key(|(_, order, created)| (order.unwrap_or(u32::MAX), created.unwrap_or(0)));
    for (tab, _, _) in tabs {
        commands.entity(tab).insert((
            Transform::default(),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
        ));
        if let Ok(parent) = layout.parents.get(tab) {
            commands.entity(tab).insert(ChildOf(parent.get()));
        }
    }

    for (entity, split) in &layout.splits {
        let flex_direction = match split.direction {
            PaneSplitDirection::Row => FlexDirection::Row,
            PaneSplitDirection::Column => FlexDirection::Column,
        };
        let gap = pane_split_gaps(split.direction, crate::event::PANE_GAP_PX);
        commands.entity(entity).insert((
            HostWindow(primary_window),
            Transform::default(),
            Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                flex_direction,
                column_gap: gap.column_gap,
                row_gap: gap.row_gap,
                ..default()
            },
        ));
    }

    for pane in &layout.panes {
        let flex_grow = layout
            .pane_sizes
            .get(pane)
            .map(|size| size.flex_grow)
            .unwrap_or(1.0);
        commands.entity(pane).insert((
            Transform::default(),
            Node {
                flex_grow,
                flex_basis: Val::Px(0.0),
                align_items: AlignItems::Stretch,
                justify_content: JustifyContent::Stretch,
                ..default()
            },
        ));
    }

    let mut despawned = std::collections::HashSet::new();
    for (stack, metadata) in &layout.stacks {
        if metadata.url.is_empty() {
            despawned.insert(stack);
            commands.entity(stack).despawn();
            continue;
        }
        commands.entity(stack).insert((
            Transform::default(),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                ..default()
            },
        ));
        commands.spawn((
            PageOpenTask {
                id: PageOpenId::new(),
                stack,
                url: metadata.url.clone(),
                request_id: None,
            },
            PageRestore,
        ));
    }

    let mut repaired = std::collections::HashSet::new();
    for entity in layout
        .splits
        .iter()
        .map(|(entity, _)| entity)
        .chain(layout.panes.iter())
        .chain(layout.stacks.iter().map(|(entity, _)| entity))
    {
        let Ok(parent) = layout.parents.get(entity) else {
            continue;
        };
        let parent = parent.get();
        if !repaired.insert(parent) {
            continue;
        }
        let Ok(children) = layout.children.get(parent) else {
            continue;
        };
        for child in children.iter() {
            if despawned.contains(&child) {
                continue;
            }
            if let Ok(parent) = layout.parents.get(child) {
                commands.entity(child).insert(ChildOf(parent.get()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_tabs_are_ordered_before_legacy_tabs() {
        let ordered = Entity::from_bits(1);
        let legacy = Entity::from_bits(2);
        let mut tabs = [(legacy, None, Some(0i64)), (ordered, Some(5u32), Some(999))];
        tabs.sort_by_key(|(_, order, created)| (order.unwrap_or(u32::MAX), created.unwrap_or(0)));

        assert_eq!(tabs.map(|(entity, _, _)| entity), [ordered, legacy]);
    }

    #[test]
    fn empty_page_metadata_rejects_a_persisted_store() {
        assert!(persisted_store_has_only_empty_page_urls(
            "\"vmux_header::system::PageMetadata\": (\nurl: \"\",\n),"
        ));
        assert!(!persisted_store_has_only_empty_page_urls(
            "\"vmux_header::system::PageMetadata\": (\nurl: \"vmux://start/\",\n),"
        ));
    }
}
