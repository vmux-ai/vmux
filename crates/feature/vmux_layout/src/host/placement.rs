use crate::pane::PaneSplitDirection;
use bevy::math::Vec2;
use bevy::prelude::Entity;
#[cfg(test)]
use vmux_ecs::page::PageSplitPreference;
use vmux_ecs::page::{PagePlacement, PageSplitAxis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Placement {
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
pub(crate) struct LeafInfo {
    pub(crate) pane: Entity,
    pub(crate) placements: Vec<PagePlacement>,
    pub(crate) spawn_seq: u64,
    pub(crate) size: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReuseHit {
    pub(crate) tab: Entity,
    pub(crate) stack: Entity,
}

fn longer_axis(size: Vec2) -> PaneSplitDirection {
    if size.x >= size.y {
        PaneSplitDirection::Row
    } else {
        PaneSplitDirection::Column
    }
}

fn newest_primary_leaf(leaves: &[LeafInfo]) -> Option<&LeafInfo> {
    leaves
        .iter()
        .filter(|leaf| !leaf.placements.iter().any(|placement| placement.auxiliary))
        .max_by_key(|leaf| leaf.spawn_seq)
}

fn newest_leaf_in_group<'a>(leaves: &'a [LeafInfo], group: &str) -> Option<&'a LeafInfo> {
    leaves
        .iter()
        .filter(|leaf| leaf.placements.len() == 1 && leaf.placements[0].group == group)
        .max_by_key(|leaf| leaf.spawn_seq)
}

fn preferred_split(
    placement: PagePlacement,
    leaves: &[LeafInfo],
) -> Option<(Entity, PaneSplitDirection)> {
    let split = placement.split?;
    if !leaves.iter().all(|leaf| {
        leaf.placements
            .iter()
            .all(|placement| split.allowed_groups.contains(&placement.group))
    }) {
        return None;
    }
    let anchor = newest_leaf_in_group(leaves, split.anchor_group)?;
    let axis = match split.axis {
        PageSplitAxis::Row => PaneSplitDirection::Row,
        PageSplitAxis::Column => PaneSplitDirection::Column,
    };
    Some((anchor.pane, axis))
}

impl Placement {
    pub(crate) fn resolve(
        placement: PagePlacement,
        reuse: Option<ReuseHit>,
        leaves: &[LeafInfo],
        self_pane: Entity,
    ) -> Self {
        if let Some(hit) = reuse {
            return Self::Focus {
                tab: hit.tab,
                stack: hit.stack,
            };
        }
        if let Some(empty) = leaves.iter().find(|leaf| leaf.placements.is_empty()) {
            return Self::AddTab { pane: empty.pane };
        }
        if placement.auxiliary {
            if let Some(existing) = newest_leaf_in_group(leaves, placement.group) {
                return Self::AddTab {
                    pane: existing.pane,
                };
            }
            if let Some(anchor) = newest_primary_leaf(leaves) {
                return Self::Spiral {
                    anchor: anchor.pane,
                    axis: longer_axis(anchor.size),
                };
            }
            return Self::AddTab { pane: self_pane };
        }
        if let Some(same) = newest_leaf_in_group(leaves, placement.group) {
            return Self::AddTab { pane: same.pane };
        }
        if let Some((anchor, axis)) = preferred_split(placement, leaves) {
            return Self::Spiral { anchor, axis };
        }
        if let Some(anchor) = newest_primary_leaf(leaves) {
            return Self::Spiral {
                anchor: anchor.pane,
                axis: longer_axis(anchor.size),
            };
        }
        if let Some(auxiliary) = leaves
            .iter()
            .find(|leaf| leaf.placements.iter().any(|placement| placement.auxiliary))
        {
            return Self::Spiral {
                anchor: auxiliary.pane,
                axis: longer_axis(auxiliary.size),
            };
        }
        Self::AddTab { pane: self_pane }
    }

    pub(crate) fn split_anchor(leaves: &[LeafInfo], self_pane: Entity) -> Entity {
        newest_primary_leaf(leaves)
            .or_else(|| {
                leaves
                    .iter()
                    .find(|leaf| leaf.placements.iter().any(|placement| placement.auxiliary))
            })
            .map(|leaf| leaf.pane)
            .unwrap_or(self_pane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(value: u64) -> Entity {
        Entity::from_bits(value)
    }

    fn page(group: &'static str) -> PagePlacement {
        PagePlacement {
            group,
            ..PagePlacement::DEFAULT
        }
    }

    fn auxiliary(group: &'static str) -> PagePlacement {
        PagePlacement {
            group,
            auxiliary: true,
            ..PagePlacement::DEFAULT
        }
    }

    fn terminal() -> PagePlacement {
        PagePlacement {
            group: "terminal",
            split: Some(PageSplitPreference {
                anchor_group: "browser",
                allowed_groups: &["assistant", "browser"],
                axis: PageSplitAxis::Column,
            }),
            ..PagePlacement::DEFAULT
        }
    }

    fn leaf(pane: u64, placements: &[PagePlacement], sequence: u64, size: (f32, f32)) -> LeafInfo {
        LeafInfo {
            pane: entity(pane),
            placements: placements.to_vec(),
            spawn_seq: sequence,
            size: Vec2::new(size.0, size.1),
        }
    }

    #[test]
    fn reuse_wins() {
        let hit = ReuseHit {
            tab: entity(1),
            stack: entity(2),
        };
        assert_eq!(
            Placement::resolve(
                page("browser"),
                Some(hit),
                &[leaf(10, &[page("browser")], 5, (800.0, 600.0))],
                entity(10),
            ),
            Placement::Focus {
                tab: entity(1),
                stack: entity(2),
            }
        );
    }

    #[test]
    fn newest_pure_group_receives_the_page() {
        assert_eq!(
            Placement::resolve(
                page("terminal"),
                None,
                &[
                    leaf(10, &[page("terminal")], 1, (800.0, 600.0)),
                    leaf(20, &[page("terminal")], 9, (800.0, 600.0)),
                    leaf(30, &[page("files")], 12, (800.0, 600.0)),
                ],
                entity(1),
            ),
            Placement::AddTab { pane: entity(20) }
        );
    }

    #[test]
    fn mixed_group_is_not_reused_as_a_bucket() {
        assert_eq!(
            Placement::resolve(
                page("browser"),
                None,
                &[leaf(
                    20,
                    &[page("files"), page("browser")],
                    9,
                    (900.0, 400.0),
                )],
                entity(20),
            ),
            Placement::Spiral {
                anchor: entity(20),
                axis: PaneSplitDirection::Row,
            }
        );
    }

    #[test]
    fn split_anchor_ignores_newer_auxiliary_pages() {
        assert_eq!(
            Placement::split_anchor(
                &[
                    leaf(10, &[page("terminal")], 9, (800.0, 600.0)),
                    leaf(20, &[page("browser")], 12, (800.0, 600.0)),
                    leaf(30, &[auxiliary("assistant")], 50, (800.0, 600.0)),
                ],
                entity(30),
            ),
            entity(20)
        );
    }

    #[test]
    fn empty_leaf_is_filled_first() {
        assert_eq!(
            Placement::resolve(
                page("browser"),
                None,
                &[leaf(10, &[], 1, (800.0, 600.0))],
                entity(10),
            ),
            Placement::AddTab { pane: entity(10) }
        );
    }

    #[test]
    fn preferred_split_uses_manifest_policy() {
        assert_eq!(
            Placement::resolve(
                terminal(),
                None,
                &[
                    leaf(1, &[auxiliary("assistant")], 1, (800.0, 900.0)),
                    leaf(2, &[page("browser")], 10, (900.0, 400.0)),
                ],
                entity(1),
            ),
            Placement::Spiral {
                anchor: entity(2),
                axis: PaneSplitDirection::Column,
            }
        );
    }

    #[test]
    fn auxiliary_pages_share_their_bucket() {
        assert_eq!(
            Placement::resolve(
                auxiliary("assistant"),
                None,
                &[
                    leaf(1, &[auxiliary("assistant")], 1, (800.0, 900.0)),
                    leaf(2, &[page("browser")], 9, (900.0, 400.0)),
                ],
                entity(2),
            ),
            Placement::AddTab { pane: entity(1) }
        );
    }

    #[test]
    fn primary_page_bootstraps_from_auxiliary_leaf() {
        assert_eq!(
            Placement::resolve(
                page("browser"),
                None,
                &[leaf(1, &[auxiliary("assistant")], 1, (1600.0, 900.0),)],
                entity(1),
            ),
            Placement::Spiral {
                anchor: entity(1),
                axis: PaneSplitDirection::Row,
            }
        );
    }
}
