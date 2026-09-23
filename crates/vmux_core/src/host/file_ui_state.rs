use bevy::prelude::*;

use crate::event::FileUiStatePatch;

#[derive(Clone, EntityEvent)]
pub struct FileUiStateWrite {
    #[event_target]
    pub webview: Entity,
    pub patch: FileUiStatePatch,
}

impl FileUiStateWrite {
    pub fn from_event<T>(webview: Entity, event: &T) -> Self
    where
        T: Clone + Into<FileUiStatePatch>,
    {
        Self {
            webview,
            patch: event.clone().into(),
        }
    }
}
