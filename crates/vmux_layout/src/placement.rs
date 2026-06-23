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
    Focus { tab: Entity, stack: Entity },
    AddTab { pane: Entity },
    Spiral { anchor: Entity, axis: PaneSplitDirection },
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

/// Decide where a page (`url`) should open.
///
/// `reuse`: an exact-URL hit anywhere in the space, if any (highest priority).
/// `leaves`: leaf panes in the CURRENT tab, each with the kinds of its stacks,
/// its `SpawnSeq`, and its pixel size.
/// `self_pane`: the calling agent's own pane, used as a fallback target.
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
        if let Some(agent) = leaves.iter().find(|l| l.kinds.contains(&PageKind::Agent)) {
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

    if let Some(same) = leaves.iter().find(|l| l.kinds.contains(&kind)) {
        return Placement::AddTab { pane: same.pane };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_four_kinds() {
        assert_eq!(page_kind_for_url("vmux://agent/vibe/abc"), PageKind::Agent);
        assert_eq!(page_kind_for_url("vmux://terminal/123"), PageKind::Terminal);
        assert_eq!(page_kind_for_url("file:///x.rs"), PageKind::File);
        assert_eq!(page_kind_for_url("https://example.com"), PageKind::Browser);
        assert_eq!(page_kind_for_url("vmux://services/"), PageKind::Browser);
        assert_eq!(page_kind_for_url("vmux://spaces/"), PageKind::Browser);
    }

    fn e(n: u64) -> Entity {
        Entity::from_bits(n)
    }

    fn leaf(pane: u64, kinds: &[PageKind], seq: u64, size: (f32, f32)) -> LeafInfo {
        LeafInfo {
            pane: e(pane),
            kinds: kinds.to_vec(),
            spawn_seq: seq,
            size: Vec2::new(size.0, size.1),
        }
    }

    #[test]
    fn exact_url_reuse_wins() {
        let hit = ReuseHit {
            tab: e(1),
            stack: e(2),
        };
        let got = resolve_placement(
            "https://x.com",
            Some(hit),
            &[leaf(10, &[PageKind::Browser], 5, (800.0, 600.0))],
            e(10),
        );
        assert_eq!(
            got,
            Placement::Focus {
                tab: e(1),
                stack: e(2)
            }
        );
    }

    #[test]
    fn same_type_adds_tab_no_split() {
        let got = resolve_placement(
            "https://b.com",
            None,
            &[leaf(10, &[PageKind::Browser], 5, (800.0, 600.0))],
            e(10),
        );
        assert_eq!(got, Placement::AddTab { pane: e(10) });
    }

    #[test]
    fn first_page_fills_empty_leaf() {
        let got = resolve_placement(
            "https://b.com",
            None,
            &[leaf(10, &[], 1, (800.0, 600.0))],
            e(10),
        );
        assert_eq!(got, Placement::AddTab { pane: e(10) });
    }

    #[test]
    fn new_type_splits_newest_nonagent_leaf_along_longer_side() {
        let leaves = [
            leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
            leaf(2, &[PageKind::File], 9, (900.0, 400.0)),
        ];
        let got = resolve_placement("https://b.com", None, &leaves, e(1));
        assert_eq!(
            got,
            Placement::Spiral {
                anchor: e(2),
                axis: PaneSplitDirection::Row
            }
        );
    }

    #[test]
    fn new_type_splits_tall_leaf_into_column() {
        let leaves = [leaf(2, &[PageKind::File], 9, (400.0, 900.0))];
        let got = resolve_placement("https://b.com", None, &leaves, e(2));
        assert_eq!(
            got,
            Placement::Spiral {
                anchor: e(2),
                axis: PaneSplitDirection::Column
            }
        );
    }

    #[test]
    fn agent_page_never_splits_when_agent_pane_exists() {
        let leaves = [
            leaf(1, &[PageKind::Agent], 1, (800.0, 900.0)),
            leaf(2, &[PageKind::Browser], 9, (900.0, 400.0)),
        ];
        let got = resolve_placement("vmux://agent/vibe/x", None, &leaves, e(2));
        assert_eq!(got, Placement::AddTab { pane: e(1) });
    }

    #[test]
    fn nonagent_page_bootstraps_by_splitting_agent_when_only_leaf() {
        let leaves = [leaf(1, &[PageKind::Agent], 1, (1600.0, 900.0))];
        let got = resolve_placement("https://b.com", None, &leaves, e(1));
        assert_eq!(
            got,
            Placement::Spiral {
                anchor: e(1),
                axis: PaneSplitDirection::Row
            }
        );
    }

    #[test]
    fn agent_page_bootstraps_by_splitting_newest_nonagent_when_no_agent_pane() {
        let leaves = [leaf(2, &[PageKind::Browser], 9, (400.0, 900.0))];
        let got = resolve_placement("vmux://agent/vibe/x", None, &leaves, e(2));
        assert_eq!(
            got,
            Placement::Spiral {
                anchor: e(2),
                axis: PaneSplitDirection::Column
            }
        );
    }
}
