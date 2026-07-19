use super::Open;
use crate::event::CEF_RESERVED_HEIGHT_PX;
use bevy::prelude::*;

#[derive(Component)]
pub struct Header;

pub(crate) struct HeaderLayoutPlugin;

impl Plugin for HeaderLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            sync_header_visibility
                .before(crate::toggle::LayoutChromeTransitionSet)
                .before(bevy::ui::UiSystems::Layout),
        );
    }
}

fn sync_header_visibility(
    mut header_q: Query<(&mut Visibility, &mut Node), With<Header>>,
    added: Query<Entity, (With<Header>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    for entity in &added {
        if let Ok((mut vis, mut node)) = header_q.get_mut(entity) {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
            node.height = Val::Px(CEF_RESERVED_HEIGHT_PX);
        }
    }

    for entity in removed.read() {
        if let Ok((mut vis, mut node)) = header_q.get_mut(entity) {
            *vis = Visibility::Hidden;
            node.display = Display::None;
            node.height = Val::Px(0.0);
        }
    }
}
