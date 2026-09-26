#[vmux_api::ui_event(Default, Eq, url = "git://")]
pub struct PageContextRequest {}

#[cfg(host)]
#[derive(bevy::prelude::Message, Clone, Debug, PartialEq, Eq)]
pub struct RecordVisitRequest {
    pub url: String,
    pub title: String,
}
