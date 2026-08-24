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
    pub radius: f32,
    pub locale: String,
    pub catalog: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_event_rkyv_roundtrip() {
        let original = ThemeEvent {
            radius: 8.0,
            locale: "ja".to_string(),
            catalog: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<ThemeEvent, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
