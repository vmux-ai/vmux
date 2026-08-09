use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeWindow {
    pub id: i32,
    pub focused: bool,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub incognito: bool,
    #[serde(rename = "type")]
    pub window_type: String,
    pub state: String,
    pub always_on_top: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeTab {
    pub id: i32,
    pub window_id: i32,
    pub index: u32,
    pub active: bool,
    pub highlighted: bool,
    pub pinned: bool,
    pub url: String,
    pub title: String,
    pub status: String,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ChromeModel {
    pub windows: Vec<ChromeWindow>,
    pub tabs: Vec<ChromeTab>,
}

#[derive(Resource)]
pub struct ChromeStableIds {
    next_window: i32,
    next_tab: i32,
    windows: HashMap<Entity, i32>,
    tabs: HashMap<Entity, i32>,
}

impl Default for ChromeStableIds {
    fn default() -> Self {
        Self {
            next_window: 1,
            next_tab: 1,
            windows: HashMap::new(),
            tabs: HashMap::new(),
        }
    }
}

impl ChromeStableIds {
    pub(crate) fn window(&mut self, entity: Entity) -> i32 {
        if let Some(id) = self.windows.get(&entity) {
            return *id;
        }
        let id = self.next_window;
        self.next_window += 1;
        self.windows.insert(entity, id);
        id
    }

    pub(crate) fn tab(&mut self, entity: Entity) -> i32 {
        if let Some(id) = self.tabs.get(&entity) {
            return *id;
        }
        let id = self.next_tab;
        self.next_tab += 1;
        self.tabs.insert(entity, id);
        id
    }

    pub(crate) fn tab_entity(&self, id: i32) -> Option<Entity> {
        self.tabs
            .iter()
            .find_map(|(entity, stable_id)| (*stable_id == id).then_some(*entity))
    }

    pub(crate) fn window_entity(&self, id: i32) -> Option<Entity> {
        self.windows
            .iter()
            .find_map(|(entity, stable_id)| (*stable_id == id).then_some(*entity))
    }
}

#[derive(Message, Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum ChromeModelEvent {
    WindowCreated(ChromeWindow),
    WindowRemoved { window_id: i32 },
    WindowFocusChanged { window_id: i32 },
    WindowBoundsChanged(ChromeWindow),
    TabCreated(ChromeTab),
    TabUpdated { old: ChromeTab, new: ChromeTab },
    TabRemoved { tab_id: i32, window_id: i32 },
    TabActivated { tab_id: i32, window_id: i32 },
}

pub fn extension_visible_url(url: &str) -> bool {
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("chrome-extension://")
}

#[cfg(test)]
#[path = "model.test.rs"]
mod tests;
