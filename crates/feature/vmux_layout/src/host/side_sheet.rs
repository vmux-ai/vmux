use super::Open;
#[cfg(test)]
use crate::settings::LayoutSettings;
use bevy::prelude::*;
use vmux_ecs::store::StoreAppExt;
use vmux_flex::prelude::*;

impl Plugin for SideSheetLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.register_stored::<SideSheetSectionsExpanded>()
            .register_type::<SideSheetPaneExpanded>()
            .add_systems(PostUpdate, sync_visibility.before(LayoutSystems::Layout));
        #[cfg(target_os = "macos")]
        app.add_plugins(super::window_buttons::WindowButtonsPlugin);
    }
}

pub(crate) struct SideSheetLayoutPlugin;

#[derive(Component)]
pub struct SideSheet;

#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::side_sheet"]
#[require(moonshine_save::prelude::Save)]
pub struct SideSheetSectionsExpanded {
    pub projects: bool,
    pub bookmarks: bool,
    pub knowledge: bool,
    pub tools: bool,
}

impl Default for SideSheetSectionsExpanded {
    fn default() -> Self {
        Self {
            projects: false,
            bookmarks: true,
            knowledge: false,
            tools: false,
        }
    }
}

impl SideSheetSectionsExpanded {
    pub fn set(&mut self, section: &str, expanded: bool) -> bool {
        let value = match section {
            "projects" => &mut self.projects,
            "bookmarks" => &mut self.bookmarks,
            "knowledge" => &mut self.knowledge,
            "tools" => &mut self.tools,
            _ => return false,
        };
        *value = expanded;
        true
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct SideSheetSections<'w, 's> {
    hierarchy: super::space::SpaceHierarchy<'w, 's>,
    expanded: Query<'w, 's, &'static SideSheetSectionsExpanded, With<super::space::Space>>,
}

impl SideSheetSections<'_, '_> {
    pub fn space_of(&self, entity: Entity) -> Option<Entity> {
        self.hierarchy.get(entity)
    }

    pub fn under(&self, entity: Entity) -> SideSheetSectionsExpanded {
        let Some(space) = self.space_of(entity) else {
            return SideSheetSectionsExpanded::default();
        };

        self.expanded.get(space).copied().unwrap_or_default()
    }
}

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
#[type_path = "vmux_desktop::layout::side_sheet"]
#[require(moonshine_save::prelude::Save)]
pub struct SideSheetPaneExpanded;

#[derive(Component, PartialEq, Eq)]
pub enum SideSheetPosition {
    Left,
    Right,
    Bottom,
}

fn sync_visibility(
    mut side_sheet_q: Query<
        (Entity, &SideSheetPosition, &mut Visibility, &mut Node),
        With<SideSheet>,
    >,
    added: Query<Entity, (With<SideSheet>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    for entity in &added {
        if let Ok((_, pos, mut visibility, mut node)) = side_sheet_q.get_mut(entity)
            && *pos == SideSheetPosition::Left
        {
            *visibility = Visibility::Visible;
            node.display = Display::Flex;
        }
    }
    for entity in removed.read() {
        if let Ok((_, pos, mut visibility, mut node)) = side_sheet_q.get_mut(entity)
            && *pos == SideSheetPosition::Left
        {
            *visibility = Visibility::Hidden;
            node.display = Display::None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_changes_only_for_the_side_sheet_whose_open_state_changed() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(LayoutSettings::default())
            .add_systems(Update, sync_visibility);
        let first = app
            .world_mut()
            .spawn((
                SideSheet,
                SideSheetPosition::Left,
                Open,
                Visibility::Hidden,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();
        let second = app
            .world_mut()
            .spawn((
                SideSheet,
                SideSheetPosition::Left,
                Visibility::Hidden,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().get::<Visibility>(first),
            Some(&Visibility::Visible)
        );
        assert_eq!(
            app.world().get::<Visibility>(second),
            Some(&Visibility::Hidden)
        );

        app.world_mut().entity_mut(first).remove::<Open>();
        app.world_mut().entity_mut(second).insert(Open);
        app.update();

        assert_eq!(
            app.world().get::<Visibility>(first),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(second),
            Some(&Visibility::Visible)
        );
    }
}
