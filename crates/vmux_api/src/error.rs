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

impl ErrorPageData {
    pub fn failed_to_load(url: &str, message: &str) -> Self {
        Self {
            title: FAILED_TO_LOAD.to_string(),
            message: message.to_string(),
            url: url.to_string(),
        }
    }

    pub fn not_found(url: &str) -> Self {
        Self {
            title: NOT_FOUND.to_string(),
            message: String::new(),
            url: url.to_string(),
        }
    }

    pub fn heading_message_id(&self) -> Option<&'static str> {
        match self.title.as_str() {
            FAILED_TO_LOAD => Some("error-page-failed-load"),
            NOT_FOUND => Some("error-page-not-found"),
            _ => None,
        }
    }
}
