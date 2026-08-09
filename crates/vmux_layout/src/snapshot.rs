use bevy::prelude::*;
use vmux_core::PageMetadata;

use crate::pane::{Pane, PaneSize, PaneSplit, PaneSplitDirection, Zoomed};
use crate::protocol::format_id;
use crate::protocol::{
    Focus, LayoutNode, LayoutSnapshot, NodeKind, SplitDirection, Stack as StackDto, Tab as TabDto,
};
use crate::stack::{FocusedStack, Stack};
use crate::tab::Tab as LayoutTab;

pub fn build_layout_snapshot(
    tabs_q: &Query<(Entity, &LayoutTab, Option<&Children>)>,
    splits_q: &Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: &Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: &Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes_q: &Query<&PaneSize>,
    zoomed_q: &Query<&Zoomed>,
    focused: &FocusedStack,
    self_stack: Option<Entity>,
) -> LayoutSnapshot {
    let active_tab = focused.tab;
    let tabs = tabs_q
        .iter()
        .map(|(tab_entity, tab, children)| {
            let zoomed_leaf = zoomed_q.get(tab_entity).ok().map(|z| z.leaf);
            let root = children
                .and_then(|c| c.iter().next())
                .map(|root_entity| {
                    build_node(
                        root_entity,
                        splits_q,
                        leaves_q,
                        stacks_q,
                        pane_sizes_q,
                        zoomed_leaf,
                        self_stack,
                    )
                })
                .unwrap_or(LayoutNode::Pane {
                    id: None,
                    is_zoomed: false,
                    stacks: Vec::new(),
                });
            TabDto {
                id: Some(format_id(NodeKind::Tab, tab_entity.to_bits())),
                name: tab.name.clone(),
                is_active: Some(tab_entity) == active_tab,
                root,
            }
        })
        .collect();

    LayoutSnapshot {
        tabs,
        focused: Focus {
            tab: focused.tab.map(|e| format_id(NodeKind::Tab, e.to_bits())),
            pane: focused.pane.map(|e| format_id(NodeKind::Pane, e.to_bits())),
            stack: focused
                .stack
                .map(|e| format_id(NodeKind::Stack, e.to_bits())),
        },
    }
}

fn build_node(
    entity: Entity,
    splits_q: &Query<(Entity, &PaneSplit, Option<&Children>), With<Pane>>,
    leaves_q: &Query<(Entity, Option<&Children>), (With<Pane>, Without<PaneSplit>)>,
    stacks_q: &Query<(Entity, Option<&Children>, Option<&PageMetadata>), With<Stack>>,
    pane_sizes_q: &Query<&PaneSize>,
    zoomed_leaf: Option<Entity>,
    self_stack: Option<Entity>,
) -> LayoutNode {
    if let Ok((split_entity, split, children)) = splits_q.get(entity) {
        let child_entities: Vec<Entity> = children.map(|c| c.iter().collect()).unwrap_or_default();
        let flex_weights = child_entities
            .iter()
            .map(|child| {
                pane_sizes_q
                    .get(*child)
                    .map(|ps| ps.flex_grow)
                    .unwrap_or(1.0)
            })
            .collect();
        let children_dto = child_entities
            .into_iter()
            .map(|child| {
                build_node(
                    child,
                    splits_q,
                    leaves_q,
                    stacks_q,
                    pane_sizes_q,
                    zoomed_leaf,
                    self_stack,
                )
            })
            .collect();
        return LayoutNode::Split {
            id: Some(format_id(NodeKind::Split, split_entity.to_bits())),
            direction: match split.direction {
                PaneSplitDirection::Row => SplitDirection::Row,
                PaneSplitDirection::Column => SplitDirection::Column,
            },
            flex_weights,
            children: children_dto,
        };
    }
    if let Ok((leaf_entity, leaf_children)) = leaves_q.get(entity) {
        let stacks = leaf_children
            .map(|c| {
                c.iter()
                    .filter_map(|child| stacks_q.get(child).ok())
                    .map(|(stack_entity, _stack_children, page)| {
                        build_stack(stack_entity, page, self_stack)
                    })
                    .collect()
            })
            .unwrap_or_default();
        return LayoutNode::Pane {
            id: Some(format_id(NodeKind::Pane, leaf_entity.to_bits())),
            is_zoomed: zoomed_leaf == Some(leaf_entity),
            stacks,
        };
    }
    LayoutNode::Pane {
        id: None,
        is_zoomed: false,
        stacks: Vec::new(),
    }
}

fn stack_kind_for_url(url: &str) -> &'static str {
    if url.starts_with("vmux://terminal/") {
        "terminal"
    } else if url.starts_with("file:") {
        "files"
    } else {
        "browser"
    }
}

fn build_stack(
    stack_entity: Entity,
    page: Option<&PageMetadata>,
    self_stack: Option<Entity>,
) -> StackDto {
    let url = page.map(|p| p.url.clone()).unwrap_or_default();
    StackDto {
        id: Some(format_id(NodeKind::Stack, stack_entity.to_bits())),
        title: page.map(|p| p.title.clone()).unwrap_or_default(),
        kind: stack_kind_for_url(&url).to_string(),
        url,
        is_loading: false,
        icon: page.map(|p| p.icon.clone()).unwrap_or_default(),
        is_self: Some(stack_entity) == self_stack,
        process_id: None,
    }
}

#[cfg(test)]
#[path = "snapshot.test.rs"]
mod tests;
