use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use vmux_api::space::SpaceKey;
use vmux_command::{CommandDefinition, CommandInvocation, ReadCommandRequests};

pub(crate) struct SpaceKeyPlugin;

impl Plugin for SpaceKeyPlugin {
    fn build(&self, app: &mut App) {
        SpaceKeyRequest::register(app);
        app.add_systems(Update, echo_key_command.in_set(ReadCommandRequests));
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct SpaceKeyRequest {
    caller: Entity,
    key: SpaceKey,
}

impl SpaceKeyRequest {
    pub fn register(app: &mut App) {
        CommandDefinition::register(app, Self::definitions, Self::from_invocation);
    }

    pub fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("space_next", "Next Space", "Layout > Space")
                .hidden()
                .direct_when("ArrowDown", Some("spaces"))
                .direct_when("Ctrl+n", Some("spaces"))
                .direct_when("Ctrl+j", Some("spaces")),
            CommandDefinition::new("space_previous", "Previous Space", "Layout > Space")
                .hidden()
                .direct_when("ArrowUp", Some("spaces"))
                .direct_when("Ctrl+p", Some("spaces"))
                .direct_when("Ctrl+k", Some("spaces")),
            CommandDefinition::new("space_attach", "Open Selected Space", "Layout > Space")
                .hidden()
                .direct_when("Enter", Some("spaces")),
            CommandDefinition::new("space_delete", "Delete Selected Space", "Layout > Space")
                .hidden()
                .direct_when("Delete", Some("spaces"))
                .direct_when("Backspace", Some("spaces")),
        ]
    }

    pub fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
        let key = match invocation.id.as_str() {
            "space_next" => SpaceKey::Next,
            "space_previous" => SpaceKey::Previous,
            "space_attach" => SpaceKey::Attach,
            "space_delete" => SpaceKey::Delete,
            _ => return None,
        };
        Some(Self {
            caller: invocation.caller,
            key,
        })
    }
}

fn echo_key_command(mut requests: MessageReader<SpaceKeyRequest>, mut commands: Commands) {
    for request in requests.read() {
        commands.trigger(BinHostEmitEvent::from_event(request.caller, &request.key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::BinEvent;

    #[derive(Resource, Default)]
    struct Echoed(Vec<(Entity, String)>);

    impl Echoed {
        fn record(trigger: On<BinHostEmitEvent>, mut echoed: ResMut<Self>) {
            let decoded = rkyv::from_bytes::<SpaceKey, rkyv::rancor::Error>(trigger.payload())
                .map(|key| format!("{key:?}"))
                .unwrap_or_else(|_| "undecodable".to_string());
            echoed
                .0
                .push((trigger.webview(), format!("{}:{decoded}", trigger.id())));
        }
    }

    struct Echo;

    impl Echo {
        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_plugins(SpaceKeyPlugin)
                .init_resource::<Echoed>()
                .add_observer(Echoed::record);
            app
        }

        fn issue(app: &mut App, caller: Entity, key: SpaceKey) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<SpaceKeyRequest>>()
                .write(SpaceKeyRequest { caller, key });
            app.update();
        }
    }

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(&mut app, pressed, SpaceKey::Delete);

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{}:Delete", SpaceKey::id()))]
        );
        assert!(
            !app.world()
                .resource::<Echoed>()
                .0
                .iter()
                .any(|(entity, _)| *entity == other)
        );
    }
}
