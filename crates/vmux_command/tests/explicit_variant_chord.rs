use bevy::input::keyboard::KeyCode;
use vmux_macro::{DefaultShortcuts, OsSubMenu};

use shortcut::{KeyCombo, Modifiers, Shortcut};
pub use vmux_command::shortcut;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Right,
    Left,
}

#[derive(OsSubMenu, DefaultShortcuts, Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum Sample {
    #[menu(id = "sample_pane", label = "Pane")]
    #[shortcut(chord = "Ctrl+g, %", variant = "Pane { direction: Direction::Right }")]
    Pane { direction: Direction },
}

#[test]
fn explicit_variant_chord_registers_under_specified_instantiation() {
    let extras: Vec<(Shortcut, Sample)> = Sample::extra_chord_bindings();
    assert_eq!(extras.len(), 1);
    let (shortcut, variant) = &extras[0];
    assert_eq!(
        *variant,
        Sample::Pane {
            direction: Direction::Right
        }
    );
    assert_eq!(
        *shortcut,
        Shortcut::Chord(
            KeyCombo {
                key: KeyCode::KeyG,
                modifiers: Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            },
            KeyCombo {
                key: KeyCode::Digit5,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
        )
    );
}

#[test]
fn default_shortcuts_is_empty_when_only_extra_chords() {
    let defaults = Sample::default_shortcuts();
    assert!(defaults.is_empty());
}

#[derive(DefaultShortcuts, Debug, Clone, PartialEq, Eq)]
enum Root {
    Inner(Sample),
}

#[test]
fn root_extra_chord_bindings_aggregates_children() {
    let bindings = Root::extra_chord_bindings();
    assert_eq!(bindings.len(), 1);
    let (shortcut, variant) = &bindings[0];
    assert!(matches!(
        variant,
        Root::Inner(Sample::Pane {
            direction: Direction::Right
        })
    ));
    assert_eq!(
        *shortcut,
        Shortcut::Chord(
            KeyCombo {
                key: KeyCode::KeyG,
                modifiers: Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            },
            KeyCombo {
                key: KeyCode::Digit5,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
        )
    );
}
