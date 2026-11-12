use bevy_ecs::prelude::*;
use bevy_transform::components::Transform;
use vmux_flex::{AlignItems, FlexDirection, JustifyContent, Node, Val, Visibility};

use crate::{Pane, PaneSize, PaneSplit, PaneSplitDirection};

use crate::PANE_GAP_PX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneSplitGaps {
    pub column_gap: Val,
    pub row_gap: Val,
}

impl Pane {
    pub fn bundle() -> impl Bundle {
        (
            Self,
            PaneSize::default(),
            Transform::default(),
            Node {
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                align_items: AlignItems::Stretch,
                justify_content: JustifyContent::Stretch,
                ..Default::default()
            },
        )
    }

    pub fn split_bundle(direction: PaneSplitDirection) -> impl Bundle {
        let gaps = direction.gaps(PANE_GAP_PX);
        (
            Self,
            PaneSplit { direction },
            PaneSize::default(),
            Transform::default(),
            Visibility::default(),
            Node {
                flex_grow: 1.0,
                flex_direction: direction.flex_direction(),
                column_gap: gaps.column_gap,
                row_gap: gaps.row_gap,
                align_items: AlignItems::Stretch,
                ..Default::default()
            },
        )
    }
}

impl PaneSplitDirection {
    pub fn flex_direction(self) -> FlexDirection {
        match self {
            Self::Row => FlexDirection::Row,
            Self::Column => FlexDirection::Column,
        }
    }

    pub fn gaps(self, gap: f32) -> PaneSplitGaps {
        match self {
            Self::Row => PaneSplitGaps {
                column_gap: Val::Px(gap),
                row_gap: Val::Px(0.0),
            },
            Self::Column => PaneSplitGaps {
                column_gap: Val::Px(0.0),
                row_gap: Val::Px(gap),
            },
        }
    }
}

impl PaneSplit {
    pub fn apply_gaps(&self, node: &mut Node, gap: f32) {
        let gaps = self.direction.gaps(gap);
        node.column_gap = gaps.column_gap;
        node.row_gap = gaps.row_gap;
    }
}
