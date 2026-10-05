use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::computed::{ComputedNode, Insets};
use crate::tree::FlexTree;

#[derive(SystemParam)]
pub(crate) struct GeometryWriter<'w, 's> {
    pub(crate) children: Query<'w, 's, &'static Children>,
    out: Query<'w, 's, &'static mut ComputedNode>,
}

impl GeometryWriter<'_, '_> {
    pub(crate) fn descend(
        &mut self,
        tree: &FlexTree,
        inverse_scale_factor: f32,
        entity: Entity,
        parent_size: Vec2,
        parent_center: Vec2,
    ) {
        let Some(layout) = tree.layout_of(entity) else {
            return;
        };
        let size = Vec2::new(layout.size.width, layout.size.height);
        let location = Vec2::new(layout.location.x, layout.location.y);
        let padding = Insets {
            min: Vec2::new(layout.padding.left, layout.padding.top),
            max: Vec2::new(layout.padding.right, layout.padding.bottom),
        };
        let center = parent_center + location + 0.5 * (size - parent_size);

        if let Ok(mut computed) = self.out.get_mut(entity) {
            if computed.size != size
                || computed.center != center
                || computed.inverse_scale_factor != inverse_scale_factor
            {
                computed.size = size;
                computed.center = center;
                computed.inverse_scale_factor = inverse_scale_factor;
            }
            if computed.padding != padding {
                computed.bypass_change_detection().padding = padding;
            }
        }

        let Ok(kids) = self.children.get(entity) else {
            return;
        };
        let children = kids.iter().collect::<Vec<_>>();
        for child in children {
            self.descend(tree, inverse_scale_factor, child, size, center);
        }
    }
}
