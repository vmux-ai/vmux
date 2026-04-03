### Entity

```rs
use bevy::prelude::{Bundle, Component, Name};
use chrono::{DateTime, Utc};
use url::Url;

// `Name` is Bevy’s built-in component: human-readable entity label for debugging / tools.
// Bevy’s own `Window` (winit surface) is not imported here so a layout `Window` marker can exist.

// --- COMPONENTS (layout) ---

#[derive(Component, Default, Debug, Clone, Copy)]
pub struct Space;

#[derive(Component, Default, Debug, Clone, Copy)]
pub struct Window;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Horizontal,
    Vertical,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Weight(pub f32);

// --- COMPONENTS (content) ---

#[derive(Component, Debug, Clone, Copy)]
pub struct Tab;

#[derive(Component, Debug, Clone, Copy)]
pub struct Browser;

#[derive(Component, Debug, Clone, Copy)]
pub struct Visit;

#[derive(Component, Debug, Clone)]
pub struct PageMetadata {
    pub url: Url,
    pub title: String,
    pub favicon_url: Option<String>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct CreatedAt(pub DateTime<Utc>);

// --- LAYOUT BUNDLES ---

#[derive(Bundle, Default)]
pub struct SpaceBundle {
    pub space: Space,
    pub name: Name,
}

#[derive(Bundle, Default)]
pub struct WindowBundle {
    pub window: Window,
    pub name: Name,
}

#[derive(Bundle)]
pub struct PaneBundle {
    pub pane: Pane,
    pub weight: Weight,
    pub name: Name,
}

impl Default for PaneBundle {
    fn default() -> Self {
        Self {
            pane: Pane::Horizontal,
            weight: Weight(1.0),
            name: Name::new("Pane"),
        }
    }
}

// --- CONTENT BUNDLES ---

// Tabs are not a single bundle type: each tab *kind* has its own bundle, all tagged with `Tab`
// (and typically `Weight`, `Name`, `CreatedAt`). Browser tabs carry `Browser` + `PageMetadata`;
// other kinds swap or omit those for their own content components.

#[derive(Bundle)]
pub struct BrowserTabBundle {
    pub tab: Tab,
    pub browser: Browser,
    pub metadata: PageMetadata,
    pub weight: Weight,
    pub name: Name,
    pub created_at: CreatedAt,
}

#[derive(Bundle)]
pub struct SettingsTabBundle {
    pub tab: Tab,
    pub weight: Weight,
    pub name: Name,
    pub created_at: CreatedAt,
}

#[derive(Bundle)]
pub struct VisitBundle {
    pub visit: Visit,
    pub metadata: PageMetadata,
    pub created_at: CreatedAt,
}
```

Your Pane entities act as the "Nodes," and your Tab entities act as the "Leaves."

- Space Entity
    - Children: [WindowEntity]
- Window Entity
    - Parent: WorkspaceEntity
    - Children: [RootPaneEntity]
- Pane Entity (Horizontal)
    - Parent: WindowEntity
    - Children: [SidebarPane, ContentPane]
- Tab Entity
    - Parent: ContentPane
    - Components: Tab, Browser, Active, Weight
```

### Plugin

- WebviewPlugin
- HistoryPlugin
