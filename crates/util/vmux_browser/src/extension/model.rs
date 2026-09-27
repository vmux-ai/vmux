use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionWindowSnapshot {
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
pub struct ExtensionTabSnapshot {
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

impl ExtensionTabSnapshot {
    pub(crate) fn disclosed_value(
        mut self,
        window_id: i32,
        index: u32,
        request: &vmux_extension::protocol::ApiRequest,
        authorization: &super::bridge::BridgeAuthorization,
    ) -> serde_json::Value {
        self.window_id = window_id;
        self.index = index;
        let disclose = authorization.permissions.contains("tabs")
            || url::Url::parse(&self.url).is_ok_and(|url| {
                (url.scheme() == "chrome-extension"
                    && url.host_str() == Some(request.caller_context.extension_id()))
                    || authorization
                        .host_permissions
                        .iter()
                        .any(|pattern| pattern.matches(&url))
            });
        let mut value = serde_json::to_value(self).expect("extension tab serializes");
        if !disclose {
            let object = value.as_object_mut().expect("tab object");
            object.remove("url");
            object.remove("title");
        }
        value
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ExtensionModel {
    pub windows: Vec<ExtensionWindowSnapshot>,
    pub tabs: Vec<ExtensionTabSnapshot>,
}

#[derive(Component)]
pub struct ExtensionStableIds {
    next_window: i32,
    next_tab: i32,
    windows: HashMap<Entity, i32>,
    tabs: HashMap<Entity, i32>,
}

impl Default for ExtensionStableIds {
    fn default() -> Self {
        Self {
            next_window: 1,
            next_tab: 1,
            windows: HashMap::new(),
            tabs: HashMap::new(),
        }
    }
}

impl ExtensionStableIds {
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
pub enum ExtensionModelEvent {
    WindowCreated(ExtensionWindowSnapshot),
    WindowRemoved {
        window_id: i32,
    },
    WindowFocusChanged {
        window_id: i32,
    },
    WindowBoundsChanged(ExtensionWindowSnapshot),
    TabCreated(ExtensionTabSnapshot),
    TabUpdated {
        old: ExtensionTabSnapshot,
        new: ExtensionTabSnapshot,
    },
    TabRemoved {
        tab_id: i32,
        window_id: i32,
    },
    TabActivated {
        tab_id: i32,
        window_id: i32,
    },
}

pub fn extension_visible_url(url: &str) -> bool {
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("chrome-extension://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use bevy::window::{PrimaryWindow, WindowPosition};
    use vmux_core::{Order, PageMetadata};
    use vmux_history::LastActivatedAt;
    use vmux_layout::pane::Pane;
    use vmux_layout::space::Space;
    use vmux_layout::stack::{FocusedStack, Stack};
    use vmux_layout::tab::Tab;

    fn spawn_page(
        world: &mut World,
        pane: Entity,
        url: &str,
        title: &str,
        activated: i64,
    ) -> Entity {
        world
            .spawn((
                Stack::default(),
                PageMetadata {
                    title: title.into(),
                    url: url.into(),
                    ..default()
                },
                LastActivatedAt(activated),
                ChildOf(pane),
            ))
            .id()
    }

    #[test]
    fn projects_ordered_visible_pages_with_stable_ids_and_removals() {
        let mut app = App::new();
        app.insert_resource(FocusedStack::default())
            .add_plugins(crate::extension::project::ExtensionProjectPlugin);
        app.world_mut().spawn((
            Window {
                resolution: (1200, 800).into(),
                position: WindowPosition::At(IVec2::new(40, 60)),
                focused: true,
                ..default()
            },
            PrimaryWindow,
        ));
        let space_two = app.world_mut().spawn((Space, Order(2))).id();
        let space_one = app.world_mut().spawn((Space, Order(1))).id();
        let tab_one = app
            .world_mut()
            .spawn((Tab::default(), Order(1), ChildOf(space_one)))
            .id();
        let pane_one = app.world_mut().spawn((Pane, ChildOf(tab_one))).id();
        let first = spawn_page(app.world_mut(), pane_one, "https://one.example/", "One", 1);
        let terminal = spawn_page(app.world_mut(), pane_one, "vmux://terminal/", "Terminal", 2);
        let tab_two = app
            .world_mut()
            .spawn((Tab::default(), Order(1), ChildOf(space_two)))
            .id();
        let pane_two = app.world_mut().spawn((Pane, ChildOf(tab_two))).id();
        let second = spawn_page(app.world_mut(), pane_two, "https://two.example/", "Two", 5);
        spawn_page(
            app.world_mut(),
            pane_two,
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html",
            "Extension",
            4,
        );
        spawn_page(
            app.world_mut(),
            pane_two,
            "file:///Users/x/notes.md",
            "Internal",
            3,
        );
        app.world_mut().resource_mut::<FocusedStack>().stack = Some(second);

        app.update();

        let runtime = app
            .world()
            .iter_entities()
            .find(|entity| entity.contains::<ExtensionModel>())
            .unwrap()
            .id();
        let model = app.world().get::<ExtensionModel>(runtime).unwrap();
        assert_eq!(model.windows.len(), 1);
        assert_eq!(model.tabs.len(), 3);
        assert_eq!(
            model
                .tabs
                .iter()
                .map(|tab| tab.url.as_str())
                .collect::<Vec<_>>(),
            vec![
                "https://one.example/",
                "https://two.example/",
                "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html",
            ]
        );
        assert_eq!(model.tabs.iter().filter(|tab| tab.active).count(), 1);
        assert!(!model.tabs.iter().any(|tab| tab.url.starts_with("vmux://")));
        assert!(!model.tabs.iter().any(|tab| tab.url.starts_with("cef://")));
        let first_id = model.tabs[0].id;
        let active_id = model.tabs.iter().find(|tab| tab.active).unwrap().id;

        app.world_mut().resource_mut::<FocusedStack>().stack = Some(terminal);
        app.update();
        assert_eq!(
            app.world()
                .get::<ExtensionModel>(runtime)
                .unwrap()
                .tabs
                .iter()
                .find(|tab| tab.active)
                .unwrap()
                .id,
            active_id
        );

        app.world_mut()
            .get_mut::<PageMetadata>(first)
            .unwrap()
            .title = "Updated".into();
        app.update();
        assert_eq!(
            app.world().get::<ExtensionModel>(runtime).unwrap().tabs[0].id,
            first_id
        );

        let mut cursor = app
            .world()
            .resource::<Messages<ExtensionModelEvent>>()
            .get_cursor();
        app.world_mut().entity_mut(first).despawn();
        app.update();
        let messages = app.world().resource::<Messages<ExtensionModelEvent>>();
        assert!(cursor.read(messages).any(|event| matches!(
            event,
            ExtensionModelEvent::TabRemoved { tab_id, .. } if *tab_id == first_id
        )));
    }

    #[test]
    fn filters_extension_visible_urls() {
        assert!(extension_visible_url("https://example.com/"));
        assert!(extension_visible_url("http://example.com/"));
        assert!(extension_visible_url(
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/popup.html"
        ));
        assert!(!extension_visible_url("vmux://terminal/"));
        assert!(!extension_visible_url("file:///Users/x/notes.md"));
    }
}
