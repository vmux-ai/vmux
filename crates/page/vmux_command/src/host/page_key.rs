use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_core::input::KeyStroke;

use crate::definition::CommandInvocation;
use crate::shortcut::{KeyCombo, KeyContext, Keymap};

pub struct KeyPlugin;

impl Plugin for KeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(resolve_page_key);
    }
}

fn resolve_page_key(
    trigger: On<UiInput<KeyStroke>>,
    keymap: Option<Res<Keymap>>,
    contexts: Query<&KeyContext>,
    mut invocations: MessageWriter<CommandInvocation>,
) {
    let page = trigger.event_target();
    let (Some(keymap), Ok(context), Some(pressed)) = (
        keymap.as_deref(),
        contexts.get(page),
        KeyCombo::from_stroke(&trigger.payload),
    ) else {
        return;
    };
    let Some(command) = keymap.in_context(context).scoped(&pressed) else {
        return;
    };
    invocations.write(CommandInvocation::new(page, command));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::CommandInvocation;
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
            keymap.register(["command_bar_next", "stack_close"]);

            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(KeyPlugin)
                .add_message::<CommandInvocation>()
                .insert_resource(keymap);
            app
        }

        fn page(app: &mut App, context: &[&str]) -> Entity {
            let context: KeyContext = context.iter().map(|key| (*key).to_string()).collect();
            app.world_mut().spawn(context).id()
        }

        fn press(app: &mut App, page: Entity, code: &str) -> Vec<(Entity, String)> {
            app.world_mut().trigger(UiInput {
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
                .resource_mut::<Messages<CommandInvocation>>()
                .drain()
                .map(|invocation| (invocation.caller, invocation.id))
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
            vec![(bar, "command_bar_next".to_string())]
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
            trigger: On<UiInput<KeyStroke>>,
            keymap: Res<Keymap>,
            contexts: Query<&KeyContext>,
            mut answered: ResMut<Self>,
        ) {
            let is_answered = contexts
                .get(trigger.event_target())
                .ok()
                .and_then(|context| {
                    KeyCombo::from_stroke(&trigger.payload).map(|key| (context, key))
                })
                .and_then(|(context, key)| keymap.in_context(context).scoped(&key))
                .is_some();
            answered.0.push(is_answered);
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
