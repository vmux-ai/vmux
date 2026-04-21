/// Theme settings shared between Bevy host and Dioxus webview apps.

pub const THEME_EVENT: &str = "theme";

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ThemeEvent {
    /// Border radius in CSS pixels.
    pub radius: f32,
}
