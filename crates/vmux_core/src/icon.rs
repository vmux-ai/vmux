use serde::{Deserialize, Serialize};

#[cfg_attr(not(target_arch = "wasm32"), derive(bevy_reflect::Reflect))]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum BuiltinIcon {
    Terminal,
    Files,
    Server,
    Settings,
    Clock,
    Layers,
    Users,
    Sparkles,
    Activity,
    Puzzle,
}

#[cfg_attr(not(target_arch = "wasm32"), derive(bevy_reflect::Reflect))]
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub enum PageIcon {
    #[default]
    None,
    Favicon(String),
    Builtin(BuiltinIcon),
}

impl PageIcon {
    pub fn favicon(url: impl Into<String>) -> Self {
        let url = url.into();
        if url.is_empty() {
            Self::None
        } else {
            Self::Favicon(url)
        }
    }

    pub fn favicon_url(&self) -> &str {
        match self {
            Self::Favicon(url) => url.as_str(),
            _ => "",
        }
    }

    pub fn builtin(&self) -> Option<BuiltinIcon> {
        match self {
            Self::Builtin(icon) => Some(*icon),
            _ => None,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicon_constructor_collapses_empty_to_none() {
        assert_eq!(PageIcon::favicon(""), PageIcon::None);
        assert_eq!(
            PageIcon::favicon("https://x/fav.ico"),
            PageIcon::Favicon("https://x/fav.ico".to_string())
        );
    }

    #[test]
    fn accessors() {
        assert_eq!(PageIcon::Favicon("u".into()).favicon_url(), "u");
        assert_eq!(PageIcon::Builtin(BuiltinIcon::Users).favicon_url(), "");
        assert_eq!(
            PageIcon::Builtin(BuiltinIcon::Users).builtin(),
            Some(BuiltinIcon::Users)
        );
        assert!(PageIcon::None.is_none());
        assert_eq!(PageIcon::default(), PageIcon::None);
    }
}
