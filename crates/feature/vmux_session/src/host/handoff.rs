use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use vmux_api::session::Message;
use vmux_ecs::Order;

use crate::MessageContent;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ImportedConversation {
    pub source_agent: String,
    pub source_sid: String,
    pub truncated: bool,
    pub first_prompt: Option<String>,
}

impl ImportedConversation {
    pub fn spawn_messages(commands: &mut Commands, session: Entity, messages: Vec<Message>) {
        for (index, message) in messages.into_iter().enumerate() {
            let order = u32::try_from(index).unwrap_or(u32::MAX);
            commands.spawn((
                HandoffEvent,
                Order(order),
                MessageContent(message),
                ChildOf(session),
            ));
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandoffEvent;

#[derive(SystemParam)]
pub struct HandoffMessages<'w, 's> {
    events: Query<
        'w,
        's,
        (&'static ChildOf, &'static Order, &'static MessageContent),
        With<HandoffEvent>,
    >,
}

impl HandoffMessages<'_, '_> {
    pub fn messages(&self, session: Entity) -> Vec<Message> {
        let mut ordered = Vec::new();
        for (parent, order, content) in &self.events {
            if parent.parent() == session {
                ordered.push((order.0, &content.0));
            }
        }
        ordered.sort_by_key(|(order, _)| *order);
        let mut messages = Vec::with_capacity(ordered.len());
        for (_, message) in ordered {
            messages.push(message.clone());
        }
        messages
    }
}
