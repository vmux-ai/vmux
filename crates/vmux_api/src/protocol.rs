pub mod layout;
pub mod shared;
pub use layout::{
    Focus, LayoutIdParseError, LayoutNode, LayoutSnapshot, NodeKind, SplitDirection, Stack, Tab,
    format_id, parse_id,
};
pub use shared::{
    AgentRequest, SharedAgentCommand, SharedEvent, SharedFailure, SharedMessage, SharedResponse,
};

pub use crate::ProcessId;
pub use crate::json::JsonValue;

mod agent;
mod process;
mod query;
mod remote;
mod terminal;

pub use agent::*;
pub use process::*;
pub use query::*;
pub use remote::*;
pub use terminal::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::room::ClientOpId;

    #[test]
    fn deny_is_the_last_answer_offered_though_it_is_declared_second() {
        assert_eq!(
            ApprovalDecision::OFFERED,
            [
                ApprovalDecision::Allow,
                ApprovalDecision::AllowAlways,
                ApprovalDecision::Deny,
            ],
        );
        assert_eq!(ApprovalDecision::for_index(3), None);
    }

    #[test]
    fn composed_agent_prompt_preserves_marker_literals_in_display_text() {
        let display = format!("before{PRIVATE_CONTEXT_PROMPT_MARKER}after");
        let wire = compose_agent_prompt(&display, Some("context"));

        assert!(wire.contains("Context bytes: 7\ncontext"));
        assert_eq!(extract_display_prompt(&wire), Some(display.as_str()));
    }

    #[test]
    fn legacy_composed_agent_prompt_remains_decodable() {
        let wire = format!(
            "{PRIVATE_CONTEXT_PREFIX}\ncontext\n</vmux_handoff_context>{PRIVATE_CONTEXT_PROMPT_MARKER}display"
        );

        assert_eq!(extract_display_prompt(&wire), Some("display"));
    }

    #[test]
    fn embedded_private_context_is_split_from_visible_prompt() {
        let envelope = compose_agent_prompt("show me something fun", Some("host policy"));
        let echoed = format!("show me something fun{envelope}");

        assert_eq!(
            split_private_context_prompt(&echoed),
            Some(("host policy", "show me something fun"))
        );
        assert!(has_private_context_envelope(&echoed));
    }

    #[test]
    fn shared_message_variants_are_the_whole_remote_surface() {
        assert_eq!(
            SharedMessage::VARIANT_NAMES,
            ["Agent", "ListSessions", "AgentCommand"]
        );
        assert_eq!(
            AgentRequest::VARIANT_NAMES,
            ["Attach", "Input", "Cancel", "Approve", "ListMedia"]
        );
    }

    #[test]
    fn shared_agent_command_variants_are_the_whole_remote_surface() {
        assert_eq!(
            SharedAgentCommand::VARIANT_NAMES,
            [
                "NewAgentChat",
                "ListAgents",
                "ListTeam",
                "ListModels",
                "SelectModel",
                "SetEffort",
            ]
        );
    }

    #[test]
    fn shared_event_variants_are_the_whole_remote_surface() {
        assert_eq!(
            SharedEvent::VARIANT_NAMES,
            [
                "AgentDelta",
                "AgentRunStatusChanged",
                "AgentAwaitingApproval",
                "AgentApprovalResolved",
                "AgentMessagesSnapshot",
                "AcpAgentInfo",
                "AcpWorkspaceChanged",
                "AcpModelInfo",
                "Session",
            ]
        );
    }

    #[test]
    fn agent_request_id_roundtrips() {
        let request_id = AgentRequestId::new();
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&request_id).unwrap();
        let decoded = rkyv::from_bytes::<AgentRequestId, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(decoded, request_id);
    }

    #[test]
    fn agent_cancel_and_interrupted_roundtrip() {
        let msg = ClientMessage::Shared(SharedMessage::agent("s1", AgentRequest::Cancel));
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let back = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(
            matches!(back, ClientMessage::Shared(SharedMessage::Agent { sid, request: AgentRequest::Cancel }) if sid == "s1")
        );

        let st = AgentRunStatus::Interrupted;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&st).unwrap();
        let back = rkyv::from_bytes::<AgentRunStatus, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, AgentRunStatus::Interrupted);
    }

    #[test]
    fn acp_workspace_changed_roundtrips() {
        let message = ServiceMessage::Shared(SharedEvent::AcpWorkspaceChanged {
            sid: "s1".into(),
            name: "quiet-amber-wolf".into(),
            branch: "vibe/quiet-amber-wolf".into(),
            cwd: "/worktrees/quiet-amber-wolf".into(),
            workspace_cwd: "/repo".into(),
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&message).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();

        assert!(matches!(
            decoded,
            ServiceMessage::Shared(SharedEvent::AcpWorkspaceChanged {
                sid,
                name,
                branch,
                cwd,
                workspace_cwd,
            }) if sid == "s1"
                && name == "quiet-amber-wolf"
                && branch == "vibe/quiet-amber-wolf"
                && cwd == "/worktrees/quiet-amber-wolf"
                && workspace_cwd == "/repo"
        ));
    }

    #[test]
    fn empty_browser_navigate_url_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::BrowserNavigate {
                url: String::new(),
                pane: None,
            }),
            Err(AgentCommandValidationError::EmptyBrowserUrl)
        );
    }

    #[test]
    fn empty_agent_shell_command_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::RunShell {
                command: String::new(),
                cwd: String::new(),
                mode: AgentShellMode::NewTab,
            }),
            Err(AgentCommandValidationError::EmptyShellCommand)
        );
    }

    #[test]
    fn empty_terminal_send_text_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::TerminalSend {
                text: String::new(),
                terminal: None,
            }),
            Err(AgentCommandValidationError::EmptyTerminalText)
        );
    }

    #[test]
    fn new_agent_chat_requires_prompt_and_roundtrips() {
        assert_eq!(
            validate_agent_command(&AgentCommand::Shared(SharedAgentCommand::NewAgentChat {
                client_op_id: ClientOpId::new("op"),
                prompt: "  ".to_string(),
                agent_url: None,
            })),
            Err(AgentCommandValidationError::EmptyAgentPrompt)
        );
        let command = AgentCommand::Shared(SharedAgentCommand::NewAgentChat {
            client_op_id: ClientOpId::new("op"),
            prompt: "continue from my phone".to_string(),
            agent_url: Some("vmux://sessions/claude".to_string()),
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&command).unwrap();
        let back: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn empty_rename_profile_name_is_invalid() {
        assert_eq!(
            validate_agent_command(&AgentCommand::RenameProfile {
                name: "  ".to_string(),
            }),
            Err(AgentCommandValidationError::EmptyProfileName)
        );
    }

    #[test]
    fn agent_query_read_layout_rkyv_round_trip() {
        let q = AgentQuery::ReadLayout { anchor: None };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let recovered: AgentQuery =
            rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, AgentQuery::ReadLayout { anchor: None });
    }

    #[test]
    fn agent_query_working_directory_rkyv_round_trip() {
        let query = AgentQuery::WorkingDirectory {
            anchor: ProcessId::new(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&query).unwrap();
        let recovered: AgentQuery =
            rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(recovered, query);
    }

    #[test]
    fn agent_command_catalog_rkyv_round_trip() {
        let query = AgentQuery::ListCommands;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&query).unwrap();
        let recovered: AgentQuery =
            rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, query);

        let result = AgentQueryResult::Commands(vec![AgentCommandTool {
            name: "terminal_clear".to_string(),
            description: "Clear Terminal".to_string(),
            input_schema: JsonValue::Object(vec![(
                "type".to_string(),
                JsonValue::String("object".to_string()),
            )]),
        }]);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&result).unwrap();
        let recovered: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, result);
    }

    #[test]
    fn agent_query_screenshot_rkyv_round_trip() {
        let q = AgentQuery::Screenshot {
            pane: Some("pane:42".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);

        let none = AgentQuery::Screenshot { pane: None };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&none).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, none);
    }

    #[test]
    fn agent_query_simulator_control_rkyv_round_trip() {
        let query = AgentQuery::SimulatorControl {
            input: SimulatorInput::Swipe {
                start_x: 120,
                start_y: 700,
                end_x: 120,
                end_y: 200,
                duration_ms: 300,
            },
        };

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&query).unwrap();
        let recovered: AgentQuery =
            rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(recovered, query);
    }

    #[test]
    fn agent_query_result_image_rkyv_round_trip() {
        let r = AgentQueryResult::Image {
            path: "/tmp/x.png".into(),
            png: vec![1, 2, 3, 4],
            width: 320,
            height: 200,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let back: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn agent_query_result_vault_status_rkyv_round_trip() {
        let result = AgentQueryResult::VaultStatus(crate::vault::VaultStatusSnapshot {
            root: "/Users/test/.vmux".into(),
            connected: true,
            encrypted: true,
            unlocked: true,
            recovery_key: true,
            automatic_backup: true,
            provider: Some(crate::vault::VaultProvider::Github),
            remote: Some("https://github.com/vmux-ai/vault.git".into()),
            branch: "main".into(),
            local_changes: 2,
            ahead: 1,
            behind: 0,
            sync_needed: true,
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&result).unwrap();
        let recovered: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();

        assert_eq!(recovered, result);
    }

    #[test]
    fn agent_query_record_start_rkyv_round_trip() {
        let q = AgentQuery::RecordStart {
            gif: true,
            max_secs: 120,
            pane: Some("pane:7".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn agent_query_record_stop_rkyv_round_trip() {
        let q = AgentQuery::RecordStop {
            dir: Some("/tmp/out".into()),
            name: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let back: AgentQuery = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn agent_query_result_recording_rkyv_round_trip() {
        let r = AgentQueryResult::Recording {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: Some("/tmp/x.gif".into()),
            duration_ms: 7400,
            bytes: 1_234_567,
            auto_stopped: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let back: AgentQueryResult =
            rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn notify_command_rkyv_roundtrip() {
        let cmd = AgentCommand::Notify {
            title: Some("done".to_string()),
            body: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let back: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(cmd, back);
    }

    #[test]
    fn bell_service_message_rkyv_roundtrip() {
        let pid = ProcessId::new();
        let msg = ServiceMessage::Bell { process_id: pid };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let back: ServiceMessage =
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        match back {
            ServiceMessage::Bell { process_id } => assert_eq!(process_id, pid),
            _ => panic!("expected ServiceMessage::Bell"),
        }
    }

    #[test]
    fn open_beside_round_trips_and_validates() {
        let cmd = AgentCommand::OpenBeside {
            anchor: ProcessId::new(),
            direction: Some(AgentPaneDirection::Right),
            url: "vmux://terminal/".into(),
            focus: true,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let back: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, cmd);
        assert!(validate_agent_command(&cmd).is_ok());

        let empty = AgentCommand::OpenBeside {
            anchor: ProcessId::new(),
            direction: Some(AgentPaneDirection::Right),
            url: "  ".into(),
            focus: true,
        };
        assert!(validate_agent_command(&empty).is_err());
    }

    #[test]
    fn file_touched_round_trips_and_validates() {
        let cmd = AgentCommand::FileTouched {
            anchor: ProcessId::new(),
            path: "/abs/x.rs".into(),
            line: Some(42),
            col: Some(4),
            end_col: Some(12),
            kind: FileTouchKind::Edit,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let back: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(back, cmd);
        assert!(validate_agent_command(&cmd).is_ok());

        let empty = AgentCommand::FileTouched {
            anchor: ProcessId::new(),
            path: "  ".into(),
            line: None,
            col: None,
            end_col: None,
            kind: FileTouchKind::Read,
        };
        assert!(validate_agent_command(&empty).is_err());
    }

    #[test]
    fn agent_command_update_layout_rkyv_round_trip() {
        use crate::protocol::layout::{Focus, LayoutNode, LayoutSnapshot, Tab};
        let cmd = AgentCommand::UpdateLayout {
            layout: LayoutSnapshot {
                tabs: vec![Tab {
                    id: Some("tab:1".into()),
                    name: "X".into(),
                    is_active: true,
                    root: LayoutNode::Pane {
                        id: Some("pane:2".into()),
                        is_zoomed: false,
                        stacks: vec![],
                    },
                }],
                focused: Focus {
                    tab: Some("tab:1".into()),
                    pane: Some("pane:2".into()),
                    stack: None,
                },
            },
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let recovered: AgentCommand =
            rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, cmd);
    }

    #[test]
    fn agent_command_result_roundtrips() {
        for variant in [
            AgentCommandResult::Ok,
            AgentCommandResult::Error("boom".to_string()),
        ] {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&variant).unwrap();
            let decoded =
                rkyv::from_bytes::<AgentCommandResult, rkyv::rancor::Error>(&bytes).unwrap();
            assert_eq!(decoded, variant);
        }
    }

    #[test]
    fn agent_command_result_layout_rkyv_round_trip() {
        let result = AgentCommandResult::Layout(LayoutSnapshot {
            tabs: vec![Tab {
                id: Some("tab:1".into()),
                name: "X".into(),
                is_active: true,
                root: LayoutNode::Pane {
                    id: Some("pane:2".into()),
                    is_zoomed: false,
                    stacks: vec![Stack {
                        id: Some("stack:3".into()),
                        title: "T".into(),
                        url: "https://x".into(),
                        kind: "browser".into(),
                        is_loading: false,
                        icon: crate::PageIcon::None,
                        is_self: false,
                        process_id: None,
                    }],
                },
            }],
            focused: Focus {
                tab: Some("tab:1".into()),
                pane: Some("pane:2".into()),
                stack: Some("stack:3".into()),
            },
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&result).unwrap();
        let recovered: AgentCommandResult =
            rkyv::from_bytes::<AgentCommandResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, result);
    }

    #[test]
    fn agent_command_response_messages_roundtrip() {
        let request_id = AgentRequestId::new();
        let client_msg = ClientMessage::AgentCommandResponse {
            request_id,
            result: AgentCommandResult::Ok,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&client_msg).unwrap();
        let _decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();

        let service_msg = ServiceMessage::AgentCommandResult {
            request_id,
            result: AgentCommandResult::Error("nope".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&service_msg).unwrap();
        let _decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
    }

    #[test]
    fn browser_navigate_with_pane_roundtrips() {
        let cmd = AgentCommand::BrowserNavigate {
            url: "https://example.com".to_string(),
            pane: Some("12345".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn browser_navigate_without_pane_roundtrips() {
        let cmd = AgentCommand::BrowserNavigate {
            url: "https://example.com".to_string(),
            pane: None,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn terminal_send_with_terminal_roundtrips() {
        let cmd = AgentCommand::TerminalSend {
            text: "hi".to_string(),
            terminal: Some("67890".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn status_response_roundtrips() {
        let msg = ServiceMessage::StatusResponse {
            uptime_secs: 42,
            process_count: 3,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(matches!(
            decoded,
            ServiceMessage::StatusResponse {
                uptime_secs: 42,
                process_count: 3
            }
        ));
    }

    #[test]
    fn process_created_round_trips_pid() {
        let id = ProcessId::new();
        let msg = ServiceMessage::ProcessCreated {
            process_id: id,
            pid: 12345,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        match decoded {
            ServiceMessage::ProcessCreated { process_id, pid } => {
                assert_eq!(process_id, id);
                assert_eq!(pid, 12345);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn process_create_failed_round_trips_reason() {
        let id = ProcessId::new();
        let msg = ServiceMessage::ProcessCreateFailed {
            process_id: id,
            reason: "missing PID after spawn".into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        match decoded {
            ServiceMessage::ProcessCreateFailed { process_id, reason } => {
                assert_eq!(process_id, id);
                assert_eq!(reason, "missing PID after spawn");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn update_settings_command_rkyv_roundtrip() {
        let cmd = AgentCommand::UpdateSettings {
            path: "layout.pane.gap".to_string(),
            value: JsonValue::Number("12.0".to_string()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn run_with_placement_override_rkyv_roundtrip() {
        let cmd = AgentCommand::RunWithPlacementOverride {
            anchor: ProcessId::new(),
            command: "cargo test".into(),
            direction: AgentPaneDirection::Bottom,
            focus: false,
            beside: None,
            mode: PlacementMode::Split,
            terminal: None,
            done_marker: Some("token".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cmd).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, cmd);
    }

    #[test]
    fn get_settings_query_rkyv_roundtrip() {
        let q = AgentQuery::GetSettings;
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&q).unwrap();
        let decoded = rkyv::from_bytes::<AgentQuery, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, q);
    }

    #[test]
    fn settings_query_result_rkyv_roundtrip() {
        let r = AgentQueryResult::Settings(JsonValue::Object(vec![(
            "auto_update".to_string(),
            JsonValue::Bool(true),
        )]));
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&r).unwrap();
        let decoded = rkyv::from_bytes::<AgentQueryResult, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, r);
    }

    #[test]
    fn update_settings_validation_rejects_empty_path() {
        let cmd = AgentCommand::UpdateSettings {
            path: "".to_string(),
            value: JsonValue::Number("1".to_string()),
        };
        assert!(validate_agent_command(&cmd).is_err());
    }

    #[test]
    fn create_worktree_validation_rejects_empty_branch() {
        let cmd = AgentCommand::CreateWorktreeOnBranch {
            anchor: ProcessId::new(),
            branch: "  ".to_string(),
            project: None,
        };
        assert!(validate_agent_command(&cmd).is_err());
    }

    #[test]
    fn workspace_commands_rkyv_roundtrip() {
        let commands = [
            AgentCommand::ChooseWorkspace {
                anchor: ProcessId::new(),
            },
            AgentCommand::CreateWorktreeOnBranch {
                anchor: ProcessId::new(),
                branch: "feature/fun-terminal".into(),
                project: None,
            },
            AgentCommand::RequestUserChoice {
                anchor: ProcessId::new(),
                question: "Repository?".into(),
                options: vec!["Local".into(), "Remote".into(), "Create".into()],
            },
            AgentCommand::ChooseWorkspaceAtPath {
                anchor: ProcessId::new(),
                path: "/repo".into(),
            },
            AgentCommand::PrepareWorktree {
                anchor: ProcessId::new(),
                path: Some("/repo-wt".into()),
                task: Some("feature".into()),
                create: false,
            },
        ];
        for command in commands {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&command).unwrap();
            let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
            assert_eq!(decoded, command);
        }
    }

    #[test]
    fn write_knowledge_rkyv_roundtrip() {
        let command = AgentCommand::WriteKnowledge {
            anchor: ProcessId::new(),
            path: Some("projects/yc.md".into()),
            title: "YC".into(),
            content: "Notes".into(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&command).unwrap();
        let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded, command);
    }

    #[test]
    fn knowledge_read_commands_roundtrip_and_validate() {
        let commands = [
            AgentCommand::SearchKnowledge {
                anchor: ProcessId::new(),
                query: "Obsidian links".into(),
                limit: 20,
            },
            AgentCommand::ReadKnowledge {
                anchor: ProcessId::new(),
                path: "projects/obsidian-gap-analysis.md".into(),
                line: 1,
                limit: 200,
            },
        ];
        for command in commands {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&command).unwrap();
            let decoded = rkyv::from_bytes::<AgentCommand, rkyv::rancor::Error>(&bytes).unwrap();
            assert_eq!(decoded, command);
            assert!(validate_agent_command(&command).is_ok());
        }
    }

    #[test]
    fn page_agent_client_messages_roundtrip() {
        let messages = [
            ClientMessage::SpawnPageAgent {
                sid: "s".into(),
                provider: "anthropic".into(),
                model: "m".into(),
                cwd: "/tmp".into(),
                auto_tools: vec!["list_spaces".into()],
                tools_json: "[]".into(),
            },
            ClientMessage::Shared(SharedMessage::agent("s", AgentRequest::Attach)),
            ClientMessage::DetachPageAgent { sid: "s".into() },
            ClientMessage::Shared(SharedMessage::agent(
                "s",
                AgentRequest::Input {
                    text: "hi".into(),
                    context: Some("prior conversation".into()),
                    attachments: Vec::new(),
                    preferred_mode: None,
                },
            )),
            ClientMessage::Shared(SharedMessage::agent(
                "s",
                AgentRequest::Input {
                    text: "inspect".into(),
                    context: None,
                    attachments: vec![AgentAttachment {
                        path: "/tmp/image.png".into(),
                        name: "image.png".into(),
                        mime_type: "image/png".into(),
                        size: 42,
                    }],
                    preferred_mode: Some("auto".into()),
                },
            )),
            ClientMessage::AcpSetModel {
                sid: "s".into(),
                request_id: 7,
                config_id: "model".into(),
                model_id: "sonnet".into(),
            },
            ClientMessage::Shared(SharedMessage::agent(
                "s",
                AgentRequest::Approve {
                    call_id: "c".into(),
                    decision: ApprovalDecision::Allow,
                },
            )),
            ClientMessage::Shared(SharedMessage::agent(
                "s",
                AgentRequest::Approve {
                    call_id: "ca".into(),
                    decision: ApprovalDecision::AllowAlways,
                },
            )),
            ClientMessage::ClosePageAgent { sid: "s".into() },
            ClientMessage::AgentToolResult {
                request_id: AgentRequestId::new(),
                content: "ok".into(),
                is_error: false,
            },
            ClientMessage::RebindAcpWorkspace {
                sid: "s".into(),
                cwd: "/tmp/worktree".into(),
            },
        ];
        for msg in messages {
            let expects_allow_always = matches!(
                &msg,
                ClientMessage::Shared(SharedMessage::Agent {
                    request: AgentRequest::Approve {
                        decision: ApprovalDecision::AllowAlways,
                        ..
                    },
                    ..
                })
            );
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
            let decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();
            if expects_allow_always {
                assert!(matches!(
                    decoded,
                    ClientMessage::Shared(SharedMessage::Agent {
                        request: AgentRequest::Approve {
                            decision: ApprovalDecision::AllowAlways,
                            ..
                        },
                        ..
                    })
                ));
            }
        }
    }

    #[test]
    fn the_prompt_builder_addresses_the_session_and_keeps_attachments() {
        assert!(matches!(
            ClientMessage::agent_input("s".into(), "hi".into(), None, Vec::new()),
            ClientMessage::Shared(SharedMessage::Agent {
                sid,
                request: AgentRequest::Input { text, context, attachments, preferred_mode },
            }) if sid == "s" && text == "hi" && context.is_none() && attachments.is_empty() && preferred_mode.is_none()
        ));
        assert!(matches!(
            ClientMessage::agent_input(
                "s".into(),
                "inspect".into(),
                None,
                vec![AgentAttachment {
                    path: "/tmp/image.png".into(),
                    name: "image.png".into(),
                    mime_type: "image/png".into(),
                    size: 42,
                }],
            ),
            ClientMessage::Shared(SharedMessage::Agent {
                request: AgentRequest::Input { attachments, .. },
                ..
            }) if attachments.len() == 1
        ));
    }

    #[test]
    fn acp_protocol_messages_roundtrip() {
        let client = ClientMessage::SpawnAcpAgent {
            sid: "s1".into(),
            agent_id: "vibe-acp".into(),
            command: "uv".into(),
            args: vec!["run".into()],
            env: vec![("K".into(), "V".into())],
            cwd: "/tmp".into(),
            anchor: ProcessId::new(),
            mcp_command: Some("vmux".into()),
            mcp_args: vec!["mcp".into(), "--anchor".into()],
            resume_acp_session_id: Some("prev-session".into()),
            managed_mcp_servers: vec![ManagedMcpServer {
                name: "docs".into(),
                transport: ManagedMcpTransport::Http,
                command: None,
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
                url: Some("https://example.com/mcp".into()),
                headers: Vec::new(),
            }],
            effort: Some("high".into()),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&client).unwrap();
        let decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();
        let ClientMessage::SpawnAcpAgent {
            managed_mcp_servers,
            ..
        } = decoded
        else {
            panic!("expected SpawnAcpAgent");
        };
        assert_eq!(
            managed_mcp_servers,
            vec![ManagedMcpServer {
                name: "docs".into(),
                transport: ManagedMcpTransport::Http,
                command: None,
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
                url: Some("https://example.com/mcp".into()),
                headers: Vec::new(),
            }]
        );

        let services = [
            ServiceMessage::AcpTerminalCreated {
                sid: "s".into(),
                terminal_id: "t".into(),
                process_id: ProcessId::new(),
                command: "ls".into(),
                args: vec![],
                cwd: None,
            },
            ServiceMessage::AcpProposedDiff {
                sid: "s".into(),
                call_id: "c".into(),
                path: "/tmp/a.rs".into(),
                old_text: Some("a".into()),
                new_text: "b".into(),
            },
            ServiceMessage::AcpSessionCreated {
                sid: "s".into(),
                acp_session_id: "acp-1".into(),
            },
            ServiceMessage::Shared(SharedEvent::AcpAgentInfo {
                sid: "s".into(),
                name: "Antigravity".into(),
            }),
            ServiceMessage::Shared(SharedEvent::AcpModelInfo {
                sid: "s".into(),
                config_id: "model".into(),
                current_model_id: "sonnet".into(),
                models: vec![AcpModelOption {
                    id: "sonnet".into(),
                    name: "Claude Sonnet".into(),
                    description: Some("Balanced".into()),
                }],
            }),
            ServiceMessage::AcpModelSelectionResult {
                sid: "s".into(),
                request_id: 7,
                model_id: "opus".into(),
                succeeded: false,
            },
        ];
        for msg in services {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        }
    }

    #[test]
    fn page_agent_service_messages_roundtrip() {
        let messages = [
            ServiceMessage::Shared(SharedEvent::AgentDelta {
                sid: "s".into(),
                text: "hello".into(),
            }),
            ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged {
                sid: "s".into(),
                status: AgentRunStatus::Streaming,
            }),
            ServiceMessage::Shared(SharedEvent::AgentRunStatusChanged {
                sid: "s".into(),
                status: AgentRunStatus::Errored("boom".into()),
            }),
            ServiceMessage::Shared(SharedEvent::AgentAwaitingApproval {
                sid: "s".into(),
                call_id: "c".into(),
                name: "n".into(),
                args: crate::json::JsonValue::Object(Vec::new()),
            }),
            ServiceMessage::Shared(SharedEvent::AgentApprovalResolved {
                sid: "s".into(),
                call_id: "c".into(),
            }),
            ServiceMessage::AgentToolCall {
                request_id: AgentRequestId::new(),
                sid: "s".into(),
                name: "n".into(),
                args: JsonValue::Object(Vec::new()),
            },
            ServiceMessage::Shared(SharedEvent::AgentMessagesSnapshot {
                sid: "s".into(),
                messages: Vec::new(),
            }),
        ];
        for msg in messages {
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        }
    }
}
