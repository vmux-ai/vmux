#[vmux_api::contract(Copy, Eq, Default)]
pub enum ThemeScheme {
    Light,
    #[default]
    Dark,
}

impl ThemeScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[vmux_api::ui_state(Default)]
pub struct UiTheme {
    pub radius: f32,
    pub locale: String,
    pub catalog: Option<String>,
    pub scheme: ThemeScheme,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_event_rkyv_roundtrip() {
        let original = UiTheme {
            radius: 8.0,
            locale: "ja".to_string(),
            catalog: None,
            scheme: ThemeScheme::Light,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).expect("serialize");
        let recovered =
            rkyv::from_bytes::<UiTheme, rkyv::rancor::Error>(&bytes).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
