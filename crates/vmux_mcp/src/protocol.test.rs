use super::*;

#[test]
fn read_lines_bounded_offset_and_limit() {
    let dir = std::env::temp_dir().join(format!("vmux-mcp-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("f.txt");
    std::fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();
    let p = path.to_str().unwrap();
    assert_eq!(read_lines_bounded(p, None, None).unwrap(), "a\nb\nc\nd\ne");
    assert_eq!(read_lines_bounded(p, Some(2), Some(2)).unwrap(), "b\nc");
    assert_eq!(read_lines_bounded(p, Some(4), None).unwrap(), "d\ne");
    assert_eq!(read_lines_bounded(p, Some(99), None).unwrap(), "");
    assert_eq!(
        read_lines_bounded(p, Some(1), Some(100)).unwrap(),
        "a\nb\nc\nd\ne"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn byte_to_utf16_converts_multibyte_offsets() {
    // 'a'=1B/1u, 'é'=2B/1u, '😀'=4B/2u, 'b'=1B/1u
    let line = "aé😀b";
    assert_eq!(byte_to_utf16(line, 0), 0);
    assert_eq!(byte_to_utf16(line, 1), 1);
    assert_eq!(byte_to_utf16(line, 2), 1); // mid-'é' snaps down to a boundary
    assert_eq!(byte_to_utf16(line, 3), 2);
    assert_eq!(byte_to_utf16(line, 7), 4);
    assert_eq!(byte_to_utf16(line, 999), 5); // clamps to end
}

#[test]
fn opt_u32_rejects_invalid() {
    assert_eq!(opt_u32(&json!({}), "offset", "read_file").unwrap(), None);
    assert_eq!(
        opt_u32(&json!({"offset": 7}), "offset", "read_file").unwrap(),
        Some(7)
    );
    assert!(opt_u32(&json!({"offset": -1}), "offset", "read_file").is_err());
    assert!(opt_u32(&json!({"offset": 1.5}), "offset", "read_file").is_err());
    assert!(opt_u32(&json!({"offset": 5_000_000_000u64}), "offset", "read_file").is_err());
}

#[test]
fn newline_framing_reads_single_json_message() {
    let mut lines = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n".as_slice();
    let request = read_json_line(&mut lines).unwrap().unwrap();

    assert_eq!(request["method"], "tools/list");
}

#[tokio::test]
async fn acp_tools_call_rejects_hidden_terminal_tools() {
    for name in ["run", "read_terminal"] {
        let response = handle_message(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": name, "arguments": {} }
            }),
            None,
            true,
            true,
            Duration::from_secs(50),
        )
        .await
        .unwrap();

        assert_eq!(response["result"]["isError"], true);
        assert_eq!(
            response["result"]["content"][0]["text"],
            format!("tool {name} is unavailable for ACP sessions")
        );
    }
}

#[tokio::test]
async fn acp_tools_call_rejects_resume_in_acp() {
    let response = handle_message(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "resume_in_acp", "arguments": {} }
        }),
        None,
        true,
        false,
        Duration::from_secs(50),
    )
    .await
    .unwrap();

    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["content"][0]["text"],
        "tool resume_in_acp is unavailable for ACP sessions"
    );
}

#[test]
fn image_query_result_maps_to_text_and_image_blocks() {
    use vmux_client::protocol::AgentQueryResult;
    let resp = query_result_to_mcp_response(AgentQueryResult::Image {
        path: "/tmp/shot.png".into(),
        png: vec![137, 80, 78, 71],
        width: 800,
        height: 600,
    });
    let content = resp["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert!(
        content[0]["text"]
            .as_str()
            .unwrap()
            .contains("/tmp/shot.png")
    );
    assert!(content[0]["text"].as_str().unwrap().contains("800"));
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    assert_eq!(content[1]["data"], "iVBORw==");
}

#[test]
fn recording_maps_to_text_block() {
    use vmux_client::protocol::AgentQueryResult;
    let v = query_result_to_mcp_response(AgentQueryResult::Recording {
        mp4_path: "/tmp/x.mp4".into(),
        gif_path: Some("/tmp/x.gif".into()),
        duration_ms: 7400,
        bytes: 1_000_000,
        auto_stopped: true,
    });
    let text = v["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("/tmp/x.mp4"));
    assert!(text.contains("/tmp/x.gif"));
    assert!(text.contains("auto-stopped"));
    assert!(v.get("isError").is_none());
}

#[test]
fn vault_status_reports_provider_and_pending_sync() {
    let response = vault_status_response(vmux_profile::vault::VaultStatus {
        root: "/Users/test/.vmux".into(),
        initialized: true,
        encrypted: true,
        unlocked: true,
        passkey_credentials: vec!["a1".repeat(32)],
        recovery_enabled: true,
        remote: "https://github.com/vmux-ai/vault.git".into(),
        branch: "main".into(),
        dirty: 2,
        ahead: 1,
        behind: 0,
        ..Default::default()
    });
    let status: Value =
        serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();

    assert_eq!(status["connected"], true);
    assert_eq!(status["encrypted"], true);
    assert_eq!(status["unlocked"], true);
    assert_eq!(status["passkeys"], 1);
    assert_eq!(status["recoveryKey"], true);
    assert_eq!(status["automaticBackup"], true);
    assert_eq!(status["provider"], "github");
    assert_eq!(status["localChanges"], 2);
    assert_eq!(status["ahead"], 1);
    assert_eq!(status["syncNeeded"], true);
}

#[test]
fn output_since_returns_appended_tail() {
    let baseline = "prompt$ ";
    let final_text = "prompt$ ls\nfile_a\nfile_b\nprompt$ ";
    assert_eq!(
        output_since(baseline, final_text),
        "ls\nfile_a\nfile_b\nprompt$"
    );
}

#[test]
fn output_since_falls_back_to_full_when_prefix_shifted() {
    let baseline = "old prompt$ ";
    let final_text = "different\noutput here";
    assert_eq!(output_since(baseline, final_text), "different\noutput here");
}

#[test]
fn run_result_shapes_text() {
    let timeout = Duration::from_secs(600);
    let done = run_result("pid7", Some(1), "boom", false, timeout);
    let text = done["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("terminal: pid7"));
    assert!(text.contains("exit: 1"));
    assert!(text.contains("output:\nboom"));

    let timed_out = run_result("pid7", None, "partial", true, timeout);
    let text = timed_out["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("still running"));
    assert!(text.contains("600s"));
    assert!(text.contains("read_terminal(pid7)"));
}

#[test]
fn blocking_run_sets_done_marker_token() {
    let request_id = AgentRequestId([7; 16]);
    let anchor = vmux_client::protocol::ProcessId::new();
    let run = AgentCommand::Run {
        anchor,
        command: "git status".into(),
        direction: vmux_client::protocol::AgentPaneDirection::Right,
        focus: false,
        beside: None,
        mode: vmux_client::protocol::PlacementMode::Auto,
        terminal: None,
        done_marker: None,
    };

    let marked = blocking_run_with_marker(run, request_id);

    match marked {
        AgentCommand::Run { done_marker, .. } => {
            assert_eq!(done_marker, Some(run_done_token(request_id)));
        }
        _ => panic!("expected run command"),
    }
}

#[test]
fn blocking_placement_override_run_sets_done_marker_token() {
    let request_id = AgentRequestId([9; 16]);
    let run = AgentCommand::RunWithPlacementOverride {
        anchor: vmux_client::protocol::ProcessId::new(),
        command: "git status".into(),
        direction: vmux_client::protocol::AgentPaneDirection::Bottom,
        focus: false,
        beside: None,
        mode: vmux_client::protocol::PlacementMode::Split,
        terminal: None,
        done_marker: None,
    };

    let marked = blocking_run_with_marker(run, request_id);

    match marked {
        AgentCommand::RunWithPlacementOverride { done_marker, .. } => {
            assert_eq!(done_marker, Some(run_done_token(request_id)));
        }
        _ => panic!("expected run placement override command"),
    }
}

#[test]
fn blocking_run_waits_for_new_terminal_process_to_materialize() {
    let process_id = vmux_client::protocol::ProcessId::new();
    let result = AgentQueryResult::Error(format!("process not found: {process_id}"));

    assert_eq!(
        run_completion_exit(result, "token", process_id, true).unwrap(),
        None
    );
}

#[test]
fn blocking_run_surfaces_process_missing_after_startup_grace() {
    let process_id = vmux_client::protocol::ProcessId::new();
    let message = format!("process not found: {process_id}");
    let result = AgentQueryResult::Error(message.clone());

    assert_eq!(
        run_completion_exit(result, "token", process_id, false),
        Err(message)
    );
}
