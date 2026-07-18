use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Resource)]
pub struct LayoutSettings {
    /// Corner radius (px) applied across the design system: pane corner clip,
    /// focus ring, and the CEF CSS `--radius` variable.
    #[serde(default = "default_radius")]
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

#[derive(Clone, Copy, Debug, Resource)]
pub struct ConfirmCloseSettings {
    pub enabled: bool,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct EffectiveStartupUrl(pub String);

#[derive(Resource, Clone, Debug, Default)]
pub struct EffectiveStartupDir(pub Option<(Entity, std::path::PathBuf)>);

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct EffectiveStartupDirConfigured(pub bool);

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EffectiveStartupDirSet;

impl Default for ConfirmCloseSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SideSheetSettings {
    #[serde(default = "default_side_sheet_width")]
    pub width: f32,
}

impl Default for SideSheetSettings {
    fn default() -> Self {
        Self {
            width: default_side_sheet_width(),
        }
    }
}

fn default_side_sheet_width() -> f32 {
    280.0
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WindowSettings {
    #[serde(default = "default_window_padding")]
    pub padding: f32,
}

impl WindowSettings {
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
            padding: default_window_padding(),
        }
    }
}

fn default_window_padding() -> f32 {
    crate::event::WINDOW_PAD_PX
}

fn default_radius() -> f32 {
    8.0
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
    #[serde(default = "default_focus_ring_width")]
    pub width: f32,
    #[serde(default = "default_focus_ring_color")]
    pub color: FocusRingColor,
}

impl Default for FocusRingSettings {
    fn default() -> Self {
        Self {
            width: default_focus_ring_width(),
            color: default_focus_ring_color(),
        }
    }
}

fn default_focus_ring_width() -> f32 {
    2.0
}

fn default_focus_ring_color() -> FocusRingColor {
    FocusRingColor::default()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PaneSettings {
    #[serde(default)]
    pub gap: f32,
}
