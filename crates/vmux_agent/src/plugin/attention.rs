//! Turn-end: how an agent says it is done, and what the app does about it.
//!
//! A CLI agent rings the terminal bell or fires its stop hook; both resolve to
//! [`AgentAttention`](vmux_core::notify::AgentAttention). If the agent is off screen when that
//! lands it gets a done marker and an OS notification, cleared when the user looks at its stack.

use bevy::prelude::*;
use vmux_service::protocol::AgentCommand as ServiceAgentCommand;

use crate::events::AgentCommandRequest;
use crate::session::SessionId;

pub(super) struct AttentionPlugin;

impl Plugin for AttentionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (agent_bell_to_attention, handle_agent_turn_ended)
                .chain()
                .after(vmux_layout::stack::ComputeFocusSet),
        )
        .add_systems(
            Update,
            (mark_agent_done, clear_agent_done)
                .chain()
                .after(vmux_layout::stack::ComputeFocusSet)
                .after(super::follow::tidy_on_agent_attention),
        );
    }
}

fn agent_bell_to_attention(
    mut reader: MessageReader<vmux_core::notify::BellReceived>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
    agents: Query<(Entity, &vmux_service::protocol::ProcessId), With<vmux_core::team::Agent>>,
) {
    for ev in reader.read() {
        if let Some((entity, _)) = agents.iter().find(|(_, pid)| **pid == ev.process_id) {
            attention.write(vmux_core::notify::AgentAttention {
                entity,
                title: None,
                body: None,
            });
        }
    }
}

pub(crate) const DONE_DEDUP_WINDOW_SECS: f64 = 3.0;

pub(crate) fn window_foreground(
    windows: &Query<&Window, With<bevy::window::PrimaryWindow>>,
) -> bool {
    windows
        .iter()
        .next()
        .map(|w| w.focused && w.visible)
        .unwrap_or(false)
}

pub(crate) fn agent_is_viewed(
    entity: Entity,
    foreground: bool,
    focused: &vmux_layout::stack::FocusedStack,
    stacks: &Query<(), With<vmux_layout::stack::Stack>>,
    child_of: &Query<&ChildOf>,
) -> bool {
    foreground && focused.stack == agent_stack(entity, stacks, child_of)
}

pub(crate) fn agent_stack(
    entity: Entity,
    stacks: &Query<(), With<vmux_layout::stack::Stack>>,
    child_of: &Query<&ChildOf>,
) -> Option<Entity> {
    stacks
        .get(entity)
        .is_ok()
        .then_some(entity)
        .or_else(|| child_of.get(entity).ok().map(|child| child.parent()))
}

fn mark_agent_done(
    mut reader: MessageReader<vmux_core::notify::AgentAttention>,
    mut notify: MessageWriter<vmux_core::notify::OsNotify>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    focused: Res<vmux_layout::stack::FocusedStack>,
    stacks: Query<(), With<vmux_layout::stack::Stack>>,
    child_of: Query<&ChildOf>,
    meta: Query<(
        &vmux_core::team::Profile,
        Option<&SessionId>,
        Option<&vmux_core::team::Agent>,
    )>,
    time: Res<Time>,
    mut last_notify: Local<std::collections::HashMap<Entity, f64>>,
    mut commands: Commands,
) {
    let foreground = window_foreground(&windows);
    for att in reader.read() {
        if agent_is_viewed(att.entity, foreground, &focused, &stacks, &child_of) {
            commands
                .entity(att.entity)
                .remove::<vmux_core::notify::AgentDoneUnseen>();
            continue;
        }
        commands
            .entity(att.entity)
            .insert(vmux_core::notify::AgentDoneUnseen);
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
        notify.write(vmux_core::notify::OsNotify { title, body });
    }
}

fn clear_agent_done(
    done: Query<Entity, With<vmux_core::notify::AgentDoneUnseen>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    focused: Res<vmux_layout::stack::FocusedStack>,
    stacks: Query<(), With<vmux_layout::stack::Stack>>,
    child_of: Query<&ChildOf>,
    mut prev_focused: Local<Option<Entity>>,
    mut commands: Commands,
) {
    let foreground = window_foreground(&windows);
    let current = if foreground { focused.stack } else { None };
    if current == *prev_focused {
        return;
    }
    *prev_focused = current;
    let Some(stack) = current else {
        return;
    };
    for entity in &done {
        if agent_stack(entity, &stacks, &child_of) == Some(stack) {
            commands
                .entity(entity)
                .remove::<vmux_core::notify::AgentDoneUnseen>();
        }
    }
}

/// CLI agents fire this from their `Stop` hook at turn-end: resolve the agent terminal by its
/// `anchor` `ProcessId` and raise `AgentAttention`, which drives the follow-pane auto-tidy
/// (`tidy_on_agent_attention`) and the done-dot. The terminal bell only fires on
/// idle/permission, so it is not a reliable turn-end signal.
pub(super) fn handle_agent_turn_ended(
    mut reader: MessageReader<AgentCommandRequest>,
    agents: Query<(Entity, &vmux_service::protocol::ProcessId), With<vmux_core::team::Agent>>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
) {
    for request in reader.read() {
        let ServiceAgentCommand::TurnEnded { anchor } = &request.command else {
            continue;
        };
        if let Some((entity, _)) = agents.iter().find(|(_, pid)| *pid == anchor) {
            attention.write(vmux_core::notify::AgentAttention {
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
    use crate::events::CommandOrigin;

    pub(crate) fn bell_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_core::notify::BellReceived>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, agent_bell_to_attention);
        app
    }

    pub(crate) fn spawn_agent_with_pid(
        app: &mut App,
        pid: vmux_service::protocol::ProcessId,
    ) -> Entity {
        app.world_mut()
            .spawn((
                vmux_core::team::Agent {
                    sid: "s".to_string(),
                    kind: Some(vmux_core::agent::AgentKind::Claude),
                },
                pid,
            ))
            .id()
    }

    pub(crate) fn attentions(app: &App) -> Vec<Entity> {
        let messages = app
            .world()
            .resource::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|a| a.entity).collect()
    }

    pub(crate) fn turn_end_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AgentCommandRequest>()
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_systems(Update, handle_agent_turn_ended);
        app
    }

    pub(crate) fn send_turn_ended(app: &mut App, anchor: vmux_service::protocol::ProcessId) {
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: vmux_service::protocol::AgentRequestId::new(),
                origin: CommandOrigin::Agent {
                    sid: None,
                    anchor: Some(anchor),
                },
                command: ServiceAgentCommand::TurnEnded { anchor },
            });
    }

    #[test]
    pub(crate) fn turn_ended_resolves_to_agent_attention() {
        let mut app = turn_end_test_app();
        let pid = vmux_service::protocol::ProcessId::new();
        let agent = spawn_agent_with_pid(&mut app, pid);
        send_turn_ended(&mut app, pid);
        app.update();
        assert_eq!(attentions(&app), vec![agent]);
    }

    #[test]
    pub(crate) fn turn_ended_unknown_anchor_emits_nothing() {
        let mut app = turn_end_test_app();
        let _agent = spawn_agent_with_pid(&mut app, vmux_service::protocol::ProcessId::new());
        send_turn_ended(&mut app, vmux_service::protocol::ProcessId::new());
        app.update();
        assert!(attentions(&app).is_empty());
    }

    #[test]
    pub(crate) fn bell_resolves_to_agent_attention() {
        use vmux_service::protocol::ProcessId;
        let mut app = bell_test_app();
        let pid = ProcessId::new();
        let agent = spawn_agent_with_pid(&mut app, pid);
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::BellReceived>>()
            .write(vmux_core::notify::BellReceived { process_id: pid });
        app.update();
        assert_eq!(attentions(&app), vec![agent]);
    }

    #[test]
    pub(crate) fn bell_unknown_process_id_emits_nothing() {
        use vmux_service::protocol::ProcessId;
        let mut app = bell_test_app();
        let _agent = spawn_agent_with_pid(&mut app, ProcessId::new());
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::BellReceived>>()
            .write(vmux_core::notify::BellReceived {
                process_id: ProcessId::new(),
            });
        app.update();
        assert!(attentions(&app).is_empty());
    }

    pub(crate) fn done_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<vmux_core::notify::AgentAttention>()
            .add_message::<vmux_core::notify::OsNotify>()
            .init_resource::<vmux_layout::stack::FocusedStack>()
            .add_systems(Update, (mark_agent_done, clear_agent_done));
        app
    }

    pub(crate) fn spawn_agent_in_stack(app: &mut App) -> (Entity, Entity) {
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::Stack::default())
            .id();
        let agent = app
            .world_mut()
            .spawn((
                vmux_core::team::Profile::agent(vmux_core::agent::AgentKind::Claude),
                ChildOf(stack),
            ))
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
            .resource::<bevy::ecs::message::Messages<vmux_core::notify::OsNotify>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).count()
    }

    pub(crate) fn send_attention(app: &mut App, entity: Entity) {
        app.world_mut()
            .resource_mut::<bevy::ecs::message::Messages<vmux_core::notify::AgentAttention>>()
            .write(vmux_core::notify::AgentAttention {
                entity,
                title: None,
                body: None,
            });
    }

    #[test]
    pub(crate) fn done_notifies_and_marks_when_backgrounded() {
        let mut app = done_test_app();
        let (agent, _stack) = spawn_agent_in_stack(&mut app);
        set_window(&mut app, false);
        send_attention(&mut app, agent);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(agent)
                .is_some()
        );
        assert_eq!(os_notify_count(&app), 1);
    }

    #[test]
    pub(crate) fn focused_child_agent_does_not_notify_or_mark() {
        let mut app = done_test_app();
        let (agent, stack) = spawn_agent_in_stack(&mut app);
        set_window(&mut app, true);
        app.world_mut()
            .resource_mut::<vmux_layout::stack::FocusedStack>()
            .stack = Some(stack);
        app.update();
        send_attention(&mut app, agent);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(agent)
                .is_none(),
            "focused agent has no unseen marker"
        );
        assert_eq!(os_notify_count(&app), 0, "no banner when foreground");
    }

    #[test]
    pub(crate) fn focused_stack_agent_does_not_notify_or_mark() {
        let mut app = done_test_app();
        let stack = app
            .world_mut()
            .spawn((
                vmux_layout::stack::Stack::default(),
                vmux_core::team::Profile::agent(vmux_core::agent::AgentKind::Claude),
            ))
            .id();
        set_window(&mut app, true);
        app.world_mut()
            .resource_mut::<vmux_layout::stack::FocusedStack>()
            .stack = Some(stack);
        app.update();
        send_attention(&mut app, stack);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(stack)
                .is_none(),
            "focused stack agent has no unseen marker"
        );
        assert_eq!(os_notify_count(&app), 0, "no banner when foreground");
    }

    #[test]
    pub(crate) fn clear_removes_marker_from_focused_stack_agent() {
        let mut app = done_test_app();
        let stack = app
            .world_mut()
            .spawn(vmux_layout::stack::Stack::default())
            .id();
        set_window(&mut app, true);
        app.world_mut()
            .entity_mut(stack)
            .insert(vmux_core::notify::AgentDoneUnseen);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(stack)
                .is_some()
        );
        app.world_mut()
            .resource_mut::<vmux_layout::stack::FocusedStack>()
            .stack = Some(stack);
        app.update();
        assert!(
            app.world()
                .get::<vmux_core::notify::AgentDoneUnseen>(stack)
                .is_none()
        );
    }
}
