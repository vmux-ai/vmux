#[vmux_macro::contract(Copy, Default)]
pub struct DropdownAnchor {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl DropdownAnchor {
    pub fn is_valid(self) -> bool {
        self.left.is_finite()
            && self.top.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }
}
