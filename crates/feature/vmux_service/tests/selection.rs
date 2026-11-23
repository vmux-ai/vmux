use std::sync::mpsc as std_mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use tokio::sync::{broadcast, mpsc};
use vmux_api::protocol::{CopyModeKey, ProcessId};
use vmux_ecs::event::{TermCursor, TermSelectionRange};
use vmux_process::{ProcessLaunch, ProcessPlugin, ProcessRuntime, ProcessUpdate};

struct ProcessHarness {
    id: ProcessId,
    runtime: ProcessRuntime,
    rt: tokio::runtime::Runtime,
    stop: Option<std_mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl ProcessHarness {
    fn start(cols: u16, rows: u16) -> (Self, broadcast::Receiver<ProcessUpdate>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio test runtime");
        let (wake_tx, wake_rx) = mpsc::unbounded_channel();
        let (runtime, inboxes) = ProcessRuntime::new(wake_tx);
        let (stop_tx, stop_rx) = std_mpsc::channel();
        let thread = std::thread::spawn(move || {
            let mut wake_rx = wake_rx;
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, ProcessPlugin));
            app.world_mut()
                .spawn((Name::new("vmux process runtime"), inboxes));
            loop {
                while wake_rx.try_recv().is_ok() {}
                app.update();
                match stop_rx.recv_timeout(Duration::from_millis(1)) {
                    Ok(()) | Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std_mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
            app.world_mut()
                .run_schedule(vmux_ecs::service::ShutdownComplete);
            app.update();
        });
        let id = ProcessId::new();
        let created = rt
            .block_on(runtime.create(ProcessLaunch {
                id,
                command: "/bin/sh".into(),
                args: vec!["-c".into(), "exec sleep 60".into()],
                cwd: String::new(),
                env: vec![("LC_ALL".into(), "C".into())],
                cols,
                rows,
                keep_after_exit: false,
            }))
            .expect("spawn process");
        (
            Self {
                id: created.id,
                runtime,
                rt,
                stop: Some(stop_tx),
                thread: Some(thread),
            },
            created.updates,
        )
    }

    fn start_default() -> (Self, broadcast::Receiver<ProcessUpdate>) {
        Self::start(80, 24)
    }

    fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.rt.block_on(future)
    }

    fn write_and_drain(&self, bytes: &[u8]) {
        self.block_on(self.runtime.input(self.id, bytes.to_vec()))
            .expect("write input");
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn selection_text(&self) -> Option<String> {
        let text = self
            .block_on(self.runtime.selection_text(self.id))
            .expect("selection_text");
        if text.is_empty() { None } else { Some(text) }
    }

    fn is_copy_mode(&self) -> bool {
        self.block_on(self.runtime.is_copy_mode(self.id))
            .expect("is_copy_mode")
    }

    fn latest_viewport_patch(
        rx: &mut broadcast::Receiver<ProcessUpdate>,
    ) -> Option<(TermCursor, bool)> {
        let mut latest = None;
        while let Ok(msg) = rx.try_recv() {
            if let ProcessUpdate::Viewport(patch) = msg {
                latest = Some((patch.cursor, patch.copy_mode));
            }
        }
        latest
    }
}

impl Drop for ProcessHarness {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn set_and_clear_selection() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hhello".to_vec()),
        )
        .unwrap();
    process
        .block_on(process.runtime.set_selection(
            process.id,
            Some(TermSelectionRange {
                start_col: 0,
                start_row: 0,
                end_col: 4,
                end_row: 0,
                is_block: false,
            }),
        ))
        .unwrap();
    assert_eq!(process.selection_text().as_deref(), Some("hello"));
    process
        .block_on(process.runtime.set_selection(process.id, None))
        .unwrap();
    assert!(process.selection_text().is_none());
}

#[test]
fn extend_from_empty_anchors_a_single_cell() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(process.runtime.extend_selection(process.id, 3, 1))
        .unwrap();
    let text = process.selection_text().unwrap_or_default();
    assert!(text.chars().count() <= 1, "got {text:?}");
}

#[test]
fn select_line_strips_trailing_blanks() {
    let (process, _) = ProcessHarness::start_default();
    process.write_and_drain(b"hello world\n");
    let mut found = false;
    for row in 0..24u16 {
        process
            .block_on(process.runtime.select_line(process.id, row))
            .unwrap();
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
    let (process, _) = ProcessHarness::start_default();
    process.write_and_drain(b"foo_bar baz\n");
    for row in 0..24u16 {
        for col in 0..15u16 {
            process
                .block_on(process.runtime.select_word(process.id, col, row))
                .unwrap();
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
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    assert!(process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn copy_mode_movement_without_visual_keeps_selection_empty() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::Right),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::Right),
        )
        .unwrap();
    let copied = process
        .block_on(process.runtime.copy_mode_key(process.id, CopyModeKey::Copy))
        .unwrap();
    assert!(copied.is_none());
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn visual_mode_movement_extends_selection_then_copy_clears() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Habcdef".to_vec()),
        )
        .unwrap();
    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::LineStart),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::StartSelection),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::Right),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::Right),
        )
        .unwrap();
    assert!(process.selection_text().is_some());
    let copied = process
        .block_on(process.runtime.copy_mode_key(process.id, CopyModeKey::Copy))
        .unwrap();
    assert!(copied.is_some());
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn visual_g_ends_selection_at_last_non_blank() {
    let (process, mut rx) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Halpha beta   ".to_vec()),
        )
        .unwrap();
    while rx.try_recv().is_ok() {}

    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::LineStart),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::StartSelection),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::LastNonBlank),
        )
        .unwrap();

    let (cursor, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("copy cursor move");
    assert_eq!(cursor.col, 9);
    assert_eq!(process.selection_text().as_deref(), Some("alpha beta"));
}

#[test]
fn visual_word_motions_follow_vi_word_boundaries() {
    let (process, mut rx) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hone: two_three four".to_vec()),
        )
        .unwrap();
    while rx.try_recv().is_ok() {}

    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::LineStart),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::WordForward),
        )
        .unwrap();
    let (first, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("word forward");
    assert_eq!(first.col, 3);

    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::WordForward),
        )
        .unwrap();
    let (second, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("second word forward");
    assert_eq!(second.col, 5);

    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::WordEndForward),
        )
        .unwrap();
    let (end, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("word end");
    assert_eq!(end.col, 13);

    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::WordBackward),
        )
        .unwrap();
    let (backward, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("word backward");
    assert_eq!(backward.col, 5);
}

#[test]
fn visual_big_word_motion_uses_whitespace_boundaries() {
    let (process, mut rx) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hone: two_three four".to_vec()),
        )
        .unwrap();
    while rx.try_recv().is_ok() {}

    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::LineStart),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::BigWordForward),
        )
        .unwrap();
    let (moved, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("big word forward");
    assert_eq!(moved.col, 5);

    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::BigWordEndForward),
        )
        .unwrap();
    let (end, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("big word end");
    assert_eq!(end.col, 13);
}

#[test]
fn visual_find_and_till_motions_track_single_line_targets() {
    let (process, mut rx) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Habc def def".to_vec()),
        )
        .unwrap();
    while rx.try_recv().is_ok() {}

    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::LineStart),
        )
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::FindForward('d')),
        )
        .unwrap();
    let (found, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("find forward");
    assert_eq!(found.col, 4);

    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::RepeatFind),
        )
        .unwrap();
    let (repeated, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("repeat find");
    assert_eq!(repeated.col, 8);

    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::TillBackward('c')),
        )
        .unwrap();
    let (till, _) = ProcessHarness::latest_viewport_patch(&mut rx).expect("till backward");
    assert_eq!(till.col, 3);
}

#[test]
fn line_visual_mode_selects_current_line_then_copy_clears() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hhello world".to_vec()),
        )
        .unwrap();
    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::StartLineSelection),
        )
        .unwrap();
    assert_eq!(process.selection_text().as_deref(), Some("hello world"));
    let copied = process
        .block_on(process.runtime.copy_mode_key(process.id, CopyModeKey::Copy))
        .unwrap();
    assert_eq!(copied.as_deref(), Some("hello world"));
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn line_visual_mode_scrolls_up_past_viewport_top() {
    let (process, _) = ProcessHarness::start(12, 3);
    process
        .block_on(process.runtime.feed_output(
            process.id,
            b"\x1b[2J\x1b[Hone\r\ntwo\r\nthree\r\nfour\r\nfive".to_vec(),
        ))
        .unwrap();

    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::StartLineSelection),
        )
        .unwrap();
    for _ in 0..3 {
        process
            .block_on(process.runtime.copy_mode_key(process.id, CopyModeKey::Up))
            .unwrap();
    }

    assert_eq!(
        process.selection_text().as_deref(),
        Some("two\nthree\nfour\nfive")
    );
}

#[test]
fn copy_mode_selection_survives_screen_redraw() {
    let (process, _) = ProcessHarness::start(12, 3);
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hone\r\ntwo\r\nthree".to_vec()),
        )
        .unwrap();

    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .copy_mode_key(process.id, CopyModeKey::StartLineSelection),
        )
        .unwrap();
    process
        .block_on(process.runtime.copy_mode_key(process.id, CopyModeKey::Up))
        .unwrap();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[Htwo\r\nthree\r\nfour".to_vec()),
        )
        .unwrap();

    assert!(process.selection_text().is_some());
}

#[test]
fn copy_mode_viewport_patch_uses_copy_cursor() {
    let (process, mut rx) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hhello world".to_vec()),
        )
        .unwrap();
    while rx.try_recv().is_ok() {}

    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    let (first_cursor, first_copy_mode) =
        ProcessHarness::latest_viewport_patch(&mut rx).expect("enter copy mode patch");
    assert!(first_copy_mode);
    assert!(first_cursor.visible);
    let start_col = first_cursor.col;

    process
        .block_on(process.runtime.copy_mode_key(process.id, CopyModeKey::Left))
        .unwrap();
    let (moved_cursor, moved_copy_mode) =
        ProcessHarness::latest_viewport_patch(&mut rx).expect("copy cursor move patch");
    assert!(moved_copy_mode);
    assert_eq!(moved_cursor.col, start_col.saturating_sub(1));
}

#[test]
fn copy_mode_exit_clears_state() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(process.runtime.copy_mode_key(process.id, CopyModeKey::Exit))
        .unwrap();
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_none());
}

#[test]
fn direct_copy_mode_exit_keeps_selection_for_mouse_release() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hab".to_vec()),
        )
        .unwrap();
    process
        .block_on(process.runtime.enter_copy_mode(process.id))
        .unwrap();
    process
        .block_on(process.runtime.set_selection(
            process.id,
            Some(TermSelectionRange {
                start_col: 0,
                start_row: 0,
                end_col: 1,
                end_row: 0,
                is_block: false,
            }),
        ))
        .unwrap();
    assert!(process.selection_text().is_some());
    process
        .block_on(process.runtime.exit_copy_mode(process.id))
        .unwrap();
    assert!(!process.is_copy_mode());
    assert!(process.selection_text().is_some());
}

#[test]
fn buffer_mutation_clears_selection() {
    let (process, _) = ProcessHarness::start_default();
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[2J\x1b[Hhello world".to_vec()),
        )
        .unwrap();
    process
        .block_on(process.runtime.select_line(process.id, 0))
        .unwrap();
    assert!(process.selection_text().is_some());
    process
        .block_on(
            process
                .runtime
                .feed_output(process.id, b"\x1b[Hgoodbye".to_vec()),
        )
        .unwrap();
    assert!(
        process.selection_text().is_none(),
        "selection should clear after buffer mutation, got: {:?}",
        process.selection_text()
    );
}
