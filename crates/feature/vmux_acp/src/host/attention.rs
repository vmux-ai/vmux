use std::collections::HashSet;

use bevy::prelude::*;
#[cfg(test)]
use vmux_api::protocol::AgentRequest;
use vmux_api::protocol::{AgentTurnEnded, ProcessId};
use vmux_ecs::agent::AgentRequestReady;
use vmux_ecs::notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
use vmux_ecs::service::ServiceMessageSet;
use vmux_ecs::team::{Agent, Profile};
#[cfg(test)]
use vmux_layout::active_pane::ActiveStack;
use vmux_layout::stack::ComputeFocusSet;
#[cfg(test)]
use vmux_layout::stack::Stack;

use super::attention_driver::AttentionContext;
#[cfg(test)]
use crate::host::event::AgentRequestInput;
use vmux_session::SessionId;

pub(super) struct AttentionPlugin;

impl Plugin for AttentionPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<vmux_ecs::agent::AgentRequestDispatchPlugin>() {
            app.add_plugins(vmux_ecs::agent::AgentRequestDispatchPlugin);
        }
        app.add_message::<BellReceived>()
            .add_message::<AgentAttention>()
            .add_message::<OsNotify>()
            .add_systems(
                Update,
                (
                    bell,
                    handle_turn_ended
                        .in_set(TurnEndedSet)
                        .after(vmux_ecs::agent::AgentRequestDispatchSet)
                        .after(ServiceMessageSet),
                )
                    .chain()
                    .after(ComputeFocusSet),
            )
            .add_systems(
                Update,
                (mark_done, clear_done)
                    .chain()
                    .after(ComputeFocusSet)
                    .after(super::tidy::TidySet),
            );
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct TurnEndedSet;

#[derive(Component)]
struct LastAttentionNotified(f64);

impl LastAttentionNotified {
    const DEDUP_WINDOW_SECS: f64 = 3.0;
}

#[derive(Component)]
struct PreviouslyFocusedStack;

fn bell(
    mut reader: MessageReader<BellReceived>,
    mut attention: MessageWriter<AgentAttention>,
    agents: Query<(Entity, &ProcessId), With<Agent>>,
) {
    for ev in reader.read() {
        if let Some((entity, _)) = agents.iter().find(|(_, pid)| **pid == ev.process_id) {
            attention.write(AgentAttention {
                entity,
                title: None,
                body: None,
            });
        }
    }
}

fn mark_done(
    mut reader: MessageReader<AgentAttention>,
    mut notify: MessageWriter<OsNotify>,
    context: AttentionContext,
    meta: Query<(&Profile, Option<&SessionId>, Option<&Agent>)>,
    last_notified: Query<&LastAttentionNotified>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let mut notified_this_frame = HashSet::new();
    for att in reader.read() {
        if context.viewed(att.entity) {
            commands.entity(att.entity).remove::<AgentDoneUnseen>();
            continue;
        }
        commands.entity(att.entity).insert(AgentDoneUnseen);
        let now = time.elapsed_secs_f64();
        let recently_notified = !notified_this_frame.insert(att.entity)
            || last_notified
                .get(att.entity)
                .is_ok_and(|last| now - last.0 < LastAttentionNotified::DEDUP_WINDOW_SECS);
        if recently_notified {
            continue;
        }
        commands
            .entity(att.entity)
            .insert(LastAttentionNotified(now));
        let (name, sid) = match meta.get(att.entity) {
            Ok((profile, session, agent)) => {
                let sid = session
                    .map(|s| s.0.clone())
                    .filter(|s| !s.is_empty())
                    .or_else(|| agent.map(|a| a.sid.clone()).filter(|s| !s.is_empty()))
                    .unwrap_or_default();
                (profile.name.clone(), sid)
            }
            Err(_) => ("Agent".to_string(), String::new()),
        };
        let short_sid: String = sid.chars().take(8).collect();
        let title = att
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{name} finished"));
        let body = att
            .body
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                if short_sid.is_empty() {
                    String::new()
                } else {
                    format!("session {short_sid}")
                }
            });
        notify.write(OsNotify { title, body });
    }
}

fn clear_done(
    done: Query<Entity, With<AgentDoneUnseen>>,
    previous: Query<Entity, With<PreviouslyFocusedStack>>,
    context: AttentionContext,
    mut commands: Commands,
) {
    let current = if context.foreground() {
        context.focused.stack
    } else {
        None
    };
    let prev = previous.iter().next();
    if current == prev {
        return;
    }
    for entity in &previous {
        commands.entity(entity).remove::<PreviouslyFocusedStack>();
    }
    let Some(stack) = current else {
        return;
    };
    commands.entity(stack).insert(PreviouslyFocusedStack);
    for entity in &done {
        if context.stack(entity) == Some(stack) {
            commands.entity(entity).remove::<AgentDoneUnseen>();
        }
    }
}

fn handle_turn_ended(
    mut reader: MessageReader<AgentRequestReady>,
    agents: Query<(Entity, &ProcessId), With<Agent>>,
    mut attention: MessageWriter<AgentAttention>,
) {
    for request in reader.read() {
        let Ok(Some(command)) = request.decode::<AgentTurnEnded>() else {
            continue;
        };
        let anchor = &command.anchor;
        if let Some((entity, _)) = agents.iter().find(|(_, pid)| *pid == anchor) {
            attention.write(AgentAttention {
                entity,
                title: None,
                body: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::event::CommandOrigin;

    struct AttentionFixture(App);

    impl AttentionFixture {
        fn new() -> Self {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                vmux_layout::LayoutContractPlugin,
                AttentionPlugin,
            ))
            .add_message::<AgentRequestInput>();
            app.world_mut().spawn(ActiveStack::default().local_bundle());
            Self(app)
        }

        fn spawn_agent_with_pid(&mut self, pid: ProcessId) -> Entity {
            self.0
                .world_mut()
                .spawn((
                    Agent {
                        sid: "s".to_string(),
                    },
                    pid,
                ))
                .id()
        }

        fn spawn_agent_in_stack(&mut self) -> (Entity, Entity) {
            let stack = self.0.world_mut().spawn(Stack::default()).id();
            let agent = self
                .0
                .world_mut()
                .spawn((Profile::registry("Agent", "test-agent"), ChildOf(stack)))
                .id();
            (agent, stack)
        }

        fn set_window(&mut self, focused: bool) {
            self.0.world_mut().spawn((
                Window {
                    focused,
                    visible: true,
                    ..default()
                },
                bevy::window::PrimaryWindow,
            ));
        }

        fn focus_stack(&mut self, stack: Entity) {
            let world = self.0.world_mut();
            let mut query = world.query::<&mut ActiveStack>();
            query.single_mut(world).unwrap().stack = Some(stack);
        }

        fn send_attention(&mut self, entity: Entity) {
            self.0
                .world_mut()
                .resource_mut::<bevy::ecs::message::Messages<AgentAttention>>()
                .write(AgentAttention {
                    entity,
                    title: None,
                    body: None,
                });
        }

        fn send_turn_ended(&mut self, anchor: ProcessId) {
            self.0
                .world_mut()
                .resource_mut::<bevy::ecs::message::Messages<AgentRequestInput>>()
                .write(AgentRequestInput {
                    request_id: vmux_api::protocol::AgentRequestId::new(),
                    origin: CommandOrigin::Agent {
                        sid: None,
                        anchor: Some(anchor),
                    },
                    request: AgentRequest::encode(&AgentTurnEnded { anchor }).unwrap(),
                });
        }

        fn attentions(&self) -> Vec<Entity> {
            let messages = self
                .0
                .world()
                .resource::<bevy::ecs::message::Messages<AgentAttention>>();
            let mut cursor = messages.get_cursor();
            cursor.read(messages).map(|a| a.entity).collect()
        }

        fn os_notify_count(&self) -> usize {
            let messages = self
                .0
                .world()
                .resource::<bevy::ecs::message::Messages<OsNotify>>();
            let mut cursor = messages.get_cursor();
            cursor.read(messages).count()
        }
    }

    #[test]
    fn turn_ended_resolves_to_agent_attention() {
        let mut fixture = AttentionFixture::new();
        let pid = ProcessId::new();
        let agent = fixture.spawn_agent_with_pid(pid);
        fixture.send_turn_ended(pid);
        fixture.0.update();
        assert_eq!(fixture.attentions(), vec![agent]);
    }

    #[test]
    fn turn_ended_unknown_anchor_emits_nothing() {
        let mut fixture = AttentionFixture::new();
        let _agent = fixture.spawn_agent_with_pid(ProcessId::new());
        fixture.send_turn_ended(ProcessId::new());
        fixture.0.update();
        assert!(fixture.attentions().is_empty());
    }

    #[test]
    fn bell_resolves_to_agent_attention() {
        let mut fixture = AttentionFixture::new();
        let pid = ProcessId::new();
        let agent = fixture.spawn_agent_with_pid(pid);
        fixture
            .0
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<BellReceived>>()
            .write(BellReceived { process_id: pid });
        fixture.0.update();
        assert_eq!(fixture.attentions(), vec![agent]);
    }

    #[test]
    fn bell_unknown_process_id_emits_nothing() {
        let mut fixture = AttentionFixture::new();
        let _agent = fixture.spawn_agent_with_pid(ProcessId::new());
        fixture
            .0
            .world_mut()
            .resource_mut::<bevy::ecs::message::Messages<BellReceived>>()
            .write(BellReceived {
                process_id: ProcessId::new(),
            });
        fixture.0.update();
        assert!(fixture.attentions().is_empty());
    }

    #[test]
    fn done_notifies_and_marks_when_backgrounded() {
        let mut fixture = AttentionFixture::new();
        let (agent, _stack) = fixture.spawn_agent_in_stack();
        fixture.set_window(false);
        fixture.send_attention(agent);
        fixture.0.update();
        assert!(fixture.0.world().get::<AgentDoneUnseen>(agent).is_some());
        assert_eq!(fixture.os_notify_count(), 1);
    }

    #[test]
    fn focused_child_agent_does_not_notify_or_mark() {
        let mut fixture = AttentionFixture::new();
        let (agent, stack) = fixture.spawn_agent_in_stack();
        fixture.set_window(true);
        fixture.focus_stack(stack);
        fixture.0.update();
        fixture.send_attention(agent);
        fixture.0.update();
        assert!(
            fixture.0.world().get::<AgentDoneUnseen>(agent).is_none(),
            "focused agent has no unseen marker"
        );
        assert_eq!(fixture.os_notify_count(), 0, "no banner when foreground");
    }

    #[test]
    fn focused_stack_agent_does_not_notify_or_mark() {
        let mut fixture = AttentionFixture::new();
        let stack = fixture
            .0
            .world_mut()
            .spawn((Stack::default(), Profile::registry("Agent", "test-agent")))
            .id();
        fixture.set_window(true);
        fixture.focus_stack(stack);
        fixture.0.update();
        fixture.send_attention(stack);
        fixture.0.update();
        assert!(
            fixture.0.world().get::<AgentDoneUnseen>(stack).is_none(),
            "focused stack agent has no unseen marker"
        );
        assert_eq!(fixture.os_notify_count(), 0, "no banner when foreground");
    }

    #[test]
    fn clear_removes_marker_from_focused_stack_agent() {
        let mut fixture = AttentionFixture::new();
        let stack = fixture.0.world_mut().spawn(Stack::default()).id();
        fixture.set_window(true);
        fixture
            .0
            .world_mut()
            .entity_mut(stack)
            .insert(AgentDoneUnseen);
        fixture.0.update();
        assert!(fixture.0.world().get::<AgentDoneUnseen>(stack).is_some());
        fixture.focus_stack(stack);
        fixture.0.update();
        assert!(fixture.0.world().get::<AgentDoneUnseen>(stack).is_none());
    }
}
