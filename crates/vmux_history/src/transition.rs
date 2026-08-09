use bevy_cef_core::prelude::{CefTransitionCore, CefTransitionQualifiers};
use vmux_core::TransitionType;

pub fn map(core: CefTransitionCore, qual: CefTransitionQualifiers) -> TransitionType {
    if qual.forward_back {
        return TransitionType::BackForward;
    }
    if qual.client_redirect || qual.server_redirect {
        return TransitionType::Redirect;
    }
    match core {
        CefTransitionCore::Reload => TransitionType::Reload,
        CefTransitionCore::Explicit
        | CefTransitionCore::Generated
        | CefTransitionCore::Keyword
        | CefTransitionCore::KeywordGenerated => TransitionType::Typed,
        CefTransitionCore::Link
        | CefTransitionCore::FormSubmit
        | CefTransitionCore::AutoBookmark => TransitionType::Link,
        _ => TransitionType::Other,
    }
}

#[cfg(test)]
#[path = "transition.test.rs"]
mod tests;
