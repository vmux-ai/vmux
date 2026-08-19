//! What a conversation calls itself in the tab holding it.
//!
//! The name is a property of the session, but a tab renders from its view's copy of the page's
//! identity, so the report has to be made on the view — the same entity a terminal reports its
//! OSC title on. Reporting it on the session instead would be silently undone, because the view's
//! metadata is cloned upward and takes the identity with it.

use super::AgentChatView;
use bevy::prelude::*;
use vmux_core::PageIdentity;
use vmux_session::AgentConversationTitle;

pub struct ChatTabPlugin;

impl Plugin for ChatTabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, report_conversation_title);
    }
}

fn report_conversation_title(
    renamed: Query<(&AgentConversationTitle, &Children), Changed<AgentConversationTitle>>,
    views: Query<Option<&PageIdentity>, With<AgentChatView>>,
    mut commands: Commands,
) {
    for (title, children) in &renamed {
        for child in children.iter() {
            let Ok(reported) = views.get(child) else {
                continue;
            };
            let mut reported = reported.cloned().unwrap_or_default();
            reported.title = Some(title.0.clone());
            commands.entity(child).insert(reported);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Conversation {
        view: Entity,
        session: Entity,
    }

    impl Conversation {
        fn start(app: &mut App) -> Self {
            app.add_plugins(ChatTabPlugin);
            let session = app.world_mut().spawn(()).id();
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

        fn reported_title(&self, app: &App) -> Option<String> {
            app.world()
                .get::<PageIdentity>(self.view)
                .and_then(|reported| reported.title.clone())
        }
    }

    #[test]
    fn naming_a_conversation_renames_the_view_not_the_session() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        conversation.rename(&mut app, "ship the relay");

        assert_eq!(
            conversation.reported_title(&app).as_deref(),
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
            conversation.reported_title(&app).as_deref(),
            Some("what it turned out to be")
        );
    }

    #[test]
    fn reporting_a_title_leaves_an_icon_already_reported_alone() {
        let mut app = App::new();
        let conversation = Conversation::start(&mut app);
        let icon = vmux_core::PageIcon::favicon("https://example.test/i.png");
        app.world_mut()
            .entity_mut(conversation.view)
            .insert(PageIdentity {
                title: None,
                icon: Some(icon.clone()),
            });
        conversation.rename(&mut app, "named later");

        let reported = app.world().get::<PageIdentity>(conversation.view).unwrap();
        assert_eq!(reported.title.as_deref(), Some("named later"));
        assert_eq!(
            reported.icon.as_ref(),
            Some(&icon),
            "a title report must not blank an icon reported by something else"
        );
    }
}
