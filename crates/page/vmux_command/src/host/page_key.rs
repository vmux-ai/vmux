use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::BinReceive;
use vmux_core::input::KeyStroke;

use crate::command::AppCommand;
use crate::issued::CommandIssuer;
use crate::shortcut::{KeyCombo, KeyContext, Keymap};

pub struct PageKeyPlugin;

impl Plugin for PageKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(resolve_page_key);
    }
}

fn resolve_page_key(
    trigger: On<BinReceive<KeyStroke>>,
    keys: ScopedKeys,
    mut issuer: CommandIssuer,
) {
    let page = trigger.event_target();
    let Some(command) = keys.command(page, &trigger.payload) else {
        return;
    };
    issuer.issue(page, command);
}

#[derive(SystemParam)]
pub struct ScopedKeys<'w, 's> {
    keymap: Option<Res<'w, Keymap>>,
    contexts: Query<'w, 's, &'static KeyContext>,
}

impl ScopedKeys<'_, '_> {
    pub fn command(&self, page: Entity, stroke: &KeyStroke) -> Option<AppCommand> {
        let keymap = self.keymap.as_ref()?;
        let context = self.contexts.get(page).ok()?;
        let pressed = KeyCombo::of(stroke)?;
        keymap.in_context(context).scoped(&pressed)
    }

    pub fn answered(&self, page: Entity, stroke: &KeyStroke) -> bool {
        self.command(page, stroke).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::AppCommand;
    use crate::issued::CommandIssued;
    use crate::shortcut::{Binding, Modifiers, Shortcut, Source, When};
    use bevy::ecs::message::Messages;
    use bevy::input::keyboard::KeyCode;
    use vmux_core::input::KeyModifiers;

    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        shift: false,
        alt: false,
        super_key: false,
    };

    struct Seam;

    impl Seam {
        fn app() -> App {
            let mut keymap = Keymap::default();
            keymap.extend(
                Source::Settings,
                [
                    Binding {
                        shortcut: Shortcut::Direct(KeyCombo {
                            key: KeyCode::KeyN,
                            modifiers: CTRL,
                        }),
                        command: "command_bar_next".to_string(),
                        when: When::parse("command-bar"),
                    },
                    Binding {
                        shortcut: Shortcut::Direct(KeyCombo {
                            key: KeyCode::KeyX,
                            modifiers: CTRL,
                        }),
                        command: "stack_close".to_string(),
                        when: None,
                    },
                ],
            );

            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(PageKeyPlugin)
                .add_message::<AppCommand>()
                .add_message::<CommandIssued>()
                .insert_resource(keymap);
            app
        }

        fn page(app: &mut App, context: &[&str]) -> Entity {
            let context: KeyContext = context.iter().map(|key| (*key).to_string()).collect();
            app.world_mut().spawn(context).id()
        }

        fn press(app: &mut App, page: Entity, code: &str) -> Vec<(Entity, AppCommand)> {
            app.world_mut().trigger(BinReceive {
                webview: page,
                payload: KeyStroke {
                    key: code.to_string(),
                    code: code.to_string(),
                    mods: KeyModifiers::from(CTRL),
                    text: None,
                    repeat: false,
                },
            });
            app.update();
            app.world_mut()
                .resource_mut::<Messages<CommandIssued>>()
                .drain()
                .map(|issued| (issued.caller, issued.command))
                .collect()
        }
    }

    #[test]
    fn a_scoped_binding_resolves_only_on_the_surface_that_published_it() {
        let mut app = Seam::app();
        let bar = Seam::page(&mut app, &["command-bar"]);
        let plain = Seam::page(&mut app, &["terminal"]);

        assert_eq!(
            Seam::press(&mut app, bar, "KeyN"),
            vec![(
                bar,
                AppCommand::from_shortcut_id("command_bar_next").expect("command exists")
            )]
        );
        assert_eq!(Seam::press(&mut app, plain, "KeyN"), vec![]);
    }

    #[test]
    fn an_unconditional_binding_is_not_answered_a_second_time() {
        let mut app = Seam::app();
        let bar = Seam::page(&mut app, &["command-bar"]);

        assert_eq!(Seam::press(&mut app, bar, "KeyX"), vec![]);
    }

    #[derive(Resource, Default)]
    struct Answered(Vec<bool>);

    impl Answered {
        fn record(
            trigger: On<BinReceive<KeyStroke>>,
            keys: ScopedKeys,
            mut answered: ResMut<Self>,
        ) {
            answered
                .0
                .push(keys.answered(trigger.event_target(), &trigger.payload));
        }
    }

    #[test]
    fn only_a_scoped_binding_takes_a_key_from_the_surfaces_own_keymap() {
        let mut app = Seam::app();
        app.init_resource::<Answered>()
            .add_observer(Answered::record);
        let bar = Seam::page(&mut app, &["command-bar"]);
        let plain = Seam::page(&mut app, &["terminal"]);

        Seam::press(&mut app, bar, "KeyN");
        Seam::press(&mut app, plain, "KeyN");
        Seam::press(&mut app, bar, "KeyX");

        assert_eq!(
            app.world().resource::<Answered>().0,
            vec![true, false, false]
        );
    }
}
