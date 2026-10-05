#[vmux_api::ui_state(Default)]
pub struct ThemeUiState {
    pub radius: f32,
    pub locale: String,
    pub catalog: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_event_rkyv_roundtrip() {
        let original = ThemeUiState {
            radius: 8.0,
            locale: "ja".to_string(),
            catalog: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<ThemeUiState, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
