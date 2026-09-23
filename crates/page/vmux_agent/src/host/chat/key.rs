use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use vmux_chat::event::ChatKey;
use vmux_command::{CommandDefinition, CommandInvocation, ReadCommandRequests};

pub(crate) struct ChatKeyPlugin;

impl Plugin for ChatKeyPlugin {
    fn build(&self, app: &mut App) {
        ChatKeyRequest::register(app);
        app.add_systems(Update, echo_key_command.in_set(ReadCommandRequests));
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
struct ChatKeyRequest {
    caller: Entity,
    key: ChatKey,
}

impl ChatKeyRequest {
    pub fn register(app: &mut App) {
        CommandDefinition::register(app, Self::definitions, Self::from_invocation);
    }

    pub fn definitions() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition::new("chat_list_next", "Next Option", "Chat")
                .hidden()
                .direct_when("ArrowDown", Some("chat.list"))
                .direct_when("Ctrl+n", Some("chat.list"))
                .direct_when("Ctrl+j", Some("chat.list")),
            CommandDefinition::new("chat_list_previous", "Previous Option", "Chat")
                .hidden()
                .direct_when("ArrowUp", Some("chat.list"))
                .direct_when("Ctrl+p", Some("chat.list"))
                .direct_when("Ctrl+k", Some("chat.list")),
            CommandDefinition::new("chat_list_choose", "Choose Option", "Chat")
                .hidden()
                .direct_when("Enter", Some("chat.list")),
            CommandDefinition::new("chat_history_older", "Previous Prompt", "Chat")
                .hidden()
                .direct_when("ArrowUp", Some("chat && !chat.list"))
                .direct_when("Ctrl+p", Some("chat && !chat.list")),
            CommandDefinition::new("chat_history_newer", "Next Prompt", "Chat")
                .hidden()
                .direct_when("ArrowDown", Some("chat && !chat.list"))
                .direct_when("Ctrl+n", Some("chat && !chat.list")),
            CommandDefinition::new("chat_submit", "Send Prompt", "Chat")
                .hidden()
                .direct_when("Enter", Some("chat && !chat.list")),
            CommandDefinition::new("chat_dismiss_selector", "Close Picker", "Chat")
                .hidden()
                .direct_when("Escape", Some("chat.selector")),
            CommandDefinition::new("chat_interrupt", "Send Queued Now", "Chat")
                .hidden()
                .direct_when("Escape", Some("chat && !chat.selector")),
            CommandDefinition::new("chat_cancel", "Stop Turn", "Chat")
                .hidden()
                .direct_when("Ctrl+c", Some("chat")),
        ]
    }

    pub fn from_invocation(invocation: &CommandInvocation) -> Option<Self> {
        let key = match invocation.id.as_str() {
            "chat_list_next" => ChatKey::ListNext,
            "chat_list_previous" => ChatKey::ListPrevious,
            "chat_list_choose" => ChatKey::ListChoose,
            "chat_history_older" => ChatKey::HistoryOlder,
            "chat_history_newer" => ChatKey::HistoryNewer,
            "chat_submit" => ChatKey::Submit,
            "chat_dismiss_selector" => ChatKey::DismissSelector,
            "chat_interrupt" => ChatKey::Interrupt,
            "chat_cancel" => ChatKey::Cancel,
            _ => return None,
        };
        Some(Self {
            caller: invocation.caller,
            key,
        })
    }
}

fn echo_key_command(mut requests: MessageReader<ChatKeyRequest>, mut commands: Commands) {
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
            let decoded = rkyv::from_bytes::<ChatKey, rkyv::rancor::Error>(trigger.payload())
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
                .add_plugins(ChatKeyPlugin)
                .init_resource::<Echoed>()
                .add_observer(Echoed::record);
            app
        }

        fn issue(app: &mut App, caller: Entity, key: ChatKey) {
            app.world_mut()
                .resource_mut::<bevy::ecs::message::Messages<ChatKeyRequest>>()
                .write(ChatKeyRequest { caller, key });
            app.update();
        }
    }

    #[test]
    fn a_resolved_key_reaches_only_the_page_that_sent_it() {
        let mut app = Echo::app();
        let pressed = app.world_mut().spawn_empty().id();
        let other = app.world_mut().spawn_empty().id();

        Echo::issue(&mut app, pressed, ChatKey::ListChoose);

        assert_eq!(
            app.world().resource::<Echoed>().0,
            vec![(pressed, format!("{}:ListChoose", ChatKey::id()))]
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
