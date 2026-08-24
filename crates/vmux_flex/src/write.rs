use bevy::prelude::*;

use crate::computed::{ComputedNode, Insets};
use crate::tree::FlexTree;

pub(crate) struct GeometryWalk<'a> {
    pub(crate) tree: &'a FlexTree,
    pub(crate) inverse_scale_factor: f32,
}

impl GeometryWalk<'_> {
    pub(crate) fn descend(
        &self,
        entity: Entity,
        parent_size: Vec2,
        parent_center: Vec2,
        children: &Query<&Children>,
        out: &mut Query<&mut ComputedNode>,
    ) {
        let Some(layout) = self.tree.layout_of(entity) else {
            return;
        };
        let size = Vec2::new(layout.size.width, layout.size.height);
        let location = Vec2::new(layout.location.x, layout.location.y);
        let padding = Insets {
            min: Vec2::new(layout.padding.left, layout.padding.top),
            max: Vec2::new(layout.padding.right, layout.padding.bottom),
        };
        let center = parent_center + location + 0.5 * (size - parent_size);

        if let Ok(mut computed) = out.get_mut(entity) {
            if computed.size != size
                || computed.center != center
                || computed.inverse_scale_factor != self.inverse_scale_factor
            {
                computed.size = size;
                computed.center = center;
                computed.inverse_scale_factor = self.inverse_scale_factor;
            }
            if computed.padding != padding {
                computed.bypass_change_detection().padding = padding;
            }
        }

        let Ok(kids) = children.get(entity) else {
            return;
        };
        for child in kids.iter() {
            self.descend(child, size, center, children, out);
        }
    }
}
