#[cfg(bevy_linked)]
use bevy_ecs::component::Component;
#[cfg(bevy_linked)]
use bevy_ecs::reflect::ReflectComponent;
#[cfg(bevy_linked)]
use bevy_reflect::{Reflect, std_traits::ReflectDefault};

#[cfg_attr(bevy_linked, derive(Component, Reflect))]
#[cfg_attr(bevy_linked, reflect(Component, Default))]
#[cfg_attr(bevy_linked, type_path = "vmux_header::system")]
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct PageMetadata {
    pub title: String,
    pub url: String,
    pub icon: crate::PageIcon,
    pub bg_color: Option<String>,
}

impl PageMetadata {
    pub fn title_with<'a>(&'a self, identity: Option<&'a PageIdentity>) -> &'a str {
        match identity.and_then(|identity| identity.title.as_deref()) {
            Some(title) if !title.is_empty() => title,
            _ => &self.title,
        }
    }

    pub fn icon_with<'a>(&'a self, identity: Option<&'a PageIdentity>) -> &'a crate::PageIcon {
        match identity.and_then(|identity| identity.icon.as_ref()) {
            Some(icon) if !icon.is_none() => icon,
            _ => &self.icon,
        }
    }
}

#[cfg_attr(bevy_linked, derive(Component))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageIdentity {
    pub title: Option<String>,
    pub icon: Option<crate::PageIcon>,
}

impl From<String> for PageIdentity {
    fn from(title: String) -> Self {
        Self {
            title: Some(title),
            icon: None,
        }
    }
}

impl From<&str> for PageIdentity {
    fn from(title: &str) -> Self {
        Self::from(title.to_string())
    }
}
