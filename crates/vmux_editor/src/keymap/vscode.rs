use crate::edit::command::{EditCommand, EditMode, Motion};
use crate::keymap::{KeyInput, Keymap};

#[derive(Default)]
pub struct VscodeKeymap;

impl Keymap for VscodeKeymap {
    fn mode(&self) -> EditMode {
        EditMode::Insert
    }
    fn mode_label(&self) -> String {
        String::new()
    }

    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let m = &k.mods;
        let sel = m.shift;
        let mv = |motion: Motion| {
            if sel {
                vec![Select(motion)]
            } else {
                vec![Move(motion)]
            }
        };

        #[cfg(target_os = "macos")]
        let gui = m.meta;
        #[cfg(not(target_os = "macos"))]
        let gui = m.meta || m.ctrl;

        if gui && !m.alt {
            return match k.key.to_ascii_lowercase().as_str() {
                "c" => vec![Yank],
                "x" => vec![Cut],
                "v" => vec![Paste],
                "a" => vec![Move(Motion::DocStart), Select(Motion::DocEnd)],
                "s" => vec![Save],
                "z" if m.shift => vec![Redo],
                "z" => vec![Undo],
                "y" => vec![Redo],
                _ => vec![],
            };
        }

        // macOS readline/emacs navigation on Ctrl (Ctrl is free on macOS; on Linux
        // Ctrl is the GUI modifier handled above). Shift extends the selection.
        #[cfg(target_os = "macos")]
        if m.ctrl && !m.meta && !m.alt {
            return match k.key.as_str() {
                "a" | "A" => mv(Motion::LineStart),
                "e" | "E" => mv(Motion::LineEnd),
                "f" | "F" => mv(Motion::Right),
                "b" | "B" => mv(Motion::Left),
                "n" | "N" => mv(Motion::Down),
                "p" | "P" => mv(Motion::Up),
                "d" | "D" => vec![DeleteForward],
                "h" | "H" => vec![DeleteBack],
                "k" | "K" => vec![DeleteToLineEnd],
                "w" | "W" => vec![DeleteWordBack],
                _ => vec![],
            };
        }

        match k.key.as_str() {
            "ArrowLeft" if m.word() => {
                if sel {
                    vec![Select(Motion::WordPrev)]
                } else {
                    vec![Move(Motion::WordPrev)]
                }
            }
            "ArrowRight" if m.word() => {
                if sel {
                    vec![Select(Motion::WordNext)]
                } else {
                    vec![Move(Motion::WordNext)]
                }
            }
            "ArrowLeft" => mv(Motion::Left),
            "ArrowRight" => mv(Motion::Right),
            "ArrowUp" => mv(Motion::Up),
            "ArrowDown" => mv(Motion::Down),
            "Home" => mv(Motion::LineStart),
            "End" => mv(Motion::LineEnd),
            "PageUp" => mv(Motion::PageUp),
            "PageDown" => mv(Motion::PageDown),
            "Backspace" if m.word() => vec![DeleteWordBack],
            "Backspace" => vec![DeleteBack],
            "Delete" => vec![DeleteForward],
            "Enter" => vec![InsertNewline],
            "Tab" => vec![InsertTab],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mods;

    fn key(k: &str, mods: Mods) -> KeyInput {
        KeyInput {
            key: k.into(),
            mods,
            repeat: false,
        }
    }

    #[test]
    fn arrow_moves_shift_selects() {
        let mut km = VscodeKeymap;
        assert_eq!(
            km.handle(&key("ArrowRight", Mods::default())),
            vec![EditCommand::Move(Motion::Right)]
        );
        let shift = Mods {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("ArrowRight", shift)),
            vec![EditCommand::Select(Motion::Right)]
        );
    }

    #[test]
    fn cmd_chords() {
        let mut km = VscodeKeymap;
        let cmd = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(km.handle(&key("c", cmd)), vec![EditCommand::Yank]);
        assert_eq!(km.handle(&key("s", cmd)), vec![EditCommand::Save]);
        let cmd_shift = Mods {
            meta: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(km.handle(&key("z", cmd_shift)), vec![EditCommand::Redo]);
    }

    #[test]
    fn select_all_composes() {
        let mut km = VscodeKeymap;
        let cmd = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("a", cmd)),
            vec![
                EditCommand::Move(Motion::DocStart),
                EditCommand::Select(Motion::DocEnd)
            ]
        );
    }

    #[test]
    fn word_backspace() {
        let mut km = VscodeKeymap;
        let alt = Mods {
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("Backspace", alt)),
            vec![EditCommand::DeleteWordBack]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ctrl_emacs_nav_macos() {
        let mut km = VscodeKeymap;
        let ctrl = Mods {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("a", ctrl)),
            vec![EditCommand::Move(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&key("e", ctrl)),
            vec![EditCommand::Move(Motion::LineEnd)]
        );
        assert_eq!(
            km.handle(&key("f", ctrl)),
            vec![EditCommand::Move(Motion::Right)]
        );
        assert_eq!(
            km.handle(&key("k", ctrl)),
            vec![EditCommand::DeleteToLineEnd]
        );
        assert_eq!(km.handle(&key("h", ctrl)), vec![EditCommand::DeleteBack]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ctrl_shift_extends_macos() {
        let mut km = VscodeKeymap;
        let cs = Mods {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("A", cs)),
            vec![EditCommand::Select(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&key("E", cs)),
            vec![EditCommand::Select(Motion::LineEnd)]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ctrl_is_not_gui_on_macos() {
        let mut km = VscodeKeymap;
        let ctrl = Mods {
            ctrl: true,
            ..Default::default()
        };
        // Copy is Cmd+C on macOS; Ctrl+C is not a GUI chord (and 'c' has no emacs binding).
        assert_eq!(km.handle(&key("c", ctrl)), Vec::<EditCommand>::new());
        // Cmd+C still copies.
        let meta = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(km.handle(&key("c", meta)), vec![EditCommand::Yank]);
    }
}
