use bevy::prelude::*;
use futures_lite::future;

use crate::components::AgentMessages;
use crate::events::{AgentToolStatus, ToolStatus};
use crate::message::Message;
use crate::run_state::{AgentRunState, ToolDispatchOutput};

pub fn dispatch_tool(
    mut commands: Commands,
    mut q: Query<(Entity, &mut AgentRunState, &mut AgentMessages)>,
) {
    for (entity, mut state, mut messages) in &mut q {
        let output_opt = match &mut *state {
            AgentRunState::RunningTool { task, .. } => future::block_on(future::poll_once(task)),
            _ => continue,
        };
        let Some(output) = output_opt else {
            continue;
        };
        let ToolDispatchOutput {
            call_id,
            content,
            is_error,
        } = output;
        messages.0.push(Message::ToolResult {
            call_id: call_id.clone(),
            content: content.clone(),
            is_error,
        });
        commands.trigger(AgentToolStatus {
            session: entity,
            call_id,
            status: ToolStatus::Result { content, is_error },
        });
        *state = AgentRunState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::IoTaskPool;

    fn make_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        app.add_systems(Update, dispatch_tool);
        app
    }

    #[test]
    fn completed_tool_appends_result_and_idles() {
        let mut app = make_app();
        let task = IoTaskPool::get().spawn(async {
            ToolDispatchOutput {
                call_id: "abc".into(),
                content: "ok".into(),
                is_error: false,
            }
        });
        let entity = app
            .world_mut()
            .spawn((
                AgentMessages::default(),
                AgentRunState::RunningTool {
                    call_id: "abc".into(),
                    task,
                },
            ))
            .id();

        for _ in 0..10 {
            app.update();
            if matches!(
                app.world().get::<AgentRunState>(entity),
                Some(AgentRunState::Idle)
            ) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let world = app.world();
        let msgs = world.get::<AgentMessages>(entity).unwrap();
        assert_eq!(msgs.0.len(), 1);
        match &msgs.0[0] {
            Message::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                assert_eq!(call_id, "abc");
                assert_eq!(content, "ok");
                assert!(!is_error);
            }
            _ => panic!("expected tool result"),
        }
    }
}
