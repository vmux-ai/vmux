#[cfg(bevy_linked)]
use bevy_ecs::component::Component;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(bevy_linked, derive(Component))]
pub struct ErrorPageData {
    pub title_message_id: String,
    pub message: String,
    pub url: String,
}

pub const FAILED_TO_LOAD: &str = "error-page-failed-load";
pub const NOT_FOUND: &str = "error-page-not-found";
