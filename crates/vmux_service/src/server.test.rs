use super::*;
use crate::protocol::{AgentCommandResult, AgentQuery, AgentQueryResult, AgentRequestId};
use tokio::sync::oneshot;

#[test]
fn page_agent_prompt_appends_attachment_paths() {
    let attachments = vec![AgentAttachment {
        path: "/tmp/report.txt".into(),
        name: "report.txt".into(),
        mime_type: "text/plain".into(),
        size: 12,
    }];
    assert_eq!(
        page_agent_prompt("review".into(), &attachments),
        "review\n\nAttached files:\n- /tmp/report.txt"
    );
}

#[test]
fn page_agent_private_context_keeps_empty_display_prompt() {
    let prompt = compose_agent_prompt(&page_agent_prompt(String::new(), &[]), Some("resume"));

    assert!(prompt.contains("resume"));
    assert_eq!(crate::protocol::extract_display_prompt(&prompt), Some(""));
}

#[test]
fn acp_rejects_stdio_mcp_server_with_working_directory() {
    assert!(
        to_acp_mcp_server(ManagedMcpServer {
            name: "local".into(),
            transport: ManagedMcpTransport::Stdio,
            command: Some("server".into()),
            args: Vec::new(),
            env: Vec::new(),
            cwd: Some("/tmp/project".into()),
            url: None,
            headers: Vec::new(),
        })
        .is_none()
    );
}

#[test]
fn acp_spawn_replays_agent_info_after_subscribing() {
    let production = include_str!("server.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("server production source");
    assert!(production.contains("acp_manager.lock().await.agent_info(&sid)"));
}

#[test]
fn wake_drain_coalesces_all_pending_output() {
    let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
    for _ in 0..1024 {
        wake_tx
            .send(ProcessId::new())
            .expect("wake event should queue");
    }

    drain_pending_wakes(&mut wake_rx);

    assert!(wake_rx.try_recv().is_err());
}

#[tokio::test]
async fn pending_queries_roundtrips_oneshot() {
    let pending: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
    let request_id = AgentRequestId::new();
    let (tx, rx) = oneshot::channel::<AgentQueryResult>();
    pending.lock().await.insert(request_id, tx);

    let result = AgentQueryResult::Settings("{}".into());
    let resp_tx = pending.lock().await.remove(&request_id).expect("entry");
    resp_tx.send(result.clone()).expect("send");

    let received = rx.await.expect("recv");
    assert_eq!(received, result);
}

#[tokio::test]
async fn pending_queries_returns_none_for_unknown_request_id() {
    let pending: PendingQueries = Arc::new(Mutex::new(HashMap::new()));
    let request_id = AgentRequestId::new();
    assert!(pending.lock().await.remove(&request_id).is_none());

    let _ = AgentQuery::ReadLayout { anchor: None };
}

#[tokio::test]
async fn pending_commands_roundtrips_oneshot() {
    let pending: PendingCommands = Arc::new(Mutex::new(HashMap::new()));
    let request_id = AgentRequestId::new();
    let (tx, rx) = oneshot::channel::<AgentCommandResult>();
    pending.lock().await.insert(request_id, tx);

    let result = AgentCommandResult::Ok;
    let resp_tx = pending.lock().await.remove(&request_id).expect("entry");
    resp_tx.send(result.clone()).expect("send");

    let received = rx.await.expect("recv");
    assert_eq!(received, result);
}

#[tokio::test]
async fn shutdown_message_breaks_run_server() {
    use crate::protocol::ClientMessage;

    let dir = std::env::temp_dir().join(format!("vmux-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("test.sock");
    let _ = std::fs::remove_file(&sock);
    let listener = tokio::net::UnixListener::bind(&sock).unwrap();

    let server = tokio::spawn(super::run_server(listener));

    let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
    let (_r, mut w) = stream.into_split();
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&ClientMessage::Shutdown).expect("serialize");
    crate::framing::write_raw_frame(&mut w, &bytes)
        .await
        .expect("write shutdown");

    let res = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
    assert!(res.is_ok(), "run_server did not exit after Shutdown");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn process_alive(pid: u32, identity: &Option<String>) -> bool {
    if unsafe { libc::kill(pid as i32, 0) } != 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        if linux_proc_state(pid) == Some('Z') {
            return false;
        }
        if linux_proc_starttime(pid) != *identity {
            return false;
        }
    }
    true
}

fn proc_identity(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        linux_proc_starttime(pid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_proc_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?.1.trim_start().chars().next()
}

#[cfg(target_os = "linux")]
fn linux_proc_starttime(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)
        .map(str::to_string)
}

fn proc_state_label(pid: u32) -> String {
    #[cfg(target_os = "linux")]
    {
        linux_proc_state(pid)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "gone".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        "n/a".to_string()
    }
}

async fn await_child_pid(pidfile: &std::path::Path) -> Option<u32> {
    for _ in 0..200 {
        if let Ok(s) = std::fs::read_to_string(pidfile)
            && let Ok(pid) = s.trim().parse::<u32>()
            && pid > 0
        {
            return Some(pid);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    None
}

#[tokio::test]
async fn client_disconnect_reaps_created_processes() {
    use crate::protocol::ClientMessage;

    let dir = std::env::temp_dir().join(format!("vmux-reap-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("reap.sock");
    let pidfile = dir.join("child.pid");
    let _ = std::fs::remove_file(&sock);
    let _ = std::fs::remove_file(&pidfile);
    let listener = tokio::net::UnixListener::bind(&sock).unwrap();

    let server = tokio::spawn(super::run_server(listener));

    let stream = tokio::net::UnixStream::connect(&sock).await.unwrap();
    let (r, mut w) = stream.into_split();

    let create = ClientMessage::CreateProcess {
        process_id: ProcessId::new(),
        command: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            format!("echo $$ > {}; exec sleep 30", pidfile.display()),
        ],
        cwd: dir.display().to_string(),
        env: vec![],
        cols: 80,
        rows: 24,
    };
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&create).expect("serialize");
    crate::framing::write_raw_frame(&mut w, &bytes)
        .await
        .expect("write create");

    let pid = await_child_pid(&pidfile)
        .await
        .expect("child process should report its pid");
    let identity = proc_identity(pid);
    assert!(
        process_alive(pid, &identity),
        "child should be alive after CreateProcess"
    );

    // Simulate a desktop crash: drop the client connection without Shutdown.
    drop(w);
    drop(r);

    let reaped = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while process_alive(pid, &identity) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;

    // Hygiene: ensure no leaked child regardless of outcome.
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    server.abort();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        reaped.is_ok(),
        "child pid {pid} still alive after client disconnect — service did not reap it (state: {})",
        proc_state_label(pid)
    );
}
