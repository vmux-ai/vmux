use vmux_service::process::Process;
use vmux_service::protocol::{CopyModeKey, ServiceMessage};
use vmux_terminal::event::TermSelectionRange;

static PTY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct TestProcess {
    _guard: std::sync::MutexGuard<'static, ()>,
    process: Process,
}

impl std::ops::Deref for TestProcess {
    type Target = Process;

    fn deref(&self) -> &Self::Target {
        &self.process
    }
}

impl std::ops::DerefMut for TestProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.process
    }
}

fn new_process() -> TestProcess {
    let guard = PTY_TEST_LOCK.lock().expect("pty test lock");
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let process = Process::new(shell, String::new(), Vec::new(), 80, 24).expect("spawn process");

    TestProcess {
        _guard: guard,
        process,
    }
}

/// Write input to the PTY and let the reader thread + VTE catch up.
fn write_and_drain(process: &mut Process, bytes: &[u8]) {
    process.write_input(bytes);
    for _ in 0..50 {
        let _ = process.poll();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn set_and_clear_selection() {
    let mut process = new_process();
    process.set_selection(Some(TermSelectionRange {
        start_col: 0,
        start_row: 0,
        end_col: 4,
        end_row: 0,
        is_block: false,
    }));
    assert!(process.selection_text().is_some());
    process.set_selection(None);
    assert!(process.selection_text().is_none());
}

#[test]
fn extend_from_empty_anchors_a_single_cell() {
    let mut process = new_process();
    process.extend_selection_to(3, 1);
    let text = process.selection_text().unwrap_or_default();
    // Single-cell selection on a blank cell == "" after trailing-space strip.
    assert!(text.chars().count() <= 1, "got {text:?}");
}

#[test]
fn select_line_strips_trailing_blanks() {
    let mut process = new_process();
    write_and_drain(&mut process, b"hello world\n");
    let mut found = false;
    for row in 0..24u16 {
        process.select_line_at(row);
        if let Some(text) = process.selection_text()
            && text.contains("hello world")
        {
            assert!(!text.ends_with(' '), "got {text:?}");
            found = true;
            break;
        }
    }
    assert!(found, "did not find 'hello world' in any row");
}

#[test]
fn select_word_walks_word_chars() {
    let mut process = new_process();
    write_and_drain(&mut process, b"foo_bar baz\n");
    for row in 0..24u16 {
        for col in 0..15u16 {
            process.select_word_at(col, row);
            if let Some(text) = process.selection_text()
                && text == "foo_bar"
            {
                return;
            }
        }
    }
    panic!("did not find foo_bar word selection");
}

#[test]
fn entering_copy_mode_starts_without_selection() {
    let mut process = new_process();
    process.enter_copy_mode();
    assert!(process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn copy_mode_movement_without_visual_keeps_selection_empty() {
    let mut process = new_process();
    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::Right);
    process.copy_mode_key(CopyModeKey::Right);
    let copied = process.copy_mode_key(CopyModeKey::Copy);
    assert!(copied.is_none());
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn visual_mode_movement_extends_selection_then_copy_clears() {
    let mut process = new_process();
    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::StartSelection);
    process.copy_mode_key(CopyModeKey::Right);
    process.copy_mode_key(CopyModeKey::Right);
    assert!(process.selection_text().is_some());
    let copied = process.copy_mode_key(CopyModeKey::Copy);
    assert!(copied.is_some());
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn visual_g_ends_selection_at_last_non_blank() {
    let mut process = new_process();
    let mut rx = process.subscribe();
    process.process_output_for_test(b"\x1b[2J\x1b[Halpha beta   ");
    while rx.try_recv().is_ok() {}

    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::LineStart);
    process.copy_mode_key(CopyModeKey::StartSelection);
    process.copy_mode_key(CopyModeKey::LastNonBlank);

    let moved = latest_viewport_patch(&mut rx).expect("copy cursor move patch");
    assert_eq!(moved.cursor.col, 9);
    assert_eq!(process.selection_text().as_deref(), Some("alpha beta"));
}

#[test]
fn visual_word_motions_follow_vi_word_boundaries() {
    let mut process = new_process();
    let mut rx = process.subscribe();
    process.process_output_for_test(b"\x1b[2J\x1b[Hone: two_three four");
    while rx.try_recv().is_ok() {}

    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::LineStart);
    process.copy_mode_key(CopyModeKey::WordForward);
    let first = latest_viewport_patch(&mut rx).expect("word forward patch");
    assert_eq!(first.cursor.col, 3);

    process.copy_mode_key(CopyModeKey::WordForward);
    let second = latest_viewport_patch(&mut rx).expect("second word forward patch");
    assert_eq!(second.cursor.col, 5);

    process.copy_mode_key(CopyModeKey::WordEndForward);
    let end = latest_viewport_patch(&mut rx).expect("word end patch");
    assert_eq!(end.cursor.col, 13);

    process.copy_mode_key(CopyModeKey::WordBackward);
    let backward = latest_viewport_patch(&mut rx).expect("word backward patch");
    assert_eq!(backward.cursor.col, 5);
}

#[test]
fn visual_big_word_motion_uses_whitespace_boundaries() {
    let mut process = new_process();
    let mut rx = process.subscribe();
    process.process_output_for_test(b"\x1b[2J\x1b[Hone: two_three four");
    while rx.try_recv().is_ok() {}

    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::LineStart);
    process.copy_mode_key(CopyModeKey::BigWordForward);
    let moved = latest_viewport_patch(&mut rx).expect("big word forward patch");
    assert_eq!(moved.cursor.col, 5);

    process.copy_mode_key(CopyModeKey::BigWordEndForward);
    let end = latest_viewport_patch(&mut rx).expect("big word end patch");
    assert_eq!(end.cursor.col, 13);
}

#[test]
fn visual_find_and_till_motions_track_single_line_targets() {
    let mut process = new_process();
    let mut rx = process.subscribe();
    process.process_output_for_test(b"\x1b[2J\x1b[Habc def def");
    while rx.try_recv().is_ok() {}

    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::LineStart);
    process.copy_mode_key(CopyModeKey::FindForward('d'));
    let found = latest_viewport_patch(&mut rx).expect("find forward patch");
    assert_eq!(found.cursor.col, 4);

    process.copy_mode_key(CopyModeKey::RepeatFind);
    let repeated = latest_viewport_patch(&mut rx).expect("repeat find patch");
    assert_eq!(repeated.cursor.col, 8);

    process.copy_mode_key(CopyModeKey::TillBackward('c'));
    let till = latest_viewport_patch(&mut rx).expect("till backward patch");
    assert_eq!(till.cursor.col, 3);
}

#[test]
fn line_visual_mode_selects_current_line_then_copy_clears() {
    let mut process = new_process();
    process.process_output_for_test(b"\x1b[2J\x1b[Hhello world");
    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::StartLineSelection);
    assert_eq!(process.selection_text().as_deref(), Some("hello world"));
    let copied = process.copy_mode_key(CopyModeKey::Copy);
    assert_eq!(copied.as_deref(), Some("hello world"));
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn copy_mode_viewport_patch_uses_copy_cursor() {
    let mut process = new_process();
    let mut rx = process.subscribe();
    process.process_output_for_test(b"\x1b[2J\x1b[Hhello world");
    while rx.try_recv().is_ok() {}

    process.enter_copy_mode();
    let first = latest_viewport_patch(&mut rx).expect("enter copy mode patch");
    assert!(first.copy_mode);
    assert!(first.cursor.visible);
    let start_col = first.cursor.col;

    process.copy_mode_key(CopyModeKey::Left);
    let moved = latest_viewport_patch(&mut rx).expect("copy cursor move patch");
    assert!(moved.copy_mode);
    assert_eq!(moved.cursor.col, start_col.saturating_sub(1));
}

#[test]
fn copy_mode_exit_clears_state() {
    let mut process = new_process();
    process.enter_copy_mode();
    process.copy_mode_key(CopyModeKey::Exit);
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn direct_copy_mode_exit_keeps_selection_for_mouse_release() {
    let mut process = new_process();
    process.enter_copy_mode();
    process.set_selection(Some(TermSelectionRange {
        start_col: 0,
        start_row: 0,
        end_col: 1,
        end_row: 0,
        is_block: false,
    }));
    assert!(process.selection_text().is_some());
    process.exit_copy_mode();
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_some());
}

#[test]
fn buffer_mutation_clears_selection() {
    let mut process = new_process();
    process.process_output_for_test(b"\x1b[2J\x1b[Hhello world");
    process.select_line_at(0);
    assert!(process.selection_text().is_some());
    process.process_output_for_test(b"\x1b[Hgoodbye");
    assert!(
        process.selection_text().is_none(),
        "selection should clear after buffer mutation, got: {:?}",
        process.selection_text()
    );
}

struct ViewportPatchProbe {
    cursor: vmux_terminal::event::TermCursor,
    copy_mode: bool,
}

fn latest_viewport_patch(
    rx: &mut tokio::sync::broadcast::Receiver<ServiceMessage>,
) -> Option<ViewportPatchProbe> {
    let mut latest = None;
    while let Ok(msg) = rx.try_recv() {
        if let ServiceMessage::ViewportPatch {
            cursor, copy_mode, ..
        } = msg
        {
            latest = Some(ViewportPatchProbe { cursor, copy_mode });
        }
    }
    latest
}
