use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub min: Vec2,
    pub max: Vec2,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ComputedNode {
    pub size: Vec2,
    pub center: Vec2,
    pub padding: Insets,
    pub inverse_scale_factor: f32,
}

impl Default for ComputedNode {
    fn default() -> Self {
        Self {
            size: Vec2::ZERO,
            center: Vec2::ZERO,
            padding: Insets::default(),
            inverse_scale_factor: 1.0,
        }
    }
}

impl ComputedNode {
    pub fn from_origin(size: Vec2) -> Self {
        Self {
            size,
            center: size * 0.5,
            ..Self::default()
        }
    }

    pub fn min(self) -> Vec2 {
        self.center - self.size * 0.5
    }

    pub fn max(self) -> Vec2 {
        self.center + self.size * 0.5
    }

    pub fn contains(self, point: Vec2) -> bool {
        let min = self.min();
        let max = self.max();
        point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
    }

    pub fn overlaps_rows(self, other: Self) -> bool {
        self.min().y.max(other.min().y) < self.max().y.min(other.max().y)
    }

    pub fn overlaps_columns(self, other: Self) -> bool {
        self.min().x.max(other.min().x) < self.max().x.min(other.max().x)
    }

    pub fn is_empty(self) -> bool {
        !(self.size.x > 0.0
            && self.size.y > 0.0
            && self.size.x.is_finite()
            && self.size.y.is_finite())
    }

    pub fn scale(self) -> f32 {
        1.0 / self.inverse_scale_factor.max(1.0e-6)
    }

    pub fn local_point(self, point: Vec2) -> Option<Vec2> {
        if !self.contains(point) {
            return None;
        }
        Some((point - self.min()) / self.scale())
    }

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

    pub fn padding_box(self) -> Vec2 {
        self.size + self.padding.min + self.padding.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ComputedNode {
        fn at(center: Vec2, size: Vec2) -> Self {
            Self {
                size,
                center,
                ..default()
            }
        }
    }

    #[test]
    fn corners_sit_half_a_size_either_side_of_the_centre() {
        let rect = ComputedNode::at(Vec2::new(400.0, 300.0), Vec2::new(200.0, 100.0));

        assert_eq!(rect.min(), Vec2::new(300.0, 250.0));
        assert_eq!(rect.max(), Vec2::new(500.0, 350.0));
    }

    #[test]
    fn edges_and_corners_count_as_inside() {
        let rect = ComputedNode::at(Vec2::new(100.0, 100.0), Vec2::new(50.0, 50.0));

        assert!(rect.contains(Vec2::new(75.0, 75.0)));
        assert!(rect.contains(Vec2::new(125.0, 125.0)));
        assert!(!rect.contains(Vec2::new(74.9, 100.0)));
    }

    #[test]
    fn halving_a_retina_rect_gives_the_logical_one() {
        let rect = ComputedNode {
            inverse_scale_factor: 0.5,
            ..ComputedNode::at(Vec2::new(788.0, 84.0), Vec2::new(1544.0, 168.0))
        };

        let logical = rect.to_logical();

        assert_eq!(logical.size, Vec2::new(772.0, 84.0));
        assert_eq!(logical.min(), Vec2::new(8.0, 0.0));
        assert_eq!(logical.scale(), 1.0);
    }

    #[test]
    fn a_window_point_becomes_logical_pixels_from_the_rects_own_corner() {
        let rect = ComputedNode {
            inverse_scale_factor: 0.5,
            ..ComputedNode::at(Vec2::new(300.0, 200.0), Vec2::new(400.0, 300.0))
        };

        assert_eq!(rect.min(), Vec2::new(100.0, 50.0));
        assert_eq!(
            rect.local_point(Vec2::new(300.0, 250.0)),
            Some(Vec2::splat(100.0))
        );
        assert_eq!(rect.local_point(Vec2::new(99.0, 250.0)), None);
    }

    #[test]
    fn the_padding_box_adds_both_insets() {
        let rect = ComputedNode {
            padding: Insets {
                min: Vec2::new(4.0, 8.0),
                max: Vec2::new(6.0, 2.0),
            },
            ..ComputedNode::at(Vec2::ZERO, Vec2::new(100.0, 50.0))
        };

        assert_eq!(rect.padding_box(), Vec2::new(110.0, 60.0));
    }

    #[test]
    fn an_unscaled_rect_survives_conversion_to_logical() {
        let rect = ComputedNode::from_origin(Vec2::new(400.0, 300.0));

        assert_eq!(rect.scale(), 1.0);
        assert_eq!(rect.to_logical(), rect);
        assert_eq!(rect.min(), Vec2::ZERO);
    }

    #[test]
    fn a_rect_with_no_area_or_a_broken_one_is_empty() {
        assert!(ComputedNode::at(Vec2::ZERO, Vec2::new(0.0, 10.0)).is_empty());
        assert!(ComputedNode::at(Vec2::ZERO, Vec2::new(f32::NAN, 10.0)).is_empty());
        assert!(!ComputedNode::at(Vec2::ZERO, Vec2::new(1.0, 1.0)).is_empty());
    }
}
