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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_event_rkyv_roundtrip() {
        let original = ThemeEvent { radius: 8.0 };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<ThemeEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
