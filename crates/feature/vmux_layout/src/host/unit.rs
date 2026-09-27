use bevy::prelude::*;

pub const PIXELS_PER_METER: f32 = 100.0;

pub trait WindowExt {
    fn logical_size(&self) -> Vec2;
    fn aspect(&self) -> f32;
    fn meters(&self) -> Vec2;
}

impl WindowExt for Window {
    fn logical_size(&self) -> Vec2 {
        Vec2::new(self.width(), self.height())
    }

    fn aspect(&self) -> f32 {
        let size = self.logical_size();
        if size.y <= 0.0 { 1.0 } else { size.x / size.y }
    }

    fn meters(&self) -> Vec2 {
        self.logical_size() / PIXELS_PER_METER
    }
}
