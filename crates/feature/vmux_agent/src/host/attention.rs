use bevy::prelude::*;
#[cfg(test)]
use vmux_api::protocol::AgentRequest;
use vmux_api::protocol::{AgentTurnEnded, ProcessId};
use vmux_ecs::notify::{AgentAttention, AgentDoneUnseen, BellReceived, OsNotify};
use vmux_ecs::service::ServiceMessageSet;
use vmux_ecs::team::{Agent, Profile};
#[cfg(test)]
use vmux_layout::active_pane::ActiveStack;
use vmux_layout::stack::{ComputeFocusSet, FocusedStack, Stack};

use crate::host::event::AgentRequestInput;
use vmux_ecs::agent::SessionId;

pub(super) struct AttentionPlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct TurnEndedSet;

impl Plugin for AttentionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                bell,
                handle_turn_ended
                    .in_set(TurnEndedSet)
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

const DONE_DEDUP_WINDOW_SECS: f64 = 3.0;

#[derive(bevy::ecs::system::SystemParam)]
struct AttentionContext<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    focused: FocusedStack<'w, 's>,
    stacks: Query<'w, 's, (), With<Stack>>,
    child_of: Query<'w, 's, &'static ChildOf>,
}

impl AttentionContext<'_, '_> {
    fn foreground(&self) -> bool {
        self.windows
            .iter()
            .next()
            .map(|window| window.focused && window.visible)
            .unwrap_or(false)
    }

    fn stack(&self, entity: Entity) -> Option<Entity> {
        self.stacks
            .get(entity)
            .is_ok()
            .then_some(entity)
            .or_else(|| self.child_of.get(entity).ok().map(|child| child.parent()))
    }

    fn viewed(&self, entity: Entity) -> bool {
        self.foreground() && self.focused.stack == self.stack(entity)
    }
}

fn mark_done(
    mut reader: MessageReader<AgentAttention>,
    mut notify: MessageWriter<OsNotify>,
    context: AttentionContext,
    meta: Query<(&Profile, Option<&SessionId>, Option<&Agent>)>,
    time: Res<Time>,
    mut last_notify: Local<std::collections::HashMap<Entity, f64>>,
    mut commands: Commands,
) {
    for att in reader.read() {
        if context.viewed(att.entity) {
            commands.entity(att.entity).remove::<AgentDoneUnseen>();
            continue;
        }
        commands.entity(att.entity).insert(AgentDoneUnseen);
        let now = time.elapsed_secs_f64();
        if last_notify
            .get(&att.entity)
            .is_some_and(|t| now - t < DONE_DEDUP_WINDOW_SECS)
        {
            continue;
        }
        last_notify.insert(att.entity, now);
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
    context: AttentionContext,
    mut prev_focused: Local<Option<Entity>>,
    mut commands: Commands,
) {
    let current = if context.foreground() {
        context.focused.stack
    } else {
        None
    };
    if current == *prev_focused {
        return;
    }
    *prev_focused = current;
    let Some(stack) = current else {
        return;
    };
    for entity in &done {
        if context.stack(entity) == Some(stack) {
            commands.entity(entity).remove::<AgentDoneUnseen>();
        }
    }
}

fn handle_turn_ended(
    mut reader: MessageReader<AgentRequestInput>,
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

    pub(crate) fn bell_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<BellReceived>()
            .add_message::<AgentAttention>()
            .add_systems(Update, bell);
        app
    }

    pub(crate) fn spawn_agent_with_pid(app: &mut App, pid: ProcessId) -> Entity {
        app.world_mut()
            .spawn((
                Agent {
                    sid: "s".to_string(),
                },
                pid,
            ))
            .id()
    }

    pub(crate) fn attentions(app: &App) -> Vec<Entity> {
        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<AgentAttention>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|a| a.entity).collect()
    }

    pub(crate) fn turn_end_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AgentRequestInput>()
            .add_message::<AgentAttention>()
            .add_systems(Update, handle_turn_ended);
        app
    }

    pub(crate) fn send_turn_ended(app: &mut App, anchor: ProcessId) {
        app.world_mut()
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

    #[test]
    fn turn_ended_resolves_to_agent_attention() {
        let mut app = turn_end_test_app();
        let pid = ProcessId::new();
        let agent = spawn_agent_with_pid(&mut app, pid);
        send_turn_ended(&mut app, pid);
        app.update();
        assert_eq!(attentions(&app), vec![agent]);
    }

    #[test]
    fn turn_ended_unknown_anchor_emits_nothing() {
        let mut app = turn_end_test_app();
        let _agent = spawn_agent_with_pid(&mut app, ProcessId::new());
        send_turn_ended(&mut app, ProcessId::new());
        app.update();
        assert!(attentions(&app).is_empty());
    }

    #[test]
    fn bell_resolves_to_agent_attention() {
        let mut app = bell_test_app();
        let pid = ProcessId::new();
        let agent = spawn_agent_with_pid(&mut app, pid);
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<BellReceived>>()
            .write(BellReceived { process_id: pid });
        app.update();
        assert_eq!(attentions(&app), vec![agent]);
    }

    #[test]
    fn bell_unknown_process_id_emits_nothing() {
        let mut app = bell_test_app();
        let _agent = spawn_agent_with_pid(&mut app, ProcessId::new());
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<BellReceived>>()
            .write(BellReceived {
                process_id: ProcessId::new(),
            });
        app.update();
        assert!(attentions(&app).is_empty());
    }

    pub(crate) fn done_test_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, vmux_layout::LayoutContractPlugin))
            .add_message::<AgentAttention>()
            .add_message::<OsNotify>()
            .add_systems(Update, (mark_done, clear_done));
        app.world_mut().spawn(ActiveStack::default().local_bundle());
        app
    }

    pub(crate) fn spawn_agent_in_stack(app: &mut App) -> (Entity, Entity) {
        let stack = app.world_mut().spawn(Stack::default()).id();
        let agent = app
            .world_mut()
            .spawn((Profile::registry("Agent", "test-agent"), ChildOf(stack)))
            .id();
        (agent, stack)
    }

    pub(crate) fn set_window(app: &mut App, focused: bool) {
        app.world_mut().spawn((
            Window {
                focused,
                visible: true,
                ..default()
            },
            bevy::window::PrimaryWindow,
        ));
    }

    pub(crate) fn os_notify_count(app: &App) -> usize {
        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<OsNotify>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).count()
    }

    pub(crate) fn send_attention(app: &mut App, entity: Entity) {
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentAttention>>()
            .write(AgentAttention {
                entity,
                title: None,
                body: None,
            });
    }

    fn focus_stack(app: &mut App, stack: Entity) {
        let world = app.world_mut();
        let mut query = world.query::<&mut ActiveStack>();
        query.single_mut(world).unwrap().stack = Some(stack);
    }

    #[test]
    fn done_notifies_and_marks_when_backgrounded() {
        let mut app = done_test_app();
        let (agent, _stack) = spawn_agent_in_stack(&mut app);
        set_window(&mut app, false);
        send_attention(&mut app, agent);
        app.update();
        assert!(app.world().get::<AgentDoneUnseen>(agent).is_some());
        assert_eq!(os_notify_count(&app), 1);
    }

    #[test]
    fn focused_child_agent_does_not_notify_or_mark() {
        let mut app = done_test_app();
        let (agent, stack) = spawn_agent_in_stack(&mut app);
        set_window(&mut app, true);
        focus_stack(&mut app, stack);
        app.update();
        send_attention(&mut app, agent);
        app.update();
        assert!(
            app.world().get::<AgentDoneUnseen>(agent).is_none(),
            "focused agent has no unseen marker"
        );
        assert_eq!(os_notify_count(&app), 0, "no banner when foreground");
    }

    #[test]
    fn focused_stack_agent_does_not_notify_or_mark() {
        let mut app = done_test_app();
        let stack = app
            .world_mut()
            .spawn((Stack::default(), Profile::registry("Agent", "test-agent")))
            .id();
        set_window(&mut app, true);
        focus_stack(&mut app, stack);
        app.update();
        send_attention(&mut app, stack);
        app.update();
        assert!(
            app.world().get::<AgentDoneUnseen>(stack).is_none(),
            "focused stack agent has no unseen marker"
        );
        assert_eq!(os_notify_count(&app), 0, "no banner when foreground");
    }

    #[test]
    fn clear_removes_marker_from_focused_stack_agent() {
        let mut app = done_test_app();
        let stack = app.world_mut().spawn(Stack::default()).id();
        set_window(&mut app, true);
        app.world_mut().entity_mut(stack).insert(AgentDoneUnseen);
        app.update();
        assert!(app.world().get::<AgentDoneUnseen>(stack).is_some());
        focus_stack(&mut app, stack);
        app.update();
        assert!(app.world().get::<AgentDoneUnseen>(stack).is_none());
    }
}
