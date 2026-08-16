//! Where a laid-out node ended up, and the corners every caller actually wants.

use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;

/// Space reserved inside a node's own box, resolved to physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Left and top.
    pub min: Vec2,
    /// Right and bottom.
    pub max: Vec2,
}

/// The rectangle a laid-out node occupies, in physical pixels, origin at the window's top-left.
///
/// The layout engine reports a node's position as its *centre*, which is almost never what a
/// caller wants: everything downstream is placing a native view, cropping a framebuffer or hit
/// testing a cursor, and all three want corners. Deriving them is the same four lines at every
/// site, and a slipped sign there produces a plausible rectangle in the wrong place — nothing
/// type-checks it and no test sees it. So the conversion happens once, here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeRect {
    pub size: Vec2,
    pub center: Vec2,
    pub padding: Insets,
    /// Reciprocal of the render target's scale factor — `0.5` on a Retina display.
    pub inverse_scale_factor: f32,
}

/// A default rectangle is unscaled, not scaled by zero.
///
/// Deriving this would leave `inverse_scale_factor` at `0.0`, which is not a neutral value: it
/// makes [`NodeRect::scale`] report a million and [`NodeRect::to_logical`] shrink by the same,
/// so a rectangle built from [`NodeRect::from_origin`] would be silently unusable in logical space.
impl Default for NodeRect {
    fn default() -> Self {
        Self {
            size: Vec2::ZERO,
            center: Vec2::ZERO,
            padding: Insets::default(),
            inverse_scale_factor: 1.0,
        }
    }
}

impl NodeRect {
    pub fn of(computed: &ComputedNode, transform: &UiGlobalTransform) -> Self {
        Self {
            size: computed.size,
            center: transform.transform_point2(Vec2::ZERO),
            padding: Insets {
                min: computed.padding.min_inset,
                max: computed.padding.max_inset,
            },
            inverse_scale_factor: computed.inverse_scale_factor,
        }
    }

    /// A rectangle of `size` with its top-left corner at the origin.
    pub fn from_origin(size: Vec2) -> Self {
        Self {
            size,
            center: size * 0.5,
            ..Self::default()
        }
    }

    /// Top-left corner.
    pub fn min(self) -> Vec2 {
        self.center - self.size * 0.5
    }

    /// Bottom-right corner.
    pub fn max(self) -> Vec2 {
        self.center + self.size * 0.5
    }

    /// Inclusive on every edge, matching the pointer tests this replaces.
    pub fn contains(self, point: Vec2) -> bool {
        let min = self.min();
        let max = self.max();
        point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
    }

    /// Do the two rectangles share any rows? Directional pane navigation uses this to reject a
    /// neighbour that lies the right way but does not sit beside the pane at all.
    pub fn overlaps_rows(self, other: Self) -> bool {
        self.min().y.max(other.min().y) < self.max().y.min(other.max().y)
    }

    /// Do the two rectangles share any columns?
    pub fn overlaps_columns(self, other: Self) -> bool {
        self.min().x.max(other.min().x) < self.max().x.min(other.max().x)
    }

    /// Nothing can be placed against this rectangle — it has no area, or the layout produced a
    /// value arithmetic cannot survive.
    pub fn is_empty(self) -> bool {
        !(self.size.x > 0.0
            && self.size.y > 0.0
            && self.size.x.is_finite()
            && self.size.y.is_finite())
    }

    /// Physical pixels per logical pixel.
    pub fn scale(self) -> f32 {
        1.0 / self.inverse_scale_factor.max(1.0e-6)
    }

    /// A window-space point in this rectangle's own logical coordinates, or `None` if it falls
    /// outside. This is what a native view wants a cursor position expressed in.
    pub fn local_point(self, point: Vec2) -> Option<Vec2> {
        if !self.contains(point) {
            return None;
        }
        Some((point - self.min()) / self.scale())
    }

    /// The same rectangle in logical pixels, which is what AppKit frames and CSS offsets want.
    pub fn to_logical(self) -> Self {
        let inverse_scale = self.inverse_scale_factor.max(1.0e-6);
        Self {
            size: self.size * inverse_scale,
            center: self.center * inverse_scale,
            padding: Insets {
                min: self.padding.min * inverse_scale,
                max: self.padding.max * inverse_scale,
            },
            inverse_scale_factor: 1.0,
        }
    }

    /// Size including padding. `size` is the content box, so a node whose children are inset by
    /// its padding covers more than `size` reports.
    pub fn padding_box(self) -> Vec2 {
        self.size + self.padding.min + self.padding.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl NodeRect {
        fn at(center: Vec2, size: Vec2) -> Self {
            Self {
                size,
                center,
                ..default()
            }
        }
    }

    /// A slipped sign here puts every native view in the wrong half of the window.
    #[test]
    fn corners_sit_half_a_size_either_side_of_the_centre() {
        let rect = NodeRect::at(Vec2::new(400.0, 300.0), Vec2::new(200.0, 100.0));

        assert_eq!(rect.min(), Vec2::new(300.0, 250.0));
        assert_eq!(rect.max(), Vec2::new(500.0, 350.0));
    }

    #[test]
    fn edges_and_corners_count_as_inside() {
        let rect = NodeRect::at(Vec2::new(100.0, 100.0), Vec2::new(50.0, 50.0));

        assert!(rect.contains(Vec2::new(75.0, 75.0)));
        assert!(rect.contains(Vec2::new(125.0, 125.0)));
        assert!(rect.contains(Vec2::splat(100.0)));
        assert!(!rect.contains(Vec2::new(74.9, 100.0)));
        assert!(!rect.contains(Vec2::new(100.0, 125.1)));
    }

    /// The header on a Retina display: 1544x168 physical at scale 2 is 772x84 logical, and its
    /// left edge lands 8 logical pixels in. These are the numbers `layout_fixed_offsets_from_computed`
    /// has always produced.
    #[test]
    fn halving_a_retina_rect_gives_the_logical_one() {
        let rect = NodeRect {
            inverse_scale_factor: 0.5,
            ..NodeRect::at(Vec2::new(788.0, 84.0), Vec2::new(1544.0, 168.0))
        };

        let logical = rect.to_logical();

        assert_eq!(logical.size, Vec2::new(772.0, 84.0));
        assert_eq!(logical.min(), Vec2::new(8.0, 0.0));
        assert_eq!(logical.scale(), 1.0);
    }

    /// A cursor over a Retina view arrives in physical window pixels and has to reach the view as
    /// logical pixels from its own top-left, or every hover lands in the wrong place. The 400x300
    /// view at (100, 50) on a scale-2 display is the case `refresh_active_windowed_hover` feeds.
    #[test]
    fn a_window_point_becomes_logical_pixels_from_the_rects_own_corner() {
        let rect = NodeRect {
            inverse_scale_factor: 0.5,
            ..NodeRect::at(Vec2::new(300.0, 200.0), Vec2::new(400.0, 300.0))
        };

        assert_eq!(rect.min(), Vec2::new(100.0, 50.0));
        assert_eq!(
            rect.local_point(Vec2::new(300.0, 250.0)),
            Some(Vec2::splat(100.0))
        );
        assert_eq!(rect.local_point(Vec2::new(100.0, 50.0)), Some(Vec2::ZERO));
        assert_eq!(rect.local_point(Vec2::new(99.0, 250.0)), None);
    }

    /// `size` is the content box, so the window root's own extent is bigger than it reports by
    /// exactly the padding the layout reserves for the glass border.
    #[test]
    fn the_padding_box_adds_both_insets() {
        let rect = NodeRect {
            padding: Insets {
                min: Vec2::new(4.0, 8.0),
                max: Vec2::new(6.0, 2.0),
            },
            ..NodeRect::at(Vec2::ZERO, Vec2::new(100.0, 50.0))
        };

        assert_eq!(rect.padding_box(), Vec2::new(110.0, 60.0));
    }

    /// A rectangle nobody gave a scale to is already in logical space, so converting it must be a
    /// no-op rather than a division by a millionth.
    #[test]
    fn an_unscaled_rect_survives_conversion_to_logical() {
        let rect = NodeRect::from_origin(Vec2::new(400.0, 300.0));

        assert_eq!(rect.scale(), 1.0);
        assert_eq!(rect.to_logical(), rect);
        assert_eq!(rect.min(), Vec2::ZERO);
    }

    #[test]
    fn a_rect_with_no_area_or_a_broken_one_is_empty() {
        assert!(NodeRect::at(Vec2::ZERO, Vec2::new(0.0, 10.0)).is_empty());
        assert!(NodeRect::at(Vec2::ZERO, Vec2::new(10.0, -1.0)).is_empty());
        assert!(NodeRect::at(Vec2::ZERO, Vec2::new(f32::NAN, 10.0)).is_empty());
        assert!(NodeRect::at(Vec2::ZERO, Vec2::new(f32::INFINITY, 10.0)).is_empty());
        assert!(!NodeRect::at(Vec2::ZERO, Vec2::new(1.0, 1.0)).is_empty());
    }
}
