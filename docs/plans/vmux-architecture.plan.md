## vmux architecture plan

### Entity model (conceptual ECS)

```rs
use bevy::prelude::{Bundle, Component, Name};
use chrono::{DateTime, Utc};
use url::Url;

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

#[derive(Component, Debug, Clone, Copy)]
pub struct Active;

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

Use hierachy like this for Pane state


```rs
pub struct NewWindowEvent {
    pub workspace_entity: Entity,
    pub window_name: String,
    pub initial_url: String,
}

fn handle_new_window(
    mut commands: Commands,
    mut events: EventReader<NewWindowEvent>,
) {
    for event in events.read() {
        // 1. Spawn the Window as a child of the Workspace
        let window = commands.spawn(WindowBundle {
            name: Name::new(event.window_name.clone()),
            ..default()
        }).id();

        // Establish the Workspace -> Window link
        commands.entity(event.workspace_entity).add_child(window);

        // 2. Build the default internal hierarchy for the new window
        commands.entity(window).with_children(|parent| {
            // Every window starts with at least one Root Pane
            parent.spawn(PaneBundle {
                pane: Pane::Horizontal, // Default split direction
                weight: Weight(1.0),
                name: Name::new("Root Pane"),
                ..default()
            })
            .with_children(|root_pane| {
                // 3. Spawn the initial "Home" or "New Tab" page
                root_pane.spawn((
                    TabBundle {
                        name: Name::new("New Tab"),
                        metadata: PageMetadata {
                            url: event.initial_url.clone(),
                            ..default()
                        },
                        weight: Weight(1.0),
                        ..default()
                    },
                    Active, // Make it the visible tab immediately
                ));
            });
        });

        info!("VMUX: New OS Window logic initialized for {}", event.window_name);
    }
}
```
