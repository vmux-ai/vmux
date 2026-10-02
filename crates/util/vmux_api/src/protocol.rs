pub use crate::ProcessId;
pub use crate::json::JsonValue;
pub use agent::*;
pub use layout::{
    Focus, LayoutIdParseError, LayoutNode, LayoutSnapshot, NodeKind, SplitDirection, Stack, Tab,
};
pub use process::*;
pub use query::*;
pub use remote::*;
pub use shared::{SharedEvent, SharedFailure, SharedMessage, SharedResponse};
pub use terminal::*;

pub mod layout;
pub mod shared;

mod agent;
mod process;
mod query;
mod remote;
mod terminal;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composed_agent_prompt_preserves_marker_literals_in_display_text() {
        let display = "before\n\nCurrent user prompt:\nafter".to_string();
        let wire = AgentPromptEnvelope::compose(&display, Some("context"));

        assert!(wire.contains("Context bytes: 7\ncontext"));
        assert_eq!(
            AgentPromptEnvelope::new(&wire).display(),
            Some(display.as_str())
        );
    }

    #[test]
    fn legacy_composed_agent_prompt_remains_decodable() {
        let wire = "<vmux_handoff_context>\ncontext\n</vmux_handoff_context>\n\nCurrent user prompt:\ndisplay";

        assert_eq!(AgentPromptEnvelope::new(wire).display(), Some("display"));
    }

    #[test]
    fn embedded_private_context_is_split_from_visible_prompt() {
        let envelope = AgentPromptEnvelope::compose("show me something fun", Some("host policy"));
        let echoed = format!("show me something fun{envelope}");

        assert_eq!(
            AgentPromptEnvelope::new(&echoed).split(),
            Some(("host policy", "show me something fun"))
        );
        assert!(AgentPromptEnvelope::new(&echoed).has_private_context());
    }

    #[test]
    fn shared_message_variants_are_the_whole_remote_surface() {
        assert_eq!(
            SharedMessage::VARIANT_NAMES,
            [
                "AgentAttach",
                "AgentInput",
                "AgentCancel",
                "AgentApprove",
                "AgentListMedia",
                "ListSessions",
                "AgentNewChat",
                "AgentListAgents",
                "AgentListTeam",
                "AgentListModels",
                "AgentSelectModel",
                "AgentSetEffort",
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
        let msg = ClientMessage::Shared(SharedMessage::AgentCancel { sid: "s1".into() });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
        let back = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(
            matches!(back, ClientMessage::Shared(SharedMessage::AgentCancel { sid }) if sid == "s1")
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
    fn agent_command_catalog_rkyv_round_trip() {
        let request = AgentRequest::encode(&AgentListCommands).unwrap();
        assert_eq!(
            request.decode::<AgentListCommands>().unwrap(),
            Some(AgentListCommands)
        );

        let request_id = AgentRequestId::new();
        let commands = vec![AgentCommandTool {
            name: "terminal_clear".to_string(),
            description: "Clear Terminal".to_string(),
            input_schema: JsonValue::Object(vec![(
                "type".to_string(),
                JsonValue::String("object".to_string()),
            )]),
        }];
        let response = ServiceMessage::AgentQueryResult(AgentQueryResult {
            request_id,
            content: serde_json::to_string(&commands).unwrap(),
            is_error: false,
            image: None,
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&response).unwrap();
        let recovered: ServiceMessage =
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        let ServiceMessage::AgentQueryResult(result) = recovered else {
            panic!("unexpected response");
        };
        assert_eq!(result.request_id, request_id);
        let commands = serde_json::from_str::<Vec<AgentCommandTool>>(&result.content).unwrap();
        assert_eq!(commands[0].name, "terminal_clear");
    }

    #[test]
    fn agent_image_rkyv_round_trip() {
        let image = AgentImage {
            path: "/tmp/x.png".into(),
            png: vec![1, 2, 3, 4],
            width: 320,
            height: 200,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&image).unwrap();
        let recovered: AgentImage =
            rkyv::from_bytes::<AgentImage, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, image);
    }

    #[test]
    fn generic_agent_query_result_round_trips() {
        let request_id = AgentRequestId::new();
        let response = ServiceMessage::AgentQueryResult(AgentQueryResult {
            request_id,
            content: "saved /tmp/frame.png (320×200)".to_string(),
            is_error: false,
            image: Some(AgentImage {
                path: "/tmp/frame.png".to_string(),
                png: vec![1, 2, 3, 4],
                width: 320,
                height: 200,
            }),
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&response).unwrap();
        let recovered: ServiceMessage =
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        let ServiceMessage::AgentQueryResult(result) = recovered else {
            panic!("unexpected response");
        };

        assert_eq!(result.request_id, request_id);
        assert_eq!(result.content, "saved /tmp/frame.png (320×200)");
        assert_eq!(result.image.unwrap().width, 320);
    }

    #[test]
    fn agent_vault_status_result_rkyv_round_trip() {
        let request_id = AgentRequestId::new();
        let snapshot = crate::vault::VaultStatusSnapshot {
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
        };
        let response = ServiceMessage::AgentQueryResult(AgentQueryResult {
            request_id,
            content: serde_json::to_string_pretty(&snapshot).unwrap(),
            is_error: false,
            image: None,
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&response).unwrap();
        let recovered: ServiceMessage =
            rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        let ServiceMessage::AgentQueryResult(result) = recovered else {
            panic!("unexpected response");
        };
        assert_eq!(result.request_id, request_id);
        let snapshot =
            serde_json::from_str::<crate::vault::VaultStatusSnapshot>(&result.content).unwrap();
        assert_eq!(snapshot.root, "/Users/test/.vmux");
    }

    #[test]
    fn agent_recording_rkyv_round_trip() {
        let recording = AgentRecording {
            mp4_path: "/tmp/x.mp4".into(),
            gif_path: Some("/tmp/x.gif".into()),
            duration_ms: 7400,
            bytes: 1_234_567,
            auto_stopped: false,
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&recording).unwrap();
        let recovered: AgentRecording =
            rkyv::from_bytes::<AgentRecording, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(recovered, recording);
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
    fn settings_query_result_rkyv_roundtrip() {
        let request_id = AgentRequestId::new();
        let settings = JsonValue::Object(vec![("auto_update".to_string(), JsonValue::Bool(true))]);
        let value = serde_json::Value::try_from(&settings).unwrap();
        let response = ServiceMessage::AgentQueryResult(AgentQueryResult {
            request_id,
            content: serde_json::to_string(&value).unwrap(),
            is_error: false,
            image: None,
        });
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&response).unwrap();
        let decoded = rkyv::from_bytes::<ServiceMessage, rkyv::rancor::Error>(&bytes).unwrap();
        let ServiceMessage::AgentQueryResult(result) = decoded else {
            panic!("unexpected response");
        };
        assert_eq!(result.request_id, request_id);
        assert_eq!(result.content, "{\"auto_update\":true}");
    }

    #[test]
    fn agent_session_client_messages_roundtrip() {
        let messages = [
            ClientMessage::Shared(SharedMessage::AgentAttach { sid: "s".into() }),
            ClientMessage::DetachAgentSession { sid: "s".into() },
            ClientMessage::Shared(SharedMessage::AgentInput {
                sid: "s".into(),
                text: "hi".into(),
                context: Some("prior conversation".into()),
                attachments: Vec::new(),
                preferred_mode: None,
            }),
            ClientMessage::Shared(SharedMessage::AgentInput {
                sid: "s".into(),
                text: "inspect".into(),
                context: None,
                attachments: vec![AgentAttachment {
                    path: "/tmp/image.png".into(),
                    name: "image.png".into(),
                    mime_type: "image/png".into(),
                    size: 42,
                }],
                preferred_mode: Some("auto".into()),
            }),
            ClientMessage::AcpSetSessionConfig {
                sid: "s".into(),
                request_id: 7,
                config_id: Some("model".into()),
                value: "sonnet".into(),
            },
            ClientMessage::Shared(SharedMessage::AgentApprove {
                sid: "s".into(),
                call_id: "c".into(),
                decision: ApprovalDecision::Allow,
            }),
            ClientMessage::Shared(SharedMessage::AgentApprove {
                sid: "s".into(),
                call_id: "ca".into(),
                decision: ApprovalDecision::AllowAlways,
            }),
            ClientMessage::CloseAgentSession { sid: "s".into() },
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
                ClientMessage::Shared(SharedMessage::AgentApprove {
                    decision: ApprovalDecision::AllowAlways,
                    ..
                })
            );
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&msg).unwrap();
            let decoded = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&bytes).unwrap();
            if expects_allow_always {
                assert!(matches!(
                    decoded,
                    ClientMessage::Shared(SharedMessage::AgentApprove {
                        decision: ApprovalDecision::AllowAlways,
                        ..
                    })
                ));
            }
        }
    }

    #[test]
    fn agent_input_envelope_addresses_the_session_and_keeps_attachments() {
        assert!(matches!(
            ClientMessage::from(SharedMessage::AgentInput {
                sid: "s".into(),
                text: "hi".into(),
                context: None,
                attachments: Vec::new(),
                preferred_mode: None,
            }),
            ClientMessage::Shared(SharedMessage::AgentInput {
                sid,
                text,
                context,
                attachments,
                preferred_mode,
            }) if sid == "s" && text == "hi" && context.is_none() && attachments.is_empty() && preferred_mode.is_none()
        ));
        assert!(matches!(
            ClientMessage::from(SharedMessage::AgentInput {
                sid: "s".into(),
                text: "inspect".into(),
                context: None,
                attachments: vec![AgentAttachment {
                    path: "/tmp/image.png".into(),
                    name: "image.png".into(),
                    mime_type: "image/png".into(),
                    size: 42,
                }],
                preferred_mode: None,
            }),
            ClientMessage::Shared(SharedMessage::AgentInput { attachments, .. }) if attachments.len() == 1
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
            ServiceMessage::AcpSessionConfigState {
                sid: "s".into(),
                configs: vec![AcpSessionConfig {
                    config_id: Some("model".into()),
                    name: "Model".into(),
                    description: None,
                    category: Some("model".into()),
                    current_value: "sonnet".into(),
                    values: vec![AcpSessionConfigValue {
                        value: "sonnet".into(),
                        name: "Claude Sonnet".into(),
                        description: Some("Balanced".into()),
                        group: None,
                    }],
                }],
            },
            ServiceMessage::AcpSessionConfigSelectionResult {
                sid: "s".into(),
                request_id: 7,
                config_id: Some("model".into()),
                value: "opus".into(),
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
