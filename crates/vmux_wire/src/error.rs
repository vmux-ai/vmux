//! The failure the error page is showing.
//!
//! The page used to read its own address — `vmux://error/?title=&message=&url=` — which is only
//! answerable where a page has a `location` to read. Natively the host builds the page's
//! `VirtualDom`, so this goes into its root scope before the first render: on the host it is the
//! component recording why a view exists, and in the page it is the context that view reads.

#[cfg(bevy_linked)]
use bevy_ecs::component::Component;

pub const ERROR_PAGE_URL: &str = "vmux://error/";

/// What went wrong.
///
/// `title` stays the host's own wording rather than a translated string, because the host has no
/// locale and the page does — [`ErrorPageData::heading_message_id`] is where the two wordings the
/// host raises become messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(bevy_linked, derive(Component))]
pub struct ErrorPageData {
    pub title: String,
    pub message: String,
    pub url: String,
}

/// The two failures the host raises itself, which the page shows in the reader's language.
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

    /// The message id for the heading, or `None` when the title is not one the host raises and so
    /// has no translation to look up.
    pub fn heading_message_id(&self) -> Option<&'static str> {
        match self.title.as_str() {
            FAILED_TO_LOAD => Some("error-page-failed-load"),
            NOT_FOUND => Some("error-page-not-found"),
            _ => None,
        }
    }
}
