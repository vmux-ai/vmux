use vmux_daemon::protocol::CopyModeKey;
use vmux_daemon::session::Session;
use vmux_terminal::event::TermSelectionRange;

fn new_session() -> Session {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    Session::new(shell, String::new(), Vec::new(), 80, 24).expect("spawn session")
}

/// Write input to the PTY and let the reader thread + VTE catch up.
fn write_and_drain(s: &mut Session, bytes: &[u8]) {
    s.write_input(bytes);
    for _ in 0..50 {
        let _ = s.poll();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn set_and_clear_selection() {
    let mut s = new_session();
    s.set_selection(Some(TermSelectionRange {
        start_col: 0,
        start_row: 0,
        end_col: 4,
        end_row: 0,
        is_block: false,
    }));
    assert!(s.selection_text().is_some());
    s.set_selection(None);
    assert!(s.selection_text().is_none());
}

#[test]
fn extend_from_empty_anchors_a_single_cell() {
    let mut s = new_session();
    s.extend_selection_to(3, 1);
    let text = s.selection_text().unwrap_or_default();
    // Single-cell selection on a blank cell == "" after trailing-space strip.
    assert!(text.chars().count() <= 1, "got {text:?}");
}

#[test]
fn select_line_strips_trailing_blanks() {
    let mut s = new_session();
    write_and_drain(&mut s, b"hello world\n");
    let mut found = false;
    for row in 0..24u16 {
        s.select_line_at(row);
        if let Some(t) = s.selection_text()
            && t.contains("hello world")
        {
            assert!(!t.ends_with(' '), "got {t:?}");
            found = true;
            break;
        }
    }
    assert!(found, "did not find 'hello world' in any row");
}

#[test]
fn select_word_walks_word_chars() {
    let mut s = new_session();
    write_and_drain(&mut s, b"foo_bar baz\n");
    for row in 0..24u16 {
        for col in 0..15u16 {
            s.select_word_at(col, row);
            if let Some(t) = s.selection_text()
                && t == "foo_bar"
            {
                return;
            }
        }
    }
    panic!("did not find foo_bar word selection");
}

#[test]
fn copy_mode_movement_creates_selection() {
    let mut s = new_session();
    s.enter_copy_mode();
    assert!(s.is_copy_mode());
    s.copy_mode_key(CopyModeKey::StartSelection);
    s.copy_mode_key(CopyModeKey::Right);
    s.copy_mode_key(CopyModeKey::Right);
    assert!(s.selection_text().is_some());
    let copied = s.copy_mode_key(CopyModeKey::Copy);
    assert!(copied.is_some());
    assert!(!s.is_copy_mode());
}

#[test]
fn copy_mode_exit_clears_state() {
    let mut s = new_session();
    s.enter_copy_mode();
    s.copy_mode_key(CopyModeKey::Exit);
    assert!(!s.is_copy_mode());
}
