#[cfg(bevy_linked)]
use bevy_ecs::component::Component;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(bevy_linked, derive(Component))]
pub struct ErrorPageData {
    pub title_message_id: String,
    pub message: String,
    pub url: String,
}

impl ErrorPageData {
    pub fn failed_to_load(url: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title_message_id: FAILED_TO_LOAD.to_string(),
            message: message.into(),
            url: url.into(),
        }
    }

    pub fn not_found(url: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title_message_id: NOT_FOUND.to_string(),
            message: message.into(),
            url: url.into(),
        }
    }
}

pub const FAILED_TO_LOAD: &str = "error-page-failed-load";
pub const NOT_FOUND: &str = "error-page-not-found";
