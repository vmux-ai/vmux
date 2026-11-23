use bevy::prelude::*;
use serde::{Deserialize, Serialize};
pub use vmux_command::ResolvedLocale;

#[derive(Clone, Debug, Default, Deserialize, Serialize, Resource)]
pub struct LayoutSettings {
    #[serde(default = "LayoutSettings::default_radius")]
    pub radius: f32,
    #[serde(default)]
    pub window: WindowSettings,
    #[serde(default)]
    pub pane: PaneSettings,
    #[serde(default)]
    pub side_sheet: SideSheetSettings,
    #[serde(default)]
    pub focus_ring: FocusRingSettings,
}

impl LayoutSettings {
    fn default_radius() -> f32 {
        8.0
    }
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct ConfirmCloseSettings {
    pub enabled: bool,
}

impl Default for ConfirmCloseSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SideSheetSettings {
    #[serde(default = "SideSheetSettings::default_width")]
    pub width: f32,
}

impl SideSheetSettings {
    fn default_width() -> f32 {
        crate::event::SIDE_SHEET_WIDTH_PX
    }
}

impl Default for SideSheetSettings {
    fn default() -> Self {
        Self {
            width: Self::default_width(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WindowSettings {
    #[serde(default = "WindowSettings::default_padding")]
    pub padding: f32,
}

impl WindowSettings {
    fn default_padding() -> f32 {
        crate::event::WINDOW_PAD_PX
    }

    pub fn pad_top(&self) -> f32 {
        self.padding
    }

    pub fn pad_right(&self) -> f32 {
        self.padding
    }

    pub fn pad_bottom(&self) -> f32 {
        self.padding
    }

    pub fn pad_left(&self) -> f32 {
        self.padding
    }
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            padding: Self::default_padding(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FocusRingColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Default for FocusRingColor {
    fn default() -> Self {
        Self {
            r: 0.52,
            g: 0.52,
            b: 0.58,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FocusRingSettings {
    #[serde(default = "FocusRingSettings::default_width")]
    pub width: f32,
    #[serde(default = "FocusRingSettings::default_color")]
    pub color: FocusRingColor,
}

impl FocusRingSettings {
    fn default_width() -> f32 {
        2.0
    }

    fn default_color() -> FocusRingColor {
        FocusRingColor::default()
    }
}

impl Default for FocusRingSettings {
    fn default() -> Self {
        Self {
            width: Self::default_width(),
            color: Self::default_color(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaneSettings {
    #[serde(default)]
    pub gap: f32,
    #[serde(default = "PaneSettings::default_focus_follows_mouse")]
    pub focus_follows_mouse: bool,
}

impl PaneSettings {
    fn default_focus_follows_mouse() -> bool {
        true
    }
}

impl Default for PaneSettings {
    fn default() -> Self {
        Self {
            gap: 0.0,
            focus_follows_mouse: Self::default_focus_follows_mouse(),
        }
    }
}
