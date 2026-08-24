use bevy_app::{App, Plugin, Update};
use bevy_ecs::prelude::*;
use vmux_service::chat::group_turns_tail;
use vmux_wire::chat::ChatItem;
use vmux_wire::page::PageEmit;
use vmux_wire::room::{
    AssistantBlock, Message as RoomMessage, RemoteAgent, RemoteApproval, RemoteEvent,
    RemoteSession, RemoteStatus, RoomEvent, RoomId,
};

use crate::event::{CHAT_SNAPSHOT_EVENT, ChatSnapshot};

pub struct ChatRoomPlugin;

impl Plugin for ChatRoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Reported>()
            .add_message::<Submitted>()
            .add_message::<PageEmit>()
            .init_resource::<Conversation>()
            .init_resource::<Log>()
            .init_resource::<LiveTurn>()
            .init_resource::<Agents>()
            .init_resource::<Snapshot>()
            .add_systems(
                Update,
                (
                    Conversation::fold.before(RoomProjection),
                    Snapshot::project.in_set(RoomProjection).run_if(
                        resource_changed::<Conversation>
                            .or_else(resource_changed::<Log>)
                            .or_else(resource_changed::<LiveTurn>)
                            .or_else(resource_changed::<Agents>),
                    ),
                    Snapshot::emit
                        .after(RoomProjection)
                        .run_if(resource_changed::<Snapshot>),
                ),
            );
    }
}

#[derive(Message)]
pub struct Reported(pub RemoteEvent);

#[derive(Message)]
pub struct Submitted;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RoomProjection;

#[derive(Resource, PartialEq)]
pub struct Conversation {
    pub session: Option<RemoteSession>,
    pub status: RemoteStatus,
    pub approval: Option<RemoteApproval>,
}

impl Default for Conversation {
    fn default() -> Self {
        Self {
            session: None,
            status: RemoteStatus::Idle,
            approval: None,
        }
    }
}

impl Conversation {
    fn fold(
        mut submitted: MessageReader<Submitted>,
        mut reported: MessageReader<Reported>,
        mut conversation: ResMut<Conversation>,
        mut log: ResMut<Log>,
        mut live: ResMut<LiveTurn>,
    ) {
        for Submitted in submitted.read() {
            if conversation.session.is_some() {
                conversation.status = RemoteStatus::Streaming;
            }
        }
        for Reported(event) in reported.read() {
            match event {
                RemoteEvent::Session { session } => {
                    if log
                        .room_id
                        .as_ref()
                        .is_some_and(|room_id| room_id != &session.room_id)
                    {
                        *log = Log::default();
                    }
                    conversation.status = session.status.clone();
                    conversation.approval = session.approval.clone();
                    conversation.session = Some(session.clone());
                }
                RemoteEvent::Snapshot {
                    room_id,
                    through_seq,
                    events,
                } => {
                    let matches_session = conversation
                        .session
                        .as_ref()
                        .is_none_or(|session| &session.room_id == room_id);
                    let has_newer_projection =
                        log.room_id.as_ref() == Some(room_id) && log.through_seq > *through_seq;
                    if matches_session && !has_newer_projection {
                        *log = Log {
                            room_id: Some(room_id.clone()),
                            through_seq: *through_seq,
                            events: events.clone(),
                        };
                        live.0.clear();
                    }
                }
                RemoteEvent::Delta { room_id, text } => {
                    let accepts_delta = log
                        .room_id
                        .as_ref()
                        .is_none_or(|current| current == room_id);
                    if accepts_delta {
                        if log.room_id.is_none() {
                            log.room_id = Some(room_id.clone());
                        }
                        live.0.push_str(text);
                    }
                }
                RemoteEvent::Status { status } => {
                    if !matches!(status, RemoteStatus::Streaming) {
                        conversation.approval = None;
                    }
                    conversation.status = status.clone();
                }
                RemoteEvent::Approval { approval } => conversation.approval = approval.clone(),
            }
        }
    }
}

#[derive(Resource, Default, PartialEq)]
pub struct Log {
    pub room_id: Option<RoomId>,
    pub through_seq: u64,
    pub events: Vec<RoomEvent>,
}

#[derive(Resource, Default, PartialEq)]
pub struct LiveTurn(pub String);

#[derive(Resource, Default, PartialEq)]
pub struct Agents(pub Vec<RemoteAgent>);

#[derive(Resource, Default)]
pub struct Snapshot(pub ChatSnapshot);

impl Snapshot {
    fn emit(snapshot: Res<Snapshot>, mut emits: MessageWriter<PageEmit>) {
        let Some(emit) = PageEmit::of(CHAT_SNAPSHOT_EVENT, &snapshot.0) else {
            return;
        };
        emits.write(emit);
    }

    fn project(
        conversation: Res<Conversation>,
        log: Res<Log>,
        live: Res<LiveTurn>,
        agents: Res<Agents>,
        mut snapshot: ResMut<Snapshot>,
    ) {
        let Some(session) = conversation.session.as_ref() else {
            snapshot.0 = ChatSnapshot::default();
            return;
        };
        let running = matches!(session.status, RemoteStatus::Streaming);
        let items = log.chat_items(&live.0, running);
        let total = items.len() as u32;
        let messages_json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
        let speaker = agents.named(&session.name);
        let (approval_call_id, approval_name, approval_args_json) =
            match conversation.approval.as_ref() {
                Some(pending) => (
                    pending.call_id.clone(),
                    pending.name.clone(),
                    pending.args_json.clone(),
                ),
                None => (String::new(), String::new(), String::new()),
            };
        let error = match &conversation.status {
            RemoteStatus::Errored(message) => message.clone(),
            _ => String::new(),
        };
        let (agent_icon, agent_segment) = match speaker {
            Some(agent) => (agent.icon.clone(), agent.id.as_str()),
            None => (String::new(), ""),
        };
        snapshot.0 = ChatSnapshot {
            messages_json,
            messages_start: 0,
            messages_total: total,
            status: conversation.status.page_status().to_string(),
            error,
            approval_call_id,
            approval_name,
            approval_args_json,
            agent_name: session.name.clone(),
            conversation_title: session.name.clone(),
            agent_icon,
            accent_color: vmux_wire::avatar::agent_color(agent_segment),
            ..ChatSnapshot::default()
        };
    }
}

impl Agents {
    fn named(&self, name: &str) -> Option<&RemoteAgent> {
        self.0.iter().find(|agent| agent.name == name)
    }
}

impl Log {
    pub fn chat_items(&self, live_turn: &str, running: bool) -> Vec<ChatItem> {
        let mut messages = Vec::with_capacity(self.events.len() + 1);
        for event in &self.events {
            messages.push(event.message.clone());
        }
        if !live_turn.is_empty() {
            messages.push(RoomMessage::Assistant {
                blocks: vec![AssistantBlock::Text(live_turn.to_string())],
            });
        }
        group_turns_tail(&[], &messages, &[], running, usize::MAX).items
    }
}

trait PageStatus {
    fn page_status(&self) -> &'static str;
}

impl PageStatus for RemoteStatus {
    fn page_status(&self) -> &'static str {
        match self {
            RemoteStatus::Streaming => "streaming",
            RemoteStatus::Errored(_) => "errored",
            RemoteStatus::Interrupted => "interrupted",
            RemoteStatus::Idle => "idle",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::chat::ChatBlock;

    struct Started(App);

    impl Started {
        fn open() -> Self {
            let mut app = App::new();
            app.add_plugins(ChatRoomPlugin)
                .insert_resource(Conversation::with_agent("ada"))
                .insert_resource(Log::sample());
            app.update();
            Self(app)
        }

        fn snapshot(&self) -> &ChatSnapshot {
            &self.0.world().resource::<Snapshot>().0
        }

        fn items(&self) -> Vec<ChatItem> {
            serde_json::from_str(&self.snapshot().messages_json).expect("a decodable transcript")
        }

        fn insert(&mut self, resource: impl Resource) {
            self.0.insert_resource(resource);
            self.0.update();
        }

        fn report(&mut self, event: RemoteEvent) {
            self.0.world_mut().write_message(Reported(event));
            self.0.update();
        }

        fn submit(&mut self) {
            self.0.world_mut().write_message(Submitted);
            self.0.update();
        }

        fn status(&self) -> RemoteStatus {
            self.0.world().resource::<Conversation>().status.clone()
        }

        fn log(&self) -> &Log {
            self.0.world().resource::<Log>()
        }

        fn snapshot_of(seq: u64, text: &str) -> RemoteEvent {
            RemoteEvent::Snapshot {
                room_id: RoomId::from("r"),
                through_seq: seq,
                events: RoomEvent::from_messages("s", seq, &[RoomMessage::user(text)]),
            }
        }
    }

    impl Conversation {
        fn with_agent(name: &str) -> Self {
            Self {
                session: Some(RemoteSession {
                    sid: "s".to_string(),
                    room_id: RoomId::from("r"),
                    title: String::new(),
                    name: name.to_string(),
                    runtime: String::new(),
                    model: None,
                    cwd: String::new(),
                    status: RemoteStatus::Idle,
                    approval: None,
                    created_at_ms: 0,
                }),
                ..Self::default()
            }
        }
    }

    impl Agents {
        fn of(named: &[(&str, &str)]) -> Self {
            let mut agents = Vec::with_capacity(named.len());
            for (name, icon) in named {
                agents.push(RemoteAgent {
                    id: name.to_string(),
                    name: name.to_string(),
                    url: String::new(),
                    icon: icon.to_string(),
                });
            }
            Self(agents)
        }
    }

    impl Log {
        fn sample() -> Self {
            Self {
                room_id: None,
                through_seq: 0,
                events: RoomEvent::from_messages(
                    "s",
                    0,
                    &[
                        RoomMessage::user("hello"),
                        RoomMessage::Assistant {
                            blocks: vec![AssistantBlock::Thinking("working".to_string())],
                        },
                        RoomMessage::ToolResult {
                            call_id: "tool-1".to_string(),
                            content: "done".to_string(),
                            is_error: false,
                        },
                        RoomMessage::Assistant {
                            blocks: vec![AssistantBlock::Text("answer".to_string())],
                        },
                    ],
                ),
            }
        }
    }

    #[test]
    fn groups_agent_activity_into_one_turn() {
        let items = Log::sample().chat_items("", false);

        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], ChatItem::User { .. }));
        assert!(matches!(
            &items[1],
            ChatItem::Turn(turn) if turn.blocks.len() == 3 && !turn.running
        ));
    }

    #[test]
    fn streaming_delta_extends_the_live_turn() {
        let items = Log::sample().chat_items("partial", true);

        let ChatItem::Turn(turn) = &items[1] else {
            panic!("expected a turn");
        };
        assert!(turn.running);
        assert_eq!(
            turn.blocks.last(),
            Some(&ChatBlock::Text("partial".to_string()))
        );
    }

    #[test]
    fn every_remote_status_names_a_state_the_shared_page_knows() {
        assert_eq!(RemoteStatus::Idle.page_status(), "idle");
        assert_eq!(RemoteStatus::Streaming.page_status(), "streaming");
        assert_eq!(RemoteStatus::Interrupted.page_status(), "interrupted");
        assert_eq!(
            RemoteStatus::Errored("boom".to_string()).page_status(),
            "errored"
        );
    }

    #[test]
    fn a_token_reaches_the_page_without_waiting_for_the_log_to_catch_up() {
        let mut started = Started::open();
        started.insert(LiveTurn("partial".to_string()));

        let items = started.items();
        let Some(ChatItem::Turn(turn)) = items.last() else {
            panic!("expected the transcript to end in a turn");
        };
        assert_eq!(
            turn.blocks.last(),
            Some(&ChatBlock::Text("partial".to_string()))
        );
    }

    #[test]
    fn leaving_a_conversation_empties_the_snapshot() {
        let mut started = Started::open();
        assert!(!started.snapshot().agent_name.is_empty());

        started.insert(Conversation::default());
        assert!(started.snapshot().agent_name.is_empty());
        assert_eq!(started.snapshot().messages_total, 0);
        assert!(started.snapshot().messages_json.is_empty());
    }

    #[test]
    fn a_snapshot_older_than_the_log_is_refused() {
        let mut started = Started::open();
        started.report(Started::snapshot_of(7, "newer"));
        assert_eq!(started.log().through_seq, 7);

        started.report(Started::snapshot_of(3, "older"));
        assert_eq!(started.log().through_seq, 7, "the older replay is dropped");
        started.report(Started::snapshot_of(9, "newest"));
        assert_eq!(started.log().through_seq, 9);
    }

    #[test]
    fn a_snapshot_retires_the_tokens_it_now_contains() {
        let mut started = Started::open();
        started.report(RemoteEvent::Delta {
            room_id: RoomId::from("r"),
            text: "partial".to_string(),
        });
        assert_eq!(started.0.world().resource::<LiveTurn>().0, "partial");

        started.report(Started::snapshot_of(1, "folded"));
        assert!(started.0.world().resource::<LiveTurn>().0.is_empty());
    }

    #[test]
    fn submitting_runs_the_conversation_before_the_relay_says_so() {
        let mut started = Started::open();
        started.submit();

        assert!(matches!(started.status(), RemoteStatus::Streaming));
    }

    #[test]
    fn a_reported_status_overrides_the_guess_in_the_same_turn() {
        let mut started = Started::open();
        started.0.world_mut().write_message(Submitted);
        started.report(RemoteEvent::Status {
            status: RemoteStatus::Idle,
        });

        assert!(matches!(started.status(), RemoteStatus::Idle));
    }

    #[test]
    fn switching_rooms_drops_the_log_the_previous_one_built() {
        let mut started = Started::open();
        started.report(Started::snapshot_of(4, "before"));
        assert!(!started.log().events.is_empty());

        let mut moved = Conversation::with_agent("ada");
        let session = moved.session.as_mut().expect("a session");
        session.room_id = RoomId::from("elsewhere");
        started.report(RemoteEvent::Session {
            session: session.clone(),
        });
        assert!(started.log().events.is_empty());
        assert_eq!(started.log().room_id, None);
    }

    #[test]
    fn a_roster_that_arrives_late_still_names_the_speaker() {
        let mut started = Started::open();
        assert!(started.snapshot().agent_icon.is_empty());

        started.insert(Agents::of(&[("grace", "G"), ("ada", "A")]));
        assert_eq!(
            started.snapshot().agent_icon,
            "A",
            "matched by name, not order"
        );
        assert_eq!(
            started.snapshot().accent_color,
            vmux_wire::avatar::agent_color("ada")
        );
    }
}
