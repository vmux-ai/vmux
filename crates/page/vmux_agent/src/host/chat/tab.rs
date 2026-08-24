use super::AgentChatView;
use crate::host::run_state::AgentRunState;
use bevy::prelude::*;
use vmux_chat::activity::ActivityIcon;
use vmux_chat::tab::Accent;
use vmux_core::team::Profile;
use vmux_core::{PageIcon, PageIdentity};
use vmux_service::chat::group_turns_tail;
use vmux_session::{AgentConversationTitle, AgentMessages, AgentSession};

pub struct ChatTabPlugin;

impl Plugin for ChatTabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, report_tab_identity);
    }
}

const TAIL_ITEMS: usize = 1;

fn report_tab_identity(
    sessions: Query<
        (
            &Children,
            Option<&AgentConversationTitle>,
            &AgentMessages,
            &AgentRunState,
            Option<&Profile>,
            Option<&AgentSession>,
        ),
        Or<(
            Changed<AgentConversationTitle>,
            Changed<AgentMessages>,
            Changed<AgentRunState>,
            Changed<Profile>,
        )>,
    >,
    views: Query<Option<&PageIdentity>, With<AgentChatView>>,
    mut commands: Commands,
) {
    for (children, title, messages, state, profile, session) in &sessions {
        for child in children.iter() {
            let Ok(reported) = views.get(child) else {
                continue;
            };
            let mut reported = reported.cloned().unwrap_or_default();
            if let Some(title) = title {
                reported.title = Some(title.0.clone());
            }
            reported.icon = activity_icon(messages, state, profile, session);
            commands.entity(child).insert(reported);
        }
    }
}

fn activity_icon(
    messages: &AgentMessages,
    state: &AgentRunState,
    profile: Option<&Profile>,
    session: Option<&AgentSession>,
) -> Option<PageIcon> {
    let running = matches!(state, AgentRunState::Streaming);
    let page = group_turns_tail(&[], &messages.0, &[], running, TAIL_ITEMS);
    let activity = ActivityIcon::current(&page.items, state.status())?;
    let provider = session
        .map(|session| session.provider.as_str())
        .unwrap_or_default();
    let accent = Accent::of_agent(
        profile
            .map(|profile| profile.avatar.color.as_str())
            .unwrap_or_default(),
        provider,
    );
    Some(PageIcon::favicon(activity.favicon(&accent.css)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_wire::chat::{ChatBlock, ChatTurn};

    struct Conversation {
        view: Entity,
        session: Entity,
    }

    impl Conversation {
        fn start(app: &mut App) -> Self {
            app.add_plugins(ChatTabPlugin);
            let session = app
                .world_mut()
                .spawn((AgentMessages::default(), AgentRunState::default()))
                .id();
            let view = app
                .world_mut()
                .spawn((AgentChatView, ChildOf(session)))
                .id();
            Conversation { view, session }
        }

        fn rename(&self, app: &mut App, title: &str) {
            app.world_mut()
                .entity_mut(self.session)
                .insert(AgentConversationTitle(title.to_string()));
            app.update();
        }

        fn run(&self, app: &mut App, state: AgentRunState) {
            app.world_mut().entity_mut(self.session).insert(state);
            app.update();
        }

        fn reported(&self, app: &App) -> PageIdentity {
            app.world()
                .get::<PageIdentity>(self.view)
                .cloned()
                .unwrap_or_default()
        }
    }

    #[test]
    fn naming_a_conversation_renames_the_view_not_the_session() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.rename(&mut app, "ship the relay");

        assert_eq!(
            conversation.reported(&app).title.as_deref(),
            Some("ship the relay")
        );
        assert!(
            app.world()
                .get::<PageIdentity>(conversation.session)
                .is_none(),
            "the session is not what a tab renders from, so reporting there would be lost"
        );
    }

    #[test]
    fn renaming_again_replaces_the_reported_title() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.rename(&mut app, "first guess");
        conversation.rename(&mut app, "what it turned out to be");

        assert_eq!(
            conversation.reported(&app).title.as_deref(),
            Some("what it turned out to be")
        );
    }

    #[test]
    fn an_idle_agent_reports_no_icon_so_its_own_shows_through() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.run(&mut app, AgentRunState::Idle);

        assert_eq!(conversation.reported(&app).icon, None);
    }

    #[test]
    fn the_icon_tracks_what_the_agent_is_doing() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);

        conversation.run(
            &mut app,
            AgentRunState::AwaitingApproval {
                call_id: "1".into(),
                name: "run".into(),
                args: serde_json::Value::Null,
            },
        );
        let awaiting = conversation.reported(&app).icon;
        assert!(awaiting.is_some(), "waiting on the user is worth showing");

        conversation.run(&mut app, AgentRunState::Errored("boom".into()));
        assert_ne!(
            conversation.reported(&app).icon,
            awaiting,
            "a failed turn must not keep showing the approval icon"
        );

        conversation.run(&mut app, AgentRunState::Idle);
        assert_eq!(
            conversation.reported(&app).icon,
            None,
            "settling back to idle hands the tab back to the agent's own icon"
        );
    }

    #[test]
    fn a_streaming_agent_is_read_from_the_last_block_of_the_running_turn() {
        let thinking = ActivityIcon::current(
            &[vmux_wire::chat::ChatItem::Turn(ChatTurn {
                running: true,
                blocks: vec![ChatBlock::Thinking(String::new())],
                ..Default::default()
            })],
            "streaming",
        );
        let writing = ActivityIcon::current(
            &[vmux_wire::chat::ChatItem::Turn(ChatTurn {
                running: true,
                blocks: vec![
                    ChatBlock::Thinking(String::new()),
                    ChatBlock::Text(String::new()),
                ],
                ..Default::default()
            })],
            "streaming",
        );

        assert_eq!(thinking, Some(ActivityIcon::Thinking));
        assert_eq!(
            writing,
            Some(ActivityIcon::Writing),
            "the newest block wins, or the icon lags a turn behind what the agent is doing"
        );
    }
}
