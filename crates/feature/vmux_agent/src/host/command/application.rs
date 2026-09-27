use bevy::prelude::*;
use vmux_api::protocol::{
    AgentCommandResult, AgentFocusPane, AgentNotify, AgentRenameProfile, AgentUpdateLayout,
    AgentUpdateSettings,
};
use vmux_layout::stack::FocusedStack;
use vmux_service::client::ServiceRequest;
use vmux_setting::AppSettings;
use vmux_space::ActiveSpace;

use crate::host::event::CommandOrigin;

use super::{AgentReply, CommandSet};

pub(super) struct ApplicationCommandPlugin;

impl Plugin for ApplicationCommandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentNotifyRequest>()
            .add_message::<AgentFocusPaneRequest>()
            .add_message::<AgentRenameProfileRequest>()
            .add_message::<AgentUpdateSettingsRequest>()
            .add_message::<AgentUpdateLayoutRequest>()
            .add_systems(
                Update,
                (
                    notify,
                    request_focus,
                    request_profile_rename,
                    update_settings,
                    update_layout,
                )
                    .in_set(CommandSet::Commands),
            )
            .add_systems(
                Update,
                (
                    focus_pane.after(request_focus),
                    rename_profile.after(request_profile_rename),
                )
                    .in_set(CommandSet::Commands),
            );
    }
}

#[derive(Message, Clone)]
pub(super) struct AgentNotifyRequest {
    pub(super) reply: AgentReply,
    pub(super) origin: CommandOrigin,
    pub(super) payload: AgentNotify,
}

#[derive(Message, Clone)]
pub(super) struct AgentFocusPaneRequest {
    pub(super) reply: AgentReply,
    pub(super) allowed: bool,
    pub(super) payload: AgentFocusPane,
}

#[derive(Message, Clone)]
pub(super) struct AgentRenameProfileRequest {
    pub(super) reply: AgentReply,
    pub(super) payload: AgentRenameProfile,
}

#[derive(Message, Clone)]
pub(super) struct AgentUpdateSettingsRequest {
    pub(super) reply: AgentReply,
    pub(super) from_agent: bool,
    pub(super) payload: AgentUpdateSettings,
}

#[derive(Message, Clone)]
pub(super) struct AgentUpdateLayoutRequest {
    pub(super) reply: AgentReply,
    pub(super) from_agent: bool,
    pub(super) payload: AgentUpdateLayout,
}

#[derive(Message, Clone)]
pub(crate) struct FocusPaneRequest {
    pane: String,
}

#[derive(Message, Clone)]
pub(crate) struct RenameProfileRequest {
    name: String,
}

#[derive(Clone, Copy)]
struct CurrentFocus {
    tab: Option<Entity>,
    pane: Option<Entity>,
    stack: Option<Entity>,
}

impl CurrentFocus {
    fn of(focus: &FocusedStack) -> Self {
        Self {
            tab: focus.tab,
            pane: focus.pane,
            stack: focus.stack,
        }
    }

    fn preserve_in(self, snapshot: &mut vmux_api::protocol::layout::LayoutSnapshot) {
        snapshot.focused = vmux_api::protocol::layout::Focus {
            tab: self.id(vmux_layout::protocol::NodeKind::Tab, self.tab),
            pane: self.id(vmux_layout::protocol::NodeKind::Pane, self.pane),
            stack: self.id(vmux_layout::protocol::NodeKind::Stack, self.stack),
        };
        if let Some(tab) = snapshot.focused.tab.as_deref() {
            for item in &mut snapshot.tabs {
                item.is_active = item.id.as_deref() == Some(tab);
            }
        }
    }

    fn id(self, kind: vmux_layout::protocol::NodeKind, entity: Option<Entity>) -> Option<String> {
        entity.map(|entity| vmux_layout::protocol::format_id(kind, entity.to_bits()))
    }
}

fn notify(
    mut requests: MessageReader<AgentNotifyRequest>,
    agents: Query<(
        Entity,
        &vmux_core::team::Agent,
        Option<&vmux_api::protocol::ProcessId>,
    )>,
    user: Query<Entity, With<vmux_core::team::User>>,
    mut attention: MessageWriter<vmux_core::notify::AgentAttention>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let caller = match &request.origin {
            CommandOrigin::Agent {
                anchor: Some(pid), ..
            } => agents
                .iter()
                .find(|(_, _, process)| process.is_some_and(|process| process == pid))
                .map(|(entity, _, _)| entity),
            CommandOrigin::Agent { sid: Some(sid), .. } if !sid.is_empty() => agents
                .iter()
                .find(|(_, agent, _)| &agent.sid == sid)
                .map(|(entity, _, _)| entity),
            CommandOrigin::User => user.single().ok(),
            _ => None,
        };
        let result = match caller {
            Some(caller) => {
                attention.write(vmux_core::notify::AgentAttention {
                    entity: caller,
                    title: request.payload.title.clone(),
                    body: request.payload.body.clone(),
                });
                AgentCommandResult::Ok
            }
            None => AgentCommandResult::Error("notify: caller not found".to_string()),
        };
        responses.write(request.reply.response(result));
    }
}

fn request_focus(
    mut requests: MessageReader<AgentFocusPaneRequest>,
    mut focus: MessageWriter<FocusPaneRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let result = if request.allowed {
            focus.write(FocusPaneRequest {
                pane: request.payload.pane.clone(),
            });
            AgentCommandResult::Ok
        } else {
            AgentCommandResult::Error("focus_pane is disabled for agents".to_string())
        };
        responses.write(request.reply.response(result));
    }
}

fn focus_pane(
    mut requests: MessageReader<FocusPaneRequest>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((_, bits)) = vmux_layout::protocol::parse_id(&request.pane) else {
            continue;
        };
        vmux_core::focus_pane_entity(Entity::from_bits(bits), &mut commands, &child_of);
    }
}

fn request_profile_rename(
    mut requests: MessageReader<AgentRenameProfileRequest>,
    mut rename: MessageWriter<RenameProfileRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        rename.write(RenameProfileRequest {
            name: request.payload.name.clone(),
        });
        responses.write(request.reply.ok());
    }
}

fn rename_profile(
    mut requests: MessageReader<RenameProfileRequest>,
    active_space: Option<ResMut<ActiveSpace>>,
) {
    let Some(mut active) = active_space else {
        return;
    };
    for request in requests.read() {
        let name = request.name.trim();
        if name.is_empty() {
            continue;
        }
        match vmux_core::profile::set_display_name(name) {
            Ok(()) => active.record.profile = name.to_string(),
            Err(error) => warn!("rename_profile: failed to persist display name: {error}"),
        }
    }
}

fn update_settings(
    mut requests: MessageReader<AgentUpdateSettingsRequest>,
    mut settings: ResMut<AppSettings>,
    mut write: MessageWriter<vmux_setting::SettingsWriteRequest>,
    mut responses: MessageWriter<ServiceRequest>,
) {
    for request in requests.read() {
        let result = match serde_json::Value::try_from(&request.payload.value) {
            Ok(value) => {
                let mut updated = (*settings).clone();
                match updated.apply_update(&request.payload.path, value) {
                    Ok(ron_bytes) => {
                        if request.from_agent
                            && updated.agent.allow_run_placement_override
                                != settings.agent.allow_run_placement_override
                        {
                            AgentCommandResult::Error(
                                "update_settings: agent.allow_run_placement_override can only be changed in Settings"
                                    .to_string(),
                            )
                        } else {
                            *settings = updated;
                            write.write(vmux_setting::SettingsWriteRequest { ron_bytes });
                            AgentCommandResult::Ok
                        }
                    }
                    Err(message) => AgentCommandResult::Error(message),
                }
            }
            Err(error) => {
                AgentCommandResult::Error(format!("update_settings: invalid JSON value: {error}"))
            }
        };
        responses.write(request.reply.response(result));
    }
}

fn update_layout(
    mut requests: MessageReader<AgentUpdateLayoutRequest>,
    focus: Res<FocusedStack>,
    mut apply: MessageWriter<vmux_layout::apply::LayoutApplyRequest>,
) {
    for request in requests.read() {
        let mut snapshot = request.payload.layout.clone();
        if request.from_agent {
            CurrentFocus::of(&focus).preserve_in(&mut snapshot);
        }
        apply.write(vmux_layout::apply::LayoutApplyRequest {
            request_id: request.reply.request_id.0,
            snapshot,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::AgentSessionPlugin;
    use crate::host::event::{AgentCommandRequest, CommandOrigin};
    use crate::host::test_support::test_settings;
    use vmux_api::protocol::{AgentCommand, AgentRequestId};

    #[test]
    fn update_settings_via_apply_mutates_resource_and_returns_ron() {
        let mut settings = test_settings();
        let ron_bytes = settings
            .apply_update(
                "browser.startup_url",
                serde_json::json!("https://example.com/custom"),
            )
            .expect("apply ok");
        assert_eq!(settings.browser.startup_url, "https://example.com/custom");
        assert!(ron_bytes.contains("https://example.com/custom"));
    }

    #[test]
    fn run_placement_override_settings_update_rejects_agents_and_allows_users() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            vmux_command::CommandPlugin,
            AgentSessionPlugin,
        ))
        .add_message::<vmux_setting::SettingsWriteRequest>()
        .add_message::<vmux_space::SpaceCreateRequest>()
        .add_message::<vmux_space::SpaceRenameRequest>()
        .add_message::<vmux_space::SpaceDeleteRequest>()
        .add_message::<vmux_history::query::HistoryOpenIntent>()
        .insert_resource(FocusedStack::default())
        .insert_resource(test_settings());

        let mut agent_value = serde_json::to_value(vmux_setting::AgentSettings::default()).unwrap();
        agent_value["allow_run_placement_override"] = serde_json::json!(true);
        for (path, value) in [
            (
                "agent.allow_run_placement_override",
                vmux_api::json::JsonValue::Bool(true),
            ),
            ("agent", vmux_api::json::JsonValue::from(agent_value)),
        ] {
            app.world_mut()
                .resource_mut::<Messages<AgentCommandRequest>>()
                .write(AgentCommandRequest {
                    request_id: AgentRequestId::new(),
                    origin: CommandOrigin::Agent {
                        sid: Some("test-agent".to_string()),
                        anchor: None,
                    },
                    command: AgentCommand::UpdateSettings(AgentUpdateSettings {
                        path: path.to_string(),
                        value,
                    }),
                });
            app.update();
            assert!(
                !app.world()
                    .resource::<AppSettings>()
                    .agent
                    .allow_run_placement_override,
                "agent update unexpectedly enabled override through {path}"
            );
        }

        app.world_mut()
            .resource_mut::<Messages<AgentCommandRequest>>()
            .write(AgentCommandRequest {
                request_id: AgentRequestId::new(),
                origin: CommandOrigin::User,
                command: AgentCommand::UpdateSettings(AgentUpdateSettings {
                    path: "agent.allow_run_placement_override".to_string(),
                    value: vmux_api::json::JsonValue::Bool(true),
                }),
            });
        app.update();
        assert!(
            app.world()
                .resource::<AppSettings>()
                .agent
                .allow_run_placement_override
        );
    }

    #[test]
    fn agent_layout_snapshot_keeps_current_focus() {
        use vmux_api::protocol::layout::{Focus, LayoutNode, LayoutSnapshot, Tab};
        let mut snapshot = LayoutSnapshot {
            tabs: vec![
                Tab {
                    id: Some("tab:9".into()),
                    name: "Agent".into(),
                    is_active: true,
                    root: LayoutNode::Pane {
                        id: Some("pane:8".into()),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
                Tab {
                    id: Some("tab:1".into()),
                    name: "User".into(),
                    is_active: false,
                    root: LayoutNode::Pane {
                        id: Some("pane:2".into()),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                },
            ],
            focused: Focus {
                tab: Some("tab:9".into()),
                pane: Some("pane:8".into()),
                stack: None,
            },
        };
        let focus = FocusedStack {
            tab: Some(Entity::from_bits(1)),
            pane: Some(Entity::from_bits(2)),
            stack: Some(Entity::from_bits(3)),
        };

        CurrentFocus::of(&focus).preserve_in(&mut snapshot);

        assert_eq!(snapshot.focused.tab.as_deref(), Some("tab:1"));
        assert_eq!(snapshot.focused.pane.as_deref(), Some("pane:2"));
        assert_eq!(snapshot.focused.stack.as_deref(), Some("stack:3"));
        assert!(!snapshot.tabs[0].is_active);
        assert!(snapshot.tabs[1].is_active);
    }
}
