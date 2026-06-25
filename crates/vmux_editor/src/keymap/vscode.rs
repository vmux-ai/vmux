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

        if m.ctrl && !m.meta && !m.alt && k.key == " " {
            return vec![TriggerCompletion];
        }

        #[cfg(target_os = "macos")]
        let gui = m.meta;
        #[cfg(not(target_os = "macos"))]
        let gui = m.meta || m.ctrl;
        if gui && !m.alt {
            let cmd = match k.key.to_ascii_lowercase().as_str() {
                "c" => Some(vec![Yank]),
                "x" => Some(vec![Cut]),
                "v" => Some(vec![Paste]),
                "a" => Some(vec![Move(Motion::DocStart), Select(Motion::DocEnd)]),
                "s" => Some(vec![Save]),
                "z" if m.shift => Some(vec![Redo]),
                "z" => Some(vec![Undo]),
                "y" => Some(vec![Redo]),
                _ => None,
            };
            if let Some(cmd) = cmd {
                return cmd;
            }
        }

        #[cfg(target_os = "macos")]
        if m.meta && !m.ctrl && !m.alt {
            match k.key.as_str() {
                "ArrowLeft" => return mv(Motion::LineStart),
                "ArrowRight" => return mv(Motion::LineEnd),
                "ArrowUp" => return mv(Motion::DocStart),
                "ArrowDown" => return mv(Motion::DocEnd),
                _ => {}
            }
        }

        #[cfg(target_os = "macos")]
        if m.ctrl && !m.meta && !m.alt {
            let cmd = match k.key.as_str() {
                "a" | "A" => Some(mv(Motion::LineStart)),
                "e" | "E" => Some(mv(Motion::LineEnd)),
                "f" | "F" => Some(mv(Motion::Right)),
                "b" | "B" => Some(mv(Motion::Left)),
                "n" | "N" => Some(mv(Motion::Down)),
                "p" | "P" => Some(mv(Motion::Up)),
                "d" | "D" => Some(vec![DeleteForward]),
                "h" | "H" => Some(vec![DeleteBack]),
                "k" | "K" => Some(vec![DeleteToLineEnd]),
                "w" | "W" => Some(vec![DeleteWordBack]),
                _ => None,
            };
            if let Some(cmd) = cmd {
                return cmd;
            }
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
            "F12" if m.shift => vec![FindReferences],
            "F12" => vec![GotoDefinition],
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
        assert_eq!(km.handle(&key("c", ctrl)), Vec::<EditCommand>::new());
        let meta = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(km.handle(&key("c", meta)), vec![EditCommand::Yank]);
    }

    #[test]
    fn lsp_actions() {
        let mut km = VscodeKeymap;
        assert_eq!(
            km.handle(&key("F12", Mods::default())),
            vec![EditCommand::GotoDefinition]
        );
        let shift = Mods {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("F12", shift)),
            vec![EditCommand::FindReferences]
        );
        let ctrl = Mods {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key(" ", ctrl)),
            vec![EditCommand::TriggerCompletion]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cmd_arrow_line_doc_nav_macos() {
        let mut km = VscodeKeymap;
        let cmd = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("ArrowLeft", cmd)),
            vec![EditCommand::Move(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&key("ArrowRight", cmd)),
            vec![EditCommand::Move(Motion::LineEnd)]
        );
        assert_eq!(
            km.handle(&key("ArrowUp", cmd)),
            vec![EditCommand::Move(Motion::DocStart)]
        );
        assert_eq!(
            km.handle(&key("ArrowDown", cmd)),
            vec![EditCommand::Move(Motion::DocEnd)]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cmd_shift_arrow_selects_macos() {
        let mut km = VscodeKeymap;
        let cs = Mods {
            meta: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&key("ArrowLeft", cs)),
            vec![EditCommand::Select(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&key("ArrowDown", cs)),
            vec![EditCommand::Select(Motion::DocEnd)]
        );
    }
}
