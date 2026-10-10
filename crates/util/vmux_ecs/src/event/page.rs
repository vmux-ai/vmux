#[cfg(host)]
#[derive(bevy_ecs::prelude::Message, Clone, Debug, PartialEq, Eq)]
pub struct RecordVisitRequest {
    pub url: String,
    pub title: String,
}
