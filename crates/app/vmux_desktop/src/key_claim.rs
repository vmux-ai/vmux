use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, BinReceive, WebviewSource};
use vmux_command::shortcut::{KeyContext, Keymap};
use vmux_core::host::page::HostsPage;
use vmux_core::input::{KEY_CLAIMS_EVENT, KeyClaims, PageKeyContext};

/// Tells each page which strokes it must hand over, and keeps that answer current as its context
/// changes.
///
/// The keymap lives here and never travels; what travels is the small set of strokes it resolves to
/// for one page's context. That is what lets a page act on a key in the tick it arrives without
/// knowing a single binding.
///
/// Recomputing is deliberately off the per-keystroke path: it happens when a page says its context
/// changed, or when the keymap itself is rebuilt from settings.
pub struct KeyClaimPlugin;

impl Plugin for KeyClaimPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(receive_page_context)
            .add_observer(reclaim_on_page_ready)
            .add_systems(Update, (start_page_context, push_key_claims).chain());
    }
}

/// Gives every page an empty context as soon as it exists.
///
/// Two reasons. A page is claimed for from the start, so one that publishes nothing still gets the
/// bindings that apply everywhere. And it leaves [`receive_page_context`] with nothing to insert —
/// only a value to replace — which keeps the publish path off `Commands` entirely.
fn start_page_context(
    pages: Query<
        Entity,
        (
            Or<(With<WebviewSource>, With<HostsPage>)>,
            Without<KeyContext>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in pages.iter() {
        commands.entity(entity).insert(KeyContext::default());
    }
}

/// Records what a page says is true of it, on that page's own entity.
fn receive_page_context(
    trigger: On<BinReceive<PageKeyContext>>,
    mut contexts: Query<&mut KeyContext>,
) {
    let Ok(mut current) = contexts.get_mut(trigger.event_target()) else {
        return;
    };
    current.set_if_neq(trigger.payload.keys.iter().cloned().collect());
}

/// Pushes a page its claimed set again once it says it is listening.
///
/// Without this, a page that came up after its claims were computed would hear nothing until its
/// context happened to change — and a page that never hears its claims silently has no shortcuts at
/// all, which looks like the keymap being broken rather than a message being early.
fn reclaim_on_page_ready(
    trigger: On<BinReceive<vmux_core::host::page::PageReady>>,
    mut contexts: Query<&mut KeyContext>,
) {
    let Ok(mut context) = contexts.get_mut(trigger.event_target()) else {
        return;
    };
    context.set_changed();
}

/// Pushes each page its claimed set, when the answer for that page has changed.
fn push_key_claims(
    keymap: Option<Res<Keymap>>,
    contexts: Query<(Entity, Ref<KeyContext>), Or<(With<WebviewSource>, With<HostsPage>)>>,
    mut commands: Commands,
) {
    let Some(keymap) = keymap else {
        return;
    };
    for (entity, context) in contexts.iter() {
        if !keymap.is_changed() && !context.is_changed() {
            continue;
        }
        let claims: KeyClaims = keymap.in_context(&context).claims();
        commands.trigger(BinHostEmitEvent::from_rkyv(
            entity,
            KEY_CLAIMS_EVENT,
            &claims,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::KeyCode;
    use vmux_command::shortcut::{Binding, KeyCombo, Modifiers, Shortcut, Source, When};

    /// Records what the seam pushed to each page, which is the only observable this plugin has.
    #[derive(Resource, Default)]
    struct Pushed(Vec<(Entity, KeyClaims)>);

    impl Pushed {
        /// The claimed sets pushed to one page, oldest first, each sorted so the assertion does not
        /// restate the keymap's precedence order.
        fn codes(world: &World, page: Entity) -> Vec<Vec<String>> {
            let mut sets = Vec::new();
            for (entity, claims) in &world.resource::<Self>().0 {
                if *entity != page {
                    continue;
                }
                let mut codes: Vec<String> =
                    claims.keys.iter().map(|key| key.code.clone()).collect();
                codes.sort();
                sets.push(codes);
            }
            sets
        }

        fn record(
            trigger: On<BinHostEmitEvent>,
            mut pushed: ResMut<Self>,
            mut rejected: ResMut<Rejected>,
        ) {
            if trigger.id != KEY_CLAIMS_EVENT {
                rejected.0.push(trigger.id.clone());
                return;
            }
            let Ok(claims) = rkyv::from_bytes::<KeyClaims, rkyv::rancor::Error>(&trigger.payload)
            else {
                rejected.0.push("undecodable payload".to_string());
                return;
            };
            pushed.0.push((trigger.webview, claims));
        }
    }

    /// Anything pushed that a page could not read as its claimed set.
    #[derive(Resource, Default)]
    struct Rejected(Vec<String>);

    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        shift: false,
        alt: false,
        super_key: false,
    };

    struct Seam;

    impl Seam {
        /// An app holding one binding that always applies and one scoped to `chat.selector`, so a
        /// context change has something to change.
        fn app() -> App {
            let mut keymap = Keymap::default();
            keymap.extend(
                Source::Settings,
                [
                    Binding {
                        shortcut: Shortcut::Direct(KeyCombo {
                            key: KeyCode::KeyX,
                            modifiers: CTRL,
                        }),
                        command: "stack_close".to_string(),
                        when: None,
                    },
                    Binding {
                        shortcut: Shortcut::Direct(KeyCombo {
                            key: KeyCode::Escape,
                            modifiers: Modifiers::default(),
                        }),
                        command: "close_pane".to_string(),
                        when: When::parse("chat.selector"),
                    },
                ],
            );

            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(KeyClaimPlugin)
                .insert_resource(keymap)
                .init_resource::<Pushed>()
                .init_resource::<Rejected>()
                .add_observer(Pushed::record);
            app
        }

        /// A page that exists and has been through one frame, the way a real one has by the time
        /// its bundle is able to emit anything.
        fn page(app: &mut App) -> Entity {
            let entity = app
                .world_mut()
                .spawn(WebviewSource::new("about:blank"))
                .id();
            app.update();
            entity
        }

        /// A page whose components run in the host process, so it has no source to be found by.
        fn hosted_page(app: &mut App) -> Entity {
            let entity = app.world_mut().spawn(HostsPage).id();
            app.update();
            entity
        }

        fn publish(app: &mut App, page: Entity, keys: &[&str]) {
            app.world_mut().trigger(BinReceive {
                webview: page,
                payload: PageKeyContext {
                    keys: keys.iter().map(|key| (*key).to_string()).collect(),
                },
            });
            app.update();
        }
    }

    /// The seam end to end: a page says what is true of it, and is told what it must hand over.
    /// Republishing the same context pushes nothing, or every keystroke that moved a caret would
    /// cost an IPC round trip.
    #[test]
    fn a_page_is_pushed_its_claims_when_its_context_changes() {
        let mut app = Seam::app();
        let page = Seam::page(&mut app);
        let before = Pushed::codes(app.world(), page).len();

        Seam::publish(&mut app, page, &["chat", "chat.selector"]);
        Seam::publish(&mut app, page, &["chat", "chat.selector"]);
        Seam::publish(&mut app, page, &["chat"]);

        assert_eq!(
            Pushed::codes(app.world(), page).split_off(before),
            vec![
                vec!["Escape".to_string(), "KeyX".to_string()],
                vec!["KeyX".to_string()],
            ]
        );
        assert!(app.world().resource::<Rejected>().0.is_empty());
    }

    /// A page whose components run in the host process is still a page.
    ///
    /// The two queries here used to filter on `WebviewSource`, which was a fair way to ask "is
    /// there a page here?" while every page was a URL a browser loaded. The layout stopped being
    /// one, and skipping it is silent: it publishes a context nobody reads and is pushed a claimed
    /// set that never arrives, so every context-scoped binding on it is dead while the keymap
    /// looks fine.
    #[test]
    fn a_page_with_no_webview_is_claimed_for_like_any_other() {
        let mut app = Seam::app();
        let page = Seam::hosted_page(&mut app);
        let before = Pushed::codes(app.world(), page).len();

        Seam::publish(&mut app, page, &["chat", "chat.selector"]);

        assert_eq!(
            Pushed::codes(app.world(), page).split_off(before),
            vec![vec!["Escape".to_string(), "KeyX".to_string()]],
        );
    }

    /// A page that finished listening only after its claims were computed has to be told again. A
    /// page that never hears its claims has no shortcuts at all, and that reads as a broken keymap
    /// rather than as one message arriving early.
    #[test]
    fn a_page_is_claimed_for_again_once_it_reports_ready() {
        let mut app = Seam::app();
        let page = Seam::page(&mut app);
        Seam::publish(&mut app, page, &["chat", "chat.selector"]);
        let before = Pushed::codes(app.world(), page).len();

        app.world_mut().trigger(BinReceive {
            webview: page,
            payload: vmux_core::host::page::PageReady {},
        });
        app.update();

        assert_eq!(
            Pushed::codes(app.world(), page).split_off(before),
            vec![vec!["Escape".to_string(), "KeyX".to_string()]]
        );
    }

    /// Two panes of the same page must be able to disagree, which is why the context hangs off the
    /// entity. A `Resource`-derived context would pass every other test here and fail this one,
    /// because Bevy keeps resource singleton semantics through the `Component` impl it also
    /// provides — only the first entity would ever hold one.
    #[test]
    fn two_pages_are_claimed_for_separately() {
        let mut app = Seam::app();
        let selecting = Seam::page(&mut app);
        let plain = Seam::page(&mut app);

        Seam::publish(&mut app, selecting, &["chat", "chat.selector"]);
        Seam::publish(&mut app, plain, &["chat"]);

        assert_eq!(
            Pushed::codes(app.world(), selecting).last(),
            Some(&vec!["Escape".to_string(), "KeyX".to_string()])
        );
        assert_eq!(
            Pushed::codes(app.world(), plain).last(),
            Some(&vec!["KeyX".to_string()])
        );
    }
}
