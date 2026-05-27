use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, Resource)]
pub struct LayoutSettings {
    /// Corner radius (px) applied across the design system: pane corner clip,
    /// focus ring, and the CEF CSS `--radius` variable.
    #[serde(default)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct WindowSettings {
    #[serde(default)]
    pub padding: f32,
    #[serde(default)]
    pub padding_top: Option<f32>,
    #[serde(default)]
    pub padding_right: Option<f32>,
    #[serde(default)]
    pub padding_bottom: Option<f32>,
    #[serde(default)]
    pub padding_left: Option<f32>,
}

impl WindowSettings {
    pub fn pad_top(&self) -> f32 {
        self.padding_top.unwrap_or(self.padding)
    }

    pub fn pad_right(&self) -> f32 {
        self.padding_right.unwrap_or(self.padding)
    }

    pub fn pad_bottom(&self) -> f32 {
        self.padding_bottom.unwrap_or(self.padding)
    }

    pub fn pad_left(&self) -> f32 {
        self.padding_left.unwrap_or(self.padding)
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
pub struct FocusRingGlow {
    #[serde(default = "default_outline_glow_spread")]
    pub spread: f32,
    #[serde(default = "default_outline_glow_intensity")]
    pub intensity: f32,
}

impl Default for FocusRingGlow {
    fn default() -> Self {
        Self {
            spread: default_outline_glow_spread(),
            intensity: default_outline_glow_intensity(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FocusRingGradient {
    #[serde(default = "default_outline_gradient_enabled")]
    pub enabled: bool,
    #[serde(default = "default_outline_gradient_speed")]
    pub speed: f32,
    #[serde(default = "default_outline_gradient_cycles")]
    pub cycles: f32,
    #[serde(default = "default_outline_gradient_accent")]
    pub accent: FocusRingColor,
}

impl Default for FocusRingGradient {
    fn default() -> Self {
        Self {
            enabled: default_outline_gradient_enabled(),
            speed: default_outline_gradient_speed(),
            cycles: default_outline_gradient_cycles(),
            accent: default_outline_gradient_accent(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FocusRingSettings {
    #[serde(default = "default_focus_ring_width")]
    pub width: f32,
    #[serde(default = "default_focus_ring_color")]
    pub color: FocusRingColor,
    #[serde(default)]
    pub glow: FocusRingGlow,
    #[serde(default)]
    pub gradient: FocusRingGradient,
}

impl Default for FocusRingSettings {
    fn default() -> Self {
        Self {
            width: default_focus_ring_width(),
            color: default_focus_ring_color(),
            glow: FocusRingGlow::default(),
            gradient: FocusRingGradient::default(),
        }
    }
}

fn default_focus_ring_width() -> f32 {
    2.0
}

fn default_focus_ring_color() -> FocusRingColor {
    FocusRingColor::default()
}

fn default_outline_glow_spread() -> f32 {
    8.0
}

fn default_outline_glow_intensity() -> f32 {
    0.45
}

fn default_outline_gradient_enabled() -> bool {
    true
}

fn default_outline_gradient_speed() -> f32 {
    0.6
}

fn default_outline_gradient_cycles() -> f32 {
    1.0
}

fn default_outline_gradient_accent() -> FocusRingColor {
    FocusRingColor {
        r: 0.7,
        g: 0.7,
        b: 0.78,
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PaneSettings {
    #[serde(default)]
    pub gap: f32,
}
