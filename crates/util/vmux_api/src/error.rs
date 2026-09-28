#[cfg(bevy_linked)]
use bevy_ecs::component::Component;

pub const ERROR_PAGE_URL: &str = "vmux://error/";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(bevy_linked, derive(Component))]
pub struct ErrorPageData {
    pub title: String,
    pub message: String,
    pub url: String,
}

pub const FAILED_TO_LOAD: &str = "Page failed to load";
pub const NOT_FOUND: &str = "Page not found";
