use bevy::ecs::system::SystemParam;
use bevy::prelude::{Entity, NonSend, NonSendMut, Query, With};
use bevy_cef::prelude::Browsers;

use super::edit::document::Revision;
use super::editing::ClipboardHandle;
use super::editor::FileView;

#[derive(SystemParam)]
pub(super) struct Surface<'w, 's> {
    browsers: Option<NonSend<'w, Browsers>>,
    files: Query<'w, 's, Entity, With<FileView>>,
}

impl Surface<'_, '_> {
    pub(super) fn available(&self) -> bool {
        self.browsers.is_some()
    }

    pub(super) fn can_emit_to(&self, entity: &Entity) -> bool {
        if !self.files.contains(*entity) {
            return false;
        }
        let Some(browsers) = self.browsers.as_deref() else {
            return false;
        };
        browsers.can_emit_to(entity)
    }
}

#[derive(SystemParam)]
pub(super) struct Clipboard<'w, 's> {
    handle: Option<NonSendMut<'w, ClipboardHandle>>,
    documents: Query<'w, 's, Entity, With<Revision>>,
}

impl Clipboard<'_, '_> {
    pub(super) fn read(&mut self, entity: Entity) -> Option<String> {
        if !self.documents.contains(entity) {
            return None;
        }
        let handle = self.handle.as_mut()?.0.as_mut()?;
        handle.get_text().ok()
    }

    pub(super) fn write(&mut self, entity: Entity, text: String) -> bool {
        if !self.documents.contains(entity) {
            return false;
        }
        let Some(handle) = self.handle.as_mut() else {
            return false;
        };
        let Some(clipboard) = handle.0.as_mut() else {
            return false;
        };
        clipboard.set_text(text).is_ok()
    }
}
