use super::*;
use vmux_command::{AppCommand, LayoutCommand, PaneCommand};

fn map() -> ShortcutMap {
    ShortcutMap {
        bindings: AppCommand::default_shortcuts(),
        chord_timeout_ms: 1000,
    }
}

fn combo(key: KeyCode, ctrl: bool) -> KeyCombo {
    KeyCombo {
        key,
        modifiers: Modifiers {
            ctrl,
            ..Default::default()
        },
    }
}

fn super_combo(key: KeyCode) -> KeyCombo {
    KeyCombo {
        key,
        modifiers: Modifiers {
            super_key: true,
            ..Default::default()
        },
    }
}

#[test]
fn leader_then_h_consumes_and_emits_select_left() {
    let map = map();
    let mut pending = None;
    let now = Instant::now();

    let prefix = decide(&map, &mut pending, combo(KeyCode::KeyG, true), now);
    assert!(matches!(prefix, KeyAction::Consume(None)));
    assert!(pending.is_some());

    let second = decide(&map, &mut pending, combo(KeyCode::KeyH, false), now);
    match second {
        KeyAction::Consume(Some(AppCommand::Layout(LayoutCommand::Pane(
            PaneCommand::SelectLeft,
        )))) => {}
        _ => panic!("expected SelectLeft"),
    }
    assert!(pending.is_none());
}

#[test]
fn bare_escape_detected_only_without_modifiers() {
    assert!(is_bare_escape(&combo(KeyCode::Escape, false)));
    assert!(!is_bare_escape(&combo(KeyCode::Escape, true)));
    assert!(!is_bare_escape(&super_combo(KeyCode::Escape)));
    assert!(!is_bare_escape(&combo(KeyCode::KeyH, false)));
}

#[test]
fn command_bar_dismiss_accepts_escape_and_ctrl_c() {
    assert!(is_command_bar_dismiss_combo(&combo(KeyCode::Escape, false)));
    assert!(is_command_bar_dismiss_combo(&combo(KeyCode::KeyC, true)));
    assert!(!is_command_bar_dismiss_combo(&combo(KeyCode::KeyC, false)));
    assert!(!is_command_bar_dismiss_combo(&super_combo(KeyCode::KeyC)));
}

#[test]
fn bare_key_without_pending_passes_through() {
    let map = map();
    let mut pending = None;
    let action = decide(
        &map,
        &mut pending,
        combo(KeyCode::KeyH, false),
        Instant::now(),
    );
    assert!(matches!(action, KeyAction::PassThrough));
}

#[test]
fn consumed_shortcut_wakes_and_queues_command() {
    let mut woke = false;
    let mut queued = Vec::new();

    let consumed = handle_key_action(
        KeyAction::Consume(Some(AppCommand::Layout(LayoutCommand::Pane(
            PaneCommand::SelectLeft,
        )))),
        || woke = true,
        |command| queued.push(command),
    );

    assert!(consumed);
    assert!(woke);
    assert!(matches!(
        queued.as_slice(),
        [AppCommand::Layout(LayoutCommand::Pane(
            PaneCommand::SelectLeft
        ))]
    ));
}

#[test]
fn expired_prefix_does_not_consume_second_key() {
    let map = map();
    let mut pending = Some((combo(KeyCode::KeyG, true), Instant::now()));
    let later = Instant::now() + Duration::from_millis(2000);
    let action = decide(&map, &mut pending, combo(KeyCode::KeyH, false), later);
    assert!(matches!(action, KeyAction::PassThrough));
    assert!(pending.is_none());
}

#[test]
fn native_command_bar_shortcuts_are_consumed_before_cef() {
    use vmux_command::{BrowserBarCommand, BrowserCommand};

    let map = map();
    let mut pending = None;
    let now = Instant::now();
    let shortcuts = [
        (
            super_combo(KeyCode::KeyK),
            BrowserBarCommand::OpenCommandBar,
        ),
        (
            super_combo(KeyCode::KeyL),
            BrowserBarCommand::OpenPageInCommandBar,
        ),
        (super_combo(KeyCode::Slash), BrowserBarCommand::OpenPathBar),
    ];

    for (pressed, expected) in shortcuts {
        let action = decide(&map, &mut pending, pressed, now);
        match action {
            KeyAction::Consume(Some(AppCommand::Browser(BrowserCommand::Bar(cmd)))) => {
                assert_eq!(cmd, expected);
            }
            _ => panic!("expected command bar shortcut"),
        }
    }
}
