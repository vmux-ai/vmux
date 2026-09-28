use bevy_ecs::prelude::*;

#[derive(SystemSet, Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OsMenuSet {
    Forward,
    Dispatch,
    Cleanup,
}

#[derive(Component)]
pub struct OsMenuEntry {
    id: Option<String>,
    label: String,
    enabled: bool,
}

impl OsMenuEntry {
    pub fn new(label: String, enabled: bool) -> Self {
        Self {
            id: None,
            label,
            enabled,
        }
    }

    pub fn identified(id: String) -> Self {
        Self {
            id: Some(id),
            label: String::new(),
            enabled: true,
        }
    }

    pub fn matches(&self, event_id: &str) -> bool {
        self.id.as_deref() == Some(event_id)
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn mark_presented(&mut self, id: String) {
        self.id = Some(id);
    }
}

#[derive(Component)]
pub struct OsContextMenu {
    view: usize,
}

impl OsContextMenu {
    pub fn new(view: *mut std::ffi::c_void) -> Self {
        Self {
            view: view as usize,
        }
    }

    pub fn view(&self) -> *mut std::ffi::c_void {
        self.view as _
    }
}

#[derive(Component)]
pub struct OsMenuSeparator;

#[derive(Component)]
pub struct TransientOsMenuEntry;

#[derive(Message, Clone, Copy)]
pub struct OsMenuSelection(Entity);

impl OsMenuSelection {
    pub fn new(entity: Entity) -> Self {
        Self(entity)
    }

    pub fn target(&self) -> Entity {
        self.0
    }
}
