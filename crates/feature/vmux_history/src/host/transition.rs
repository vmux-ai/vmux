use bevy_cef_core::prelude::{CefTransitionCore, CefTransitionQualifiers};
use vmux_ecs::TransitionType;

pub struct HistoryTransition {
    core: CefTransitionCore,
    qualifiers: CefTransitionQualifiers,
}

impl HistoryTransition {
    pub fn new(core: CefTransitionCore, qualifiers: CefTransitionQualifiers) -> Self {
        Self { core, qualifiers }
    }

    pub fn kind(&self) -> TransitionType {
        if self.qualifiers.forward_back {
            return TransitionType::BackForward;
        }
        if self.qualifiers.client_redirect || self.qualifiers.server_redirect {
            return TransitionType::Redirect;
        }
        match self.core {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_qual() -> CefTransitionQualifiers {
        CefTransitionQualifiers::default()
    }

    #[test]
    fn forward_back_wins_over_core() {
        let qual = CefTransitionQualifiers {
            forward_back: true,
            ..no_qual()
        };
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::Explicit, qual).kind(),
            TransitionType::BackForward
        );
    }

    #[test]
    fn server_redirect_wins_over_core() {
        let qual = CefTransitionQualifiers {
            server_redirect: true,
            ..no_qual()
        };
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::Link, qual).kind(),
            TransitionType::Redirect
        );
    }

    #[test]
    fn typed_from_explicit() {
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::Explicit, no_qual()).kind(),
            TransitionType::Typed
        );
    }

    #[test]
    fn typed_from_generated() {
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::Generated, no_qual()).kind(),
            TransitionType::Typed
        );
    }

    #[test]
    fn link_from_link_and_form_submit() {
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::Link, no_qual()).kind(),
            TransitionType::Link
        );
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::FormSubmit, no_qual()).kind(),
            TransitionType::Link
        );
    }

    #[test]
    fn reload_maps_directly() {
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::Reload, no_qual()).kind(),
            TransitionType::Reload
        );
    }

    #[test]
    fn subframe_falls_to_other() {
        assert_eq!(
            HistoryTransition::new(CefTransitionCore::AutoSubframe, no_qual()).kind(),
            TransitionType::Other
        );
    }
}
