/// Theme settings shared between Bevy host and Dioxus webview apps.
pub const THEME_EVENT: &str = "theme";

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct ThemeEvent {
    /// Border radius in CSS pixels.
    pub radius: f32,
    pub locale: String,
    pub catalog: Option<String>,
}

#[cfg(test)]
#[path = "theme.test.rs"]
mod tests;
