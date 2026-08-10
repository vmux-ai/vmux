//! What a keystroke a page handed over turns out to mean.
//!
//! The other half of the claim seam. [`crate::shortcut::KeymapView::claims`] tells a page which
//! strokes to give up; this is where one of them comes back and gets resolved, against the same
//! keymap and the same published context, so a page never learns what any key does.
//!
//! Only context-scoped bindings are answered here — see [`crate::shortcut::KeymapView::scoped`] for
//! why. The stroke arrives stamped with the webview that sent it, and that entity rides on as
//! [`CommandIssued::caller`], which is the only way a command can act back on the surface that
//! asked for it.

use bevy::prelude::*;
use bevy_cef::prelude::BinReceive;
use vmux_core::input::KeyStroke;

use crate::issued::CommandIssuer;
use crate::shortcut::{KeyCombo, KeyContext, Keymap};

/// Resolves keystrokes pages hand over into commands.
///
/// Added once, wherever the keymap lives. A surface that wants a keyboard needs no plugin of its
/// own: it publishes a context, and a binding scoped to that context starts working.
pub struct PageKeyPlugin;

impl Plugin for PageKeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(resolve_page_key);
    }
}

fn resolve_page_key(
    trigger: On<BinReceive<KeyStroke>>,
    keymap: Option<Res<Keymap>>,
    contexts: Query<&KeyContext>,
    mut issuer: CommandIssuer,
) {
    let Some(keymap) = keymap else {
        return;
    };
    let page = trigger.event_target();
    let Ok(context) = contexts.get(page) else {
        return;
    };
    let Some(pressed) = KeyCombo::of(&trigger.payload) else {
        return;
    };
    let Some(command) = keymap.in_context(context).scoped(&pressed) else {
        return;
    };
    issuer.issue(page, command);
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
        /// An app holding one binding scoped to `command-bar` and one that applies everywhere, both
        /// on keys a page could hand over.
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

        /// The commands the page's stroke produced, with the caller each was stamped with.
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

    /// The point of the seam: a binding scoped to a surface fires for the page that published it
    /// and for no other, and the command names the page so it can be handed back.
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

    /// An unconditional binding has already been answered by the native keyboard by the time the
    /// page's copy of the stroke arrives. Answering it again here would run the command twice.
    #[test]
    fn an_unconditional_binding_is_not_answered_a_second_time() {
        let mut app = Seam::app();
        let bar = Seam::page(&mut app, &["command-bar"]);

        assert_eq!(Seam::press(&mut app, bar, "KeyX"), vec![]);
    }
}
