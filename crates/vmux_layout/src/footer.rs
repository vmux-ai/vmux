use super::Open;
use crate::event::FOOTER_HEIGHT_PX;
use bevy::prelude::*;
use vmux_command::{AppCommand, BrowserCommand, FooterCommand, ReadAppCommands};

#[derive(Component)]
pub struct Footer;

pub(crate) struct FooterLayoutPlugin;

impl Plugin for FooterLayoutPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_footer_toggle.in_set(ReadAppCommands))
            .add_systems(
                PostUpdate,
                sync_footer_visibility.before(bevy::ui::UiSystems::Layout),
            );
    }
}

fn handle_footer_toggle(
    mut reader: MessageReader<AppCommand>,
    footer_q: Query<(Entity, Has<Open>), With<Footer>>,
    mut commands: Commands,
) {
    for cmd in reader.read() {
        for (entity, is_open) in &footer_q {
            match footer_open_after_command(*cmd, is_open) {
                Some(true) => {
                    commands.entity(entity).insert(Open);
                }
                Some(false) => {
                    commands.entity(entity).remove::<Open>();
                }
                None => {}
            }
        }
    }
}

fn footer_open_after_command(cmd: AppCommand, is_open: bool) -> Option<bool> {
    match cmd {
        AppCommand::Footer(FooterCommand::Toggle) => Some(!is_open),
        AppCommand::Browser(BrowserCommand::Find) if is_open => Some(false),
        _ => None,
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
    use vmux_command::CommandPlugin;

    #[test]
    fn browser_find_closes_open_footer_on_first_command() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(CommandPlugin);
        app.add_systems(Update, handle_footer_toggle.in_set(ReadAppCommands));

        let footer = app.world_mut().spawn((Footer, Open)).id();
        app.world_mut()
            .resource_mut::<Messages<AppCommand>>()
            .write(AppCommand::Browser(BrowserCommand::Find));

        app.update();

        assert!(!app.world().entity(footer).contains::<Open>());
    }

    #[test]
    fn footer_toggle_still_opens_and_closes() {
        assert_eq!(
            footer_open_after_command(AppCommand::Footer(FooterCommand::Toggle), false),
            Some(true)
        );
        assert_eq!(
            footer_open_after_command(AppCommand::Footer(FooterCommand::Toggle), true),
            Some(false)
        );
    }
}
