use crate::pane::PaneSplitDirection;
use bevy::math::Vec2;
use bevy::prelude::Entity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Agent,
    Terminal,
    File,
    Browser,
}

pub fn page_kind_for_url(url: &str) -> PageKind {
    if url.starts_with("vmux://agent/") {
        PageKind::Agent
    } else if url.starts_with("vmux://terminal/") {
        PageKind::Terminal
    } else if url.starts_with("file:") {
        PageKind::File
    } else {
        PageKind::Browser
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    Focus {
        tab: Entity,
        stack: Entity,
    },
    AddTab {
        pane: Entity,
    },
    Spiral {
        anchor: Entity,
        axis: PaneSplitDirection,
    },
}

#[derive(Debug, Clone)]
pub struct LeafInfo {
    pub pane: Entity,
    pub kinds: Vec<PageKind>,
    pub spawn_seq: u64,
    pub size: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReuseHit {
    pub tab: Entity,
    pub stack: Entity,
}

fn longer_axis(size: Vec2) -> PaneSplitDirection {
    if size.x >= size.y {
        PaneSplitDirection::Row
    } else {
        PaneSplitDirection::Column
    }
}

fn newest_nonagent_leaf(leaves: &[LeafInfo]) -> Option<&LeafInfo> {
    leaves
        .iter()
        .filter(|l| !l.kinds.contains(&PageKind::Agent))
        .max_by_key(|l| l.spawn_seq)
}

fn newest_leaf_with_kind(leaves: &[LeafInfo], kind: PageKind) -> Option<&LeafInfo> {
    leaves
        .iter()
        .filter(|l| l.kinds.len() == 1 && l.kinds.contains(&kind))
        .max_by_key(|l| l.spawn_seq)
}

fn file_reuse_key(url: &str) -> &str {
    url.split('#').next().unwrap_or(url)
}

pub fn reusable_page_match(request_url: &str, existing_url: &str) -> bool {
    let kind = page_kind_for_url(request_url);
    if page_kind_for_url(existing_url) != kind {
        return false;
    }
    match kind {
        PageKind::File => file_reuse_key(request_url) == file_reuse_key(existing_url),
        _ => request_url == existing_url,
    }
}

pub fn resolve_placement(
    url: &str,
    reuse: Option<ReuseHit>,
    leaves: &[LeafInfo],
    self_pane: Entity,
) -> Placement {
    if let Some(hit) = reuse {
        return Placement::Focus {
            tab: hit.tab,
            stack: hit.stack,
        };
    }

    let kind = page_kind_for_url(url);

    if let Some(empty) = leaves.iter().find(|l| l.kinds.is_empty()) {
        return Placement::AddTab { pane: empty.pane };
    }

    if kind == PageKind::Agent {
        if let Some(agent) = newest_leaf_with_kind(leaves, PageKind::Agent) {
            return Placement::AddTab { pane: agent.pane };
        }
        if let Some(anchor) = newest_nonagent_leaf(leaves) {
            return Placement::Spiral {
                anchor: anchor.pane,
                axis: longer_axis(anchor.size),
            };
        }
        return Placement::AddTab { pane: self_pane };
    }

    if let Some(same) = newest_leaf_with_kind(leaves, kind) {
        return Placement::AddTab { pane: same.pane };
    }

    if kind == PageKind::Terminal
        && leaves.iter().all(|leaf| {
            leaf.kinds.contains(&PageKind::Agent)
                || (leaf.kinds.len() == 1 && leaf.kinds.contains(&PageKind::Browser))
        })
        && let Some(browser) = newest_leaf_with_kind(leaves, PageKind::Browser)
    {
        return Placement::Spiral {
            anchor: browser.pane,
            axis: PaneSplitDirection::Column,
        };
    }

    if let Some(anchor) = newest_nonagent_leaf(leaves) {
        return Placement::Spiral {
            anchor: anchor.pane,
            axis: longer_axis(anchor.size),
        };
    }

    if let Some(agent) = leaves.iter().find(|l| l.kinds.contains(&PageKind::Agent)) {
        return Placement::Spiral {
            anchor: agent.pane,
            axis: longer_axis(agent.size),
        };
    }

    Placement::AddTab { pane: self_pane }
}

pub fn resolve_split_anchor(leaves: &[LeafInfo], self_pane: Entity) -> Entity {
    newest_nonagent_leaf(leaves)
        .or_else(|| leaves.iter().find(|l| l.kinds.contains(&PageKind::Agent)))
        .map(|l| l.pane)
        .unwrap_or(self_pane)
}

#[cfg(test)]
#[path = "placement.test.rs"]
mod tests;
