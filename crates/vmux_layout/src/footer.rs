use crate::Open;
use crate::event::{FOOTER_HEIGHT_PX, FooterStateRequest};
use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevy_cef::prelude::{BinEventEmitterPlugin, BinReceive};

#[derive(Component)]
pub struct Footer;

pub(crate) struct FooterLayoutPlugin;

impl Plugin for FooterLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BinEventEmitterPlugin::<(FooterStateRequest,)>::for_hosts(&[
            "layout",
        ]))
        .add_observer(on_footer_state_emit)
        .add_systems(PostUpdate, sync_footer_visibility.before(UiSystems::Layout));
    }
}

fn on_footer_state_emit(
    trigger: On<BinReceive<FooterStateRequest>>,
    footer_q: Query<Entity, With<Footer>>,
    mut commands: Commands,
) {
    let open = trigger.event().payload.open;
    for entity in &footer_q {
        if open {
            commands.entity(entity).insert(Open);
        } else {
            commands.entity(entity).remove::<Open>();
        }
    }
}

fn sync_footer_visibility(
    mut footer_q: Query<(&mut Visibility, &mut Node), With<Footer>>,
    added: Query<Entity, (With<Footer>, Added<Open>)>,
    mut removed: RemovedComponents<Open>,
) {
    for entity in &added {
        if let Ok((mut vis, mut node)) = footer_q.get_mut(entity) {
            *vis = Visibility::Inherited;
            node.display = Display::Flex;
            node.height = Val::Px(FOOTER_HEIGHT_PX);
        }
    }

    for entity in removed.read() {
        if let Ok((mut vis, mut node)) = footer_q.get_mut(entity) {
            *vis = Visibility::Hidden;
            node.display = Display::None;
            node.height = Val::Px(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_reserves_then_collapses_on_open_toggle() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(PostUpdate, sync_footer_visibility);
        let footer = app
            .world_mut()
            .spawn((
                Footer,
                Visibility::Hidden,
                Node {
                    height: Val::Px(0.0),
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        app.world_mut().entity_mut(footer).insert(Open);
        app.update();
        assert_eq!(
            app.world().get::<Node>(footer).unwrap().height,
            Val::Px(FOOTER_HEIGHT_PX)
        );

        app.world_mut().entity_mut(footer).remove::<Open>();
        app.update();
        assert_eq!(app.world().get::<Node>(footer).unwrap().height, Val::Px(0.0));
    }

    #[test]
    fn footer_state_request_toggles_open() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, FooterLayoutPlugin));
        let footer = app
            .world_mut()
            .spawn((
                Footer,
                Visibility::Hidden,
                Node {
                    height: Val::Px(0.0),
                    display: Display::None,
                    ..default()
                },
            ))
            .id();

        app.world_mut().trigger(BinReceive::<FooterStateRequest> {
            webview: Entity::PLACEHOLDER,
            payload: FooterStateRequest { open: true },
        });
        app.world_mut().flush();
        assert!(app.world().entity(footer).contains::<Open>());

        app.world_mut().trigger(BinReceive::<FooterStateRequest> {
            webview: Entity::PLACEHOLDER,
            payload: FooterStateRequest { open: false },
        });
        app.world_mut().flush();
        assert!(!app.world().entity(footer).contains::<Open>());
    }
}
