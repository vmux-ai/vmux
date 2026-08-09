use super::*;
use std::time::{Duration, Instant};

#[test]
fn heavy_output_waits_for_frame_interval_but_sparse_and_final_output_do_not() {
    assert!(!output_viewport_due(
        true,
        false,
        Some(HEAVY_OUTPUT_FRAME_INTERVAL - Duration::from_millis(1)),
    ));
    assert!(output_viewport_due(
        true,
        false,
        Some(HEAVY_OUTPUT_FRAME_INTERVAL),
    ));
    assert!(output_viewport_due(true, true, Some(Duration::ZERO)));
    assert!(output_viewport_due(false, false, Some(Duration::ZERO)));
}

#[test]
fn input_priority_waits_for_fresh_pty_output() {
    let input_pending = AtomicBool::new(true);

    assert!(!take_input_priority(&input_pending, false));
    assert!(input_pending.load(Ordering::Acquire));
    assert!(take_input_priority(&input_pending, true));
    assert!(!input_pending.load(Ordering::Acquire));
}

#[test]
fn pty_reader_notifies_when_output_arrives() {
    let (wake_tx, mut wake_rx) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake(
        ProcessId::new(),
        "sh".to_string(),
        vec![],
        String::new(),
        Vec::new(),
        80,
        24,
        wake_tx,
    )
    .expect("process should spawn");

    process.write_input(b"printf vmux-wake-test\r");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if wake_rx.try_recv().is_ok() {
            process.kill();
            return;
        }
        if Instant::now() >= deadline {
            process.kill();
            panic!("timed out waiting for PTY wake notification");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn process_starts_in_requested_cwd_without_typing_cd() {
    let temp = std::env::temp_dir().join(format!("vmux-process-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let cwd = temp.to_string_lossy().into_owned();
    let home = temp.join("home-marker").to_string_lossy().into_owned();
    let (wake_tx, _) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake(
        ProcessId::new(),
        "/bin/sh".to_string(),
        vec![],
        cwd.clone(),
        vec![("HOME".to_string(), home.clone())],
        120,
        24,
        wake_tx,
    )
    .expect("process should spawn");

    drain_process_output(&mut process, Duration::from_millis(300));
    process.write_input(b"printf 'HOME=%s\\n' \"$HOME\"; pwd\r");
    let text = wait_for_snapshot_text(&mut process, &cwd);

    process.kill();
    let _ = std::fs::remove_dir_all(&temp);

    assert!(text.contains(&cwd));
    assert!(text.contains(&format!("HOME={home}")));
    assert!(!text.contains(&format!("cd {cwd}")));
}

#[test]
fn process_rejects_invalid_cwd_at_spawn() {
    let mut mgr = ProcessManager::default();
    let cwd = std::env::temp_dir().join(format!(
        "vmux-process-missing-cwd-{}-{}",
        std::process::id(),
        ProcessId::new()
    ));

    let result = mgr.create_process(
        ProcessId::new(),
        "/bin/sh".into(),
        vec!["-c".into(), "exit 0".into()],
        cwd.to_string_lossy().into_owned(),
        Vec::new(),
        80,
        24,
    );

    assert!(result.is_err());
    assert!(mgr.processes.is_empty());

    let file = std::env::temp_dir().join(format!(
        "vmux-process-file-cwd-{}-{}",
        std::process::id(),
        ProcessId::new()
    ));
    std::fs::write(&file, b"not a directory").expect("write cwd file");
    let result = mgr.create_process(
        ProcessId::new(),
        "/bin/sh".into(),
        vec!["-c".into(), "exit 0".into()],
        file.to_string_lossy().into_owned(),
        Vec::new(),
        80,
        24,
    );
    let _ = std::fs::remove_file(file);

    assert!(result.is_err());
    assert!(mgr.processes.is_empty());
}

#[test]
fn process_with_cwd_rejects_missing_executable_at_spawn() {
    let mut mgr = ProcessManager::default();
    let cwd = std::env::temp_dir().join(format!(
        "vmux-process-command-cwd-{}-{}",
        std::process::id(),
        ProcessId::new()
    ));
    std::fs::create_dir_all(&cwd).expect("create cwd");

    let result = mgr.create_process(
        ProcessId::new(),
        "/definitely/missing/vmux-command".into(),
        Vec::new(),
        cwd.to_string_lossy().into_owned(),
        Vec::new(),
        80,
        24,
    );
    assert!(result.is_err());
    assert!(mgr.processes.is_empty());

    let command = cwd.join("not-executable");
    std::fs::write(&command, b"#!/bin/sh\nexit 0\n").expect("write command");
    let result = mgr.create_process(
        ProcessId::new(),
        command.to_string_lossy().into_owned(),
        Vec::new(),
        cwd.to_string_lossy().into_owned(),
        Vec::new(),
        80,
        24,
    );
    let _ = std::fs::remove_dir_all(cwd);

    assert!(result.is_err());
    assert!(mgr.processes.is_empty());
}

#[test]
fn full_text_includes_scrolled_off_history() {
    let (wake_tx, _) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake(
        ProcessId::new(),
        "/bin/sh".to_string(),
        vec![],
        String::new(),
        Vec::new(),
        80,
        24,
        wake_tx,
    )
    .expect("process should spawn");

    drain_process_output(&mut process, Duration::from_millis(300));
    // Screen is 24 rows; printing ~60 lines scrolls FIRSTLINE into history.
    process.write_input(
        b"echo FIRSTLINE; for i in $(seq 1 60); do echo pad_$i; done; echo LASTLINE\r",
    );
    let _ = wait_for_snapshot_text(&mut process, "LASTLINE");
    drain_process_output(&mut process, Duration::from_millis(200));

    let visible = snapshot_text(process.snapshot());
    let full = process.full_text();
    process.kill();

    assert!(
        full.contains("LASTLINE"),
        "full_text should include last line"
    );
    assert!(
        full.contains("FIRSTLINE"),
        "full_text should include scrolled-off first line; full=\n{full}"
    );
    assert!(
        !visible.contains("FIRSTLINE"),
        "visible snapshot should not include scrolled-off line; visible=\n{visible}"
    );
}

fn drain_process_output(process: &mut Process, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        process.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn poll_broadcasts_command_lifecycle_from_osc133() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake(
        ProcessId::new(),
        "/bin/sh".to_string(),
        vec!["-c".to_string(), "printf '\\033]133;D;0\\007'".to_string()],
        String::new(),
        vec![],
        80,
        24,
        wake_tx,
    )
    .expect("spawn");
    let mut rx = process.subscribe();

    drain_process_output(&mut process, Duration::from_secs(2));

    let mut saw_end = false;
    while let Ok(msg) = rx.try_recv() {
        if let ServiceMessage::CommandLifecycle {
            kind: crate::protocol::CommandLifecycleKind::Ended { exit_code },
            ..
        } = msg
        {
            assert_eq!(exit_code, Some(0));
            saw_end = true;
        }
    }
    assert!(
        saw_end,
        "expected a CommandLifecycle Ended broadcast from OSC 133;D;0"
    );
    assert_eq!(
        process.command_status(),
        (1, Some(0)),
        "command_status must record one completed command with its exit code"
    );
}

fn wait_for_snapshot_text(process: &mut Process, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        process.poll();
        let text = snapshot_text(process.snapshot());
        if text.contains(needle) || Instant::now() >= deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn snapshot_text(snapshot: ServiceMessage) -> String {
    let ServiceMessage::Snapshot { lines, .. } = snapshot else {
        unreachable!();
    };
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn write_input_to_writer_does_not_need_process_lock() {
    #[derive(Clone)]
    struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for CapturingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let captured = Arc::new(Mutex::new(Vec::new()));
    let writer = PtyInputWriter::new(Box::new(CapturingWriter(captured.clone())));

    Process::write_input_to_writer(&writer, b"abc");

    assert_eq!(*captured.lock().unwrap(), b"abc".to_vec());
    assert!(writer.input_pending.load(Ordering::Acquire));
}

#[test]
fn copy_mode_up_at_alt_screen_top_uses_mouse_wheel_scroll() {
    #[derive(Clone)]
    struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for CapturingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let (wake_tx, _) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake(
        ProcessId::new(),
        "/bin/sh".to_string(),
        vec![],
        String::new(),
        Vec::new(),
        12,
        8,
        wake_tx,
    )
    .expect("process should spawn");
    let captured = Arc::new(Mutex::new(Vec::new()));
    process.pty_writer = PtyInputWriter::new(Box::new(CapturingWriter(captured.clone())));

    process.process_output_for_test(b"\x1b[?1049h\x1b[Hone\r\ntwo\r\nthree\x1b[H");
    process.enter_copy_mode();
    process.copy_mode_key(crate::protocol::CopyModeKey::StartLineSelection);
    process.copy_mode_key(crate::protocol::CopyModeKey::Up);

    assert_eq!(process.copy_mode.as_ref().unwrap().cursor.1, 1);
    process.copy_mode_key(crate::protocol::CopyModeKey::Up);
    process.kill();

    assert_eq!(*captured.lock().unwrap(), b"\x1b[<64;7;5M".to_vec());
}

fn capturing_process(cols: u16, rows: u16) -> (Process, Arc<Mutex<Vec<u8>>>) {
    #[derive(Clone)]
    struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for CapturingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let (wake_tx, _) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake(
        ProcessId::new(),
        "/bin/sh".to_string(),
        vec![],
        String::new(),
        Vec::new(),
        cols,
        rows,
        wake_tx,
    )
    .expect("process should spawn");
    let captured = Arc::new(Mutex::new(Vec::new()));
    process.pty_writer = PtyInputWriter::new(Box::new(CapturingWriter(captured.clone())));
    (process, captured)
}

#[test]
fn mouse_wheel_in_mouse_mode_forwards_sgr() {
    let (mut process, captured) = capturing_process(12, 8);
    process.process_output_for_test(b"\x1b[?1000h");
    process.handle_mouse_wheel(true, 6, 4, 0);
    process.handle_mouse_wheel(false, 0, 0, 0);
    process.kill();
    assert_eq!(
        *captured.lock().unwrap(),
        b"\x1b[<64;7;5M\x1b[<65;1;1M".to_vec()
    );
}

#[test]
fn mouse_wheel_in_alt_screen_sends_arrow_keys() {
    let (mut process, captured) = capturing_process(12, 8);
    process.process_output_for_test(b"\x1b[?1049h");
    process.handle_mouse_wheel(true, 0, 0, 0);
    process.handle_mouse_wheel(false, 0, 0, 0);
    process.kill();
    assert_eq!(*captured.lock().unwrap(), b"\x1b[A\x1b[B".to_vec());
}

#[test]
fn mouse_wheel_in_alt_screen_app_cursor_sends_ss3_arrows() {
    let (mut process, captured) = capturing_process(12, 8);
    process.process_output_for_test(b"\x1b[?1049h\x1b[?1h");
    process.handle_mouse_wheel(true, 0, 0, 0);
    process.kill();
    assert_eq!(*captured.lock().unwrap(), b"\x1bOA".to_vec());
}

#[test]
fn mouse_wheel_in_alt_screen_without_alternate_scroll_is_inert() {
    let (mut process, captured) = capturing_process(12, 8);
    process.process_output_for_test(b"\x1b[?1049h\x1b[?1007l");
    process.handle_mouse_wheel(true, 0, 0, 0);
    process.kill();
    assert!(captured.lock().unwrap().is_empty());
}

#[test]
fn scroll_window_serves_document_row_window() {
    let (mut process, captured) = capturing_process(12, 4);
    let mut feed = Vec::new();
    for i in 0..40 {
        feed.extend_from_slice(format!("line{i}\r\n").as_bytes());
    }
    process.process_output_for_test(&feed);

    let total = process.term.grid().total_lines() as u32;
    assert!(
        total >= 40,
        "expected scrollback to accumulate, got {total}"
    );

    let mut patches = process.subscribe();
    // Scroll to the very top (document row 0), not following.
    process.handle_scroll_window(0, false);

    // Native scroll must not move display_offset or write to the pty.
    assert_eq!(process.term.grid().display_offset(), 0);
    assert!(
        captured.lock().unwrap().is_empty(),
        "native scroll must not write to the pty"
    );

    let (changed_lines, first_row, total_rows) = std::iter::from_fn(|| patches.try_recv().ok())
        .find_map(|msg| match msg {
            ServiceMessage::ViewportPatch {
                changed_lines,
                first_row,
                total_rows,
                ..
            } => Some((changed_lines, first_row, total_rows)),
            _ => None,
        })
        .expect("scroll must broadcast a viewport patch");
    assert_eq!(
        first_row, 0,
        "top scroll serves the window from document row 0"
    );
    assert_eq!(total_rows, total, "patch carries the full document height");
    assert!(
        changed_lines.iter().any(|(r, _)| *r == 0),
        "top window must include the oldest document row"
    );

    process.kill();
}

#[test]
fn following_patch_contains_only_visible_rows() {
    let (mut process, _) = capturing_process(12, 4);
    let mut patches = process.subscribe();
    let mut feed = Vec::new();
    for i in 0..40 {
        feed.extend_from_slice(format!("line{i}\r\n").as_bytes());
    }

    process.process_output_for_test(&feed);

    let changed_lines = std::iter::from_fn(|| patches.try_recv().ok())
        .find_map(|message| match message {
            ServiceMessage::ViewportPatch { changed_lines, .. } => Some(changed_lines),
            _ => None,
        })
        .expect("output must broadcast a viewport patch");
    assert!(changed_lines.len() <= process.rows as usize);
    process.kill();
}

#[test]
fn terminal_mode_broadcasts_alt_screen_toggle() {
    let (wake_tx, _) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake(
        ProcessId::new(),
        "/bin/sh".to_string(),
        vec![],
        String::new(),
        Vec::new(),
        12,
        8,
        wake_tx,
    )
    .expect("process should spawn");

    let mut rx = process.subscribe();

    process.process_output_for_test(b"\x1b[?1049h");
    process.maybe_broadcast_mode();

    let mut alt_on = None;
    while let Ok(msg) = rx.try_recv() {
        if let ServiceMessage::TerminalMode { alt_screen, .. } = msg {
            alt_on = Some(alt_screen);
        }
    }
    assert_eq!(
        alt_on,
        Some(true),
        "entering alt screen broadcasts alt_screen=true"
    );

    process.process_output_for_test(b"\x1b[?1049l");
    process.maybe_broadcast_mode();

    let mut alt_off = None;
    while let Ok(msg) = rx.try_recv() {
        if let ServiceMessage::TerminalMode { alt_screen, .. } = msg {
            alt_off = Some(alt_screen);
        }
    }
    assert_eq!(
        alt_off,
        Some(false),
        "leaving alt screen broadcasts alt_screen=false"
    );

    process.kill();
}

#[test]
fn create_process_returns_real_pid() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut mgr = ProcessManager::new(wake_tx);
    let process_id = ProcessId::new();
    let (id, pid) = mgr
        .create_process(
            process_id,
            "/bin/sh".into(),
            vec![],
            String::new(),
            Vec::new(),
            80,
            24,
        )
        .expect("spawn");
    assert!(pid > 0, "expected real pid, got {pid}");
    assert!(mgr.processes.contains_key(&id));
}

#[test]
fn proxy_broadcasts_process_title_on_term_title_event() {
    use std::io;

    let (tx, mut rx) = broadcast::channel::<ServiceMessage>(8);
    let writer = PtyInputWriter::new(Box::new(io::sink()));
    let process_id = ProcessId::new();
    let proxy = ServiceEventProxy {
        process_id,
        pty_writer: writer,
        patch_tx: tx,
    };

    proxy.send_event(TermEvent::Title("hello-osc".into()));

    let msg = rx.try_recv().expect("ProcessTitle should be broadcast");
    match msg {
        ServiceMessage::ProcessTitle {
            process_id: got_id,
            title,
        } => {
            assert_eq!(got_id, process_id);
            assert_eq!(title, "hello-osc");
        }
        other => panic!("expected ProcessTitle, got {other:?}"),
    }
}

#[test]
fn proxy_broadcasts_bell_on_term_bell_event() {
    use std::io;

    let (tx, mut rx) = broadcast::channel::<ServiceMessage>(8);
    let writer = PtyInputWriter::new(Box::new(io::sink()));
    let process_id = ProcessId::new();
    let proxy = ServiceEventProxy {
        process_id,
        pty_writer: writer,
        patch_tx: tx,
    };

    proxy.send_event(TermEvent::Bell);

    let msg = rx.try_recv().expect("Bell should be broadcast");
    match msg {
        ServiceMessage::Bell { process_id: got_id } => assert_eq!(got_id, process_id),
        other => panic!("expected Bell, got {other:?}"),
    }
}

#[test]
fn keep_after_exit_retains_process_and_exit_code() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut mgr = ProcessManager::new(wake_tx);
    let id = ProcessId::new();
    mgr.create_process_keep_alive(
        id,
        "/bin/sh".into(),
        vec!["-c".into(), "exit 5".into()],
        String::new(),
        Vec::new(),
        80,
        24,
    )
    .expect("spawn");
    let mut rx = mgr.processes.get(&id).unwrap().subscribe();

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut saw_exit = false;
    while Instant::now() < deadline {
        if mgr.poll_all().contains(&id) {
            saw_exit = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(saw_exit, "process should have exited");

    // Kept in the manager (not reaped by poll) with its exit code recorded.
    let process = mgr.processes.get(&id).expect("kept after exit");
    assert_eq!(process.process_exit(), Some(5));

    // Further polls neither report the exit again nor drop the process.
    assert!(mgr.poll_all().is_empty());
    assert!(mgr.processes.contains_key(&id));

    // Exactly one ProcessExited was broadcast.
    let mut exits = 0;
    while let Ok(msg) = rx.try_recv() {
        if matches!(msg, ServiceMessage::ProcessExited { .. }) {
            exits += 1;
        }
    }
    assert_eq!(exits, 1);

    mgr.remove_process(&id);
}

#[test]
fn process_exit_drains_all_queued_pty_output() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut mgr = ProcessManager::new(wake_tx);
    let id = ProcessId::new();
    mgr.create_process_keep_alive(
            id,
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "awk 'BEGIN { for (i = 0; i < 70000; i++) print \"abcdefgh\"; print \"TAIL-SENTINEL\" }'"
                    .into(),
            ],
            String::new(),
            Vec::new(),
            80,
            24,
        )
        .expect("spawn");
    std::thread::sleep(Duration::from_millis(500));

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_exit = false;
    while Instant::now() < deadline {
        if mgr.poll_all().contains(&id) {
            saw_exit = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(saw_exit, "process should exit");
    assert!(
        mgr.processes
            .get(&id)
            .expect("process retained")
            .full_text()
            .contains("TAIL-SENTINEL"),
        "exit must not discard queued PTY output"
    );
    mgr.remove_process(&id);
}

#[test]
fn process_exit_is_reported_after_queued_pty_output_is_drained() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut mgr = ProcessManager::new(wake_tx);
    let id = ProcessId::new();
    mgr.create_process_keep_alive(
            id,
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "awk 'BEGIN { for (i = 0; i < 70000; i++) print \"abcdefgh\"; print \"TAIL-SENTINEL\" }'"
                    .into(),
            ],
            String::new(),
            Vec::new(),
            80,
            24,
        )
        .expect("spawn");
    let mut rx = mgr.processes.get(&id).expect("process").subscribe();
    std::thread::sleep(Duration::from_millis(500));

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut reported = false;
    while Instant::now() < deadline {
        mgr.poll_all();
        while let Ok(message) = rx.try_recv() {
            if matches!(message, ServiceMessage::ProcessExited { .. }) {
                reported = true;
            }
        }
        if reported {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(reported, "process exit should be reported");
    assert!(
        mgr.processes
            .get(&id)
            .expect("process retained")
            .full_text()
            .contains("TAIL-SENTINEL"),
        "exit must not be reported while queued PTY output remains"
    );
    mgr.remove_process(&id);
}

#[test]
fn process_exit_waits_for_pty_reader_catch_up() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut process = Process::new_with_wake_and_reader_delay(
        ProcessId::new(),
        "/bin/sh".into(),
        vec!["-c".into(), "printf READER-TAIL".into()],
        String::new(),
        Vec::new(),
        80,
        24,
        wake_tx,
        Duration::from_millis(100),
    )
    .expect("spawn");
    let mut rx = process.subscribe();

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut reported = false;
    while Instant::now() < deadline {
        process.poll();
        while let Ok(message) = rx.try_recv() {
            if matches!(message, ServiceMessage::ProcessExited { .. }) {
                reported = true;
            }
        }
        if reported {
            break;
        }
        std::thread::yield_now();
    }

    assert!(reported, "process exit should be reported");
    assert!(
        process.full_text().contains("READER-TAIL"),
        "exit raced with the PTY reader"
    );
    process.kill();
}

#[test]
fn process_exit_is_reported_before_background_descendant_closes_pty() {
    let (wake_tx, _wake_rx) = mpsc::unbounded_channel();
    let mut mgr = ProcessManager::new(wake_tx);
    let id = ProcessId::new();
    mgr.create_process_keep_alive(
        id,
        "/bin/sh".into(),
        vec!["-c".into(), "exit 0".into()],
        String::new(),
        Vec::new(),
        80,
        24,
    )
    .expect("spawn");
    std::thread::sleep(Duration::from_millis(50));
    let (pty_tx, pty_rx) = mpsc::unbounded_channel();
    let process = mgr.processes.get_mut(&id).expect("process");
    process.pty_rx = pty_rx;
    let mut rx = process.subscribe();

    let deadline = Instant::now() + Duration::from_millis(500);
    let mut reported = false;
    while Instant::now() < deadline {
        assert!(
            !mgr.poll_all().contains(&id),
            "process must remain until PTY output is drained"
        );
        while let Ok(message) = rx.try_recv() {
            if matches!(message, ServiceMessage::ProcessExited { .. }) {
                reported = true;
            }
        }
        if reported {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    drop(pty_tx);
    mgr.remove_process(&id);
    assert!(reported, "child exit must not wait for PTY closure");
}
