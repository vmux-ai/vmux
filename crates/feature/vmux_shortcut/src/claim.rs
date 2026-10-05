use bevy::prelude::*;
use bevy_cef::prelude::{UiInput, WebviewSource};
use vmux_api::input::{KeyClaimsUiState, KeyContextRequest};
use vmux_command::{KeyContext, Keymap};
use vmux_ecs::page::HostsPage;
use vmux_ecs::{UiState, UiStatePlugin, UiStateWrite};

pub(crate) struct ClaimPlugin;

type PageHost = Or<(With<WebviewSource>, With<HostsPage>)>;
type MissingKeyContext = (PageHost, Without<KeyContext>);

impl Plugin for ClaimPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiStatePlugin::<KeyClaimsUiState>::default())
            .add_observer(receive)
            .add_systems(Update, (spawn, publish).chain());
    }
}

fn spawn(pages: Query<Entity, MissingKeyContext>, mut commands: Commands) {
    for entity in pages.iter() {
        commands.entity(entity).insert((
            KeyContext::default(),
            UiState::<KeyClaimsUiState>::default(),
        ));
    }
}

fn receive(
    trigger: On<UiInput<KeyContextRequest>>,
    mut contexts: Query<&mut KeyContext>,
    mut commands: Commands,
) {
    let target = trigger.event_target();
    let next = trigger.payload.keys.iter().cloned().collect::<KeyContext>();
    match contexts.get_mut(target) {
        Ok(mut current) => {
            current.set_if_neq(next);
        }
        Err(_) => {
            commands
                .entity(target)
                .insert((next, UiState::<KeyClaimsUiState>::default()));
        }
    }
}

fn publish(
    keymaps: Query<Ref<Keymap>>,
    contexts: Query<(Entity, Ref<KeyContext>), PageHost>,
    mut commands: Commands,
) {
    let Ok(keymap) = keymaps.single() else {
        return;
    };
    for (entity, context) in contexts.iter() {
        if !keymap.is_changed() && !context.is_changed() {
            continue;
        }
        let claims: KeyClaimsUiState = keymap.in_context(&context).claims();
        commands.trigger(UiStateWrite::<KeyClaimsUiState>::from_event(
            entity, &claims,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::KeyCode;
    use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
    use vmux_api::BinEvent;
    use vmux_command::{Binding, KeyCombo, Modifiers, Shortcut, Source, When};

    #[derive(Resource, Default)]
    struct Pushed(Vec<(Entity, KeyClaimsUiState)>);

    impl Pushed {
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
            if trigger.id() != KeyClaimsUiState::id() {
                rejected.0.push(trigger.id().to_string());
                return;
            }
            let Ok(claims) =
                rkyv::from_bytes::<KeyClaimsUiState, rkyv::rancor::Error>(trigger.payload())
            else {
                rejected.0.push("undecodable payload".to_string());
                return;
            };
            pushed.0.push((trigger.webview(), claims));
        }
    }

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
        fn app() -> App {
            let mut keymap = Keymap::default();
            keymap.register(["stack_close", "close_pane"]);
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
                .add_plugins(ClaimPlugin)
                .init_resource::<Pushed>()
                .init_resource::<Rejected>()
                .add_observer(Pushed::record);
            app.world_mut().spawn(keymap);
            app.insert_non_send(Browsers::default());
            app
        }

        fn page(app: &mut App) -> Entity {
            let entity = app
                .world_mut()
                .spawn(WebviewSource::new("about:blank"))
                .id();
            app.world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(entity);
            app.update();
            entity
        }

        fn hosted_page(app: &mut App) -> Entity {
            let entity = app.world_mut().spawn(HostsPage).id();
            app.world_mut()
                .non_send_mut::<Browsers>()
                .set_externally_hosted(entity);
            app.update();
            entity
        }

        fn publish(app: &mut App, page: Entity, keys: &[&str]) {
            app.world_mut().trigger(UiInput {
                webview: page,
                payload: KeyContextRequest {
                    keys: keys.iter().map(|key| (*key).to_string()).collect(),
                },
            });
            app.update();
        }
    }

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

    #[test]
    fn a_page_is_claimed_for_again_once_it_reports_ready() {
        let mut app = Seam::app();
        let page = Seam::page(&mut app);
        Seam::publish(&mut app, page, &["chat", "chat.selector"]);
        let before = Pushed::codes(app.world(), page).len();

        app.world_mut().trigger(UiInput {
            webview: page,
            payload: vmux_ecs::page::PageReady {},
        });
        app.update();

        assert_eq!(
            Pushed::codes(app.world(), page).split_off(before),
            vec![vec!["Escape".to_string(), "KeyX".to_string()]]
        );
    }

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
