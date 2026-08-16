//! What a caller asks for: the style a node is laid out under.

use bevy::prelude::*;

use crate::computed::ComputedNode;

/// A length, resolved against the render target when layout runs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Val {
    /// Sized by the layout algorithm rather than by this value.
    #[default]
    Auto,
    /// Logical pixels. Multiplied by the render target's scale factor before layout, which is why
    /// every computed result comes back in physical pixels.
    Px(f32),
    /// Percentage of the parent's corresponding axis, `0.0` to `100.0`.
    Percent(f32),
}

impl Val {
    pub const ZERO: Self = Self::Px(0.0);
}

/// Four lengths, one per edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRect {
    pub left: Val,
    pub right: Val,
    pub top: Val,
    pub bottom: Val,
}

impl UiRect {
    pub const ZERO: Self = Self::all(Val::ZERO);

    pub const fn all(value: Val) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }
}

impl Default for UiRect {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Which algorithm lays a node's children out, or whether it participates at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    #[default]
    Flex,
    /// Removed from layout entirely, along with its whole subtree.
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionType {
    /// Placed by the parent's flex algorithm; insets shift it from there.
    #[default]
    Relative,
    /// Taken out of flow and placed against the parent's padding box by its insets.
    Absolute,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignItems {
    #[default]
    Default,
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    Default,
    Stretch,
}

/// The style a node is laid out under. Its result lands on [`ComputedNode`].
///
/// A deliberate subset of CSS flexbox: the fields the shell actually sets, and no more. There is
/// no grid, no overflow, no margin and no border here because nothing asks for them, and a field
/// that exists but is never exercised is a field whose behaviour nobody has checked.
#[derive(Component, Clone, Debug, PartialEq)]
#[require(ComputedNode, Visibility)]
pub struct Node {
    pub display: Display,
    pub position_type: PositionType,
    pub left: Val,
    pub right: Val,
    pub top: Val,
    pub bottom: Val,
    pub width: Val,
    pub height: Val,
    pub min_width: Val,
    pub min_height: Val,
    pub padding: UiRect,
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Val,
    pub row_gap: Val,
    pub column_gap: Val,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            display: Display::default(),
            position_type: PositionType::default(),
            left: Val::Auto,
            right: Val::Auto,
            top: Val::Auto,
            bottom: Val::Auto,
            width: Val::Auto,
            height: Val::Auto,
            min_width: Val::Auto,
            min_height: Val::Auto,
            padding: UiRect::ZERO,
            flex_direction: FlexDirection::default(),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Val::Auto,
            row_gap: Val::ZERO,
            column_gap: Val::ZERO,
            align_items: AlignItems::default(),
            justify_content: JustifyContent::default(),
        }
    }
}
