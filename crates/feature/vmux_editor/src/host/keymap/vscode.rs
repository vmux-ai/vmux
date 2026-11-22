use crate::edit::command::{EditCommand, EditMode, Motion, Operator, Target, VerticalDirection};
use crate::keymap::{KeyInput, Keymap};

use EditCommand::*;

#[derive(Default)]
pub struct VscodeKeymap;

impl VscodeKeymap {
    fn selection_op(operator: Operator) -> EditCommand {
        EditCommand::Op {
            operator,
            target: Target::Selection,
            register: None,
        }
    }

    #[cfg(target_os = "macos")]
    fn delete_to_line_end() -> EditCommand {
        EditCommand::Op {
            operator: Operator::Delete,
            target: Target::Motion(Motion::LineEnd, 1),
            register: None,
        }
    }

    fn move_or_select(motion: Motion, select: bool) -> Vec<EditCommand> {
        if select {
            vec![Select(motion)]
        } else {
            vec![Move(motion)]
        }
    }

    fn handle_gui_chord(k: &KeyInput) -> Option<Vec<EditCommand>> {
        let m = &k.mods;
        #[cfg(target_os = "macos")]
        let gui = m.meta;
        #[cfg(not(target_os = "macos"))]
        let gui = m.meta || m.ctrl;
        if !gui {
            return None;
        }
        if m.alt && !m.shift {
            return match k.key.as_str() {
                "ArrowUp" => Some(vec![AddCaretVertically(VerticalDirection::Up)]),
                "ArrowDown" => Some(vec![AddCaretVertically(VerticalDirection::Down)]),
                _ => None,
            };
        }
        if m.shift && !m.alt {
            match k.key.as_str() {
                "[" | "{" => return Some(vec![FoldClose]),
                "]" | "}" => return Some(vec![FoldOpen]),
                "0" | ")" => return Some(vec![FoldAll]),
                "j" | "J" => return Some(vec![UnfoldAll]),
                _ => {}
            }
        }
        if !m.alt {
            return match k.key.to_ascii_lowercase().as_str() {
                "c" => Some(vec![Self::selection_op(Operator::Yank)]),
                "x" => Some(vec![Self::selection_op(Operator::Delete)]),
                "v" => Some(vec![EditCommand::Paste]),
                "a" => Some(vec![Move(Motion::DocStart), Select(Motion::DocEnd)]),
                "s" => Some(vec![Save]),
                "z" if m.shift => Some(vec![Redo]),
                "z" => Some(vec![Undo]),
                "y" => Some(vec![Redo]),
                _ => None,
            };
        }
        None
    }

    #[cfg(target_os = "macos")]
    fn handle_macos_chord(k: &KeyInput) -> Option<Vec<EditCommand>> {
        let m = &k.mods;
        let sel = m.shift;
        if m.meta && !m.ctrl && !m.alt {
            return match k.key.as_str() {
                "ArrowLeft" => Some(Self::move_or_select(Motion::LineStart, sel)),
                "ArrowRight" => Some(Self::move_or_select(Motion::LineEnd, sel)),
                "ArrowUp" => Some(Self::move_or_select(Motion::DocStart, sel)),
                "ArrowDown" => Some(Self::move_or_select(Motion::DocEnd, sel)),
                _ => None,
            };
        }
        if m.ctrl && !m.meta && !m.alt {
            return match k.key.as_str() {
                "a" | "A" => Some(Self::move_or_select(Motion::LineStart, sel)),
                "e" | "E" => Some(Self::move_or_select(Motion::LineEnd, sel)),
                "f" | "F" => Some(Self::move_or_select(Motion::Right, sel)),
                "b" | "B" => Some(Self::move_or_select(Motion::Left, sel)),
                "n" | "N" => Some(Self::move_or_select(Motion::Down, sel)),
                "p" | "P" => Some(Self::move_or_select(Motion::Up, sel)),
                "d" | "D" => Some(vec![DeleteForward]),
                "h" | "H" => Some(vec![DeleteBack]),
                "k" | "K" => Some(vec![Self::delete_to_line_end()]),
                "w" | "W" => Some(vec![DeleteWordBack]),
                _ => None,
            };
        }
        None
    }

    fn handle_navigation_key(k: &KeyInput) -> Vec<EditCommand> {
        let m = &k.mods;
        let sel = m.shift;
        match k.key.as_str() {
            "ArrowLeft" if m.word() => Self::move_or_select(Motion::WordPrev, sel),
            "ArrowRight" if m.word() => Self::move_or_select(Motion::WordNext, sel),
            "ArrowLeft" => Self::move_or_select(Motion::Left, sel),
            "ArrowRight" => Self::move_or_select(Motion::Right, sel),
            "ArrowUp" => Self::move_or_select(Motion::Up, sel),
            "ArrowDown" => Self::move_or_select(Motion::Down, sel),
            "Home" => Self::move_or_select(Motion::LineStart, sel),
            "End" => Self::move_or_select(Motion::LineEnd, sel),
            "PageUp" => Self::move_or_select(Motion::PageUp, sel),
            "PageDown" => Self::move_or_select(Motion::PageDown, sel),
            "Backspace" if m.word() => vec![DeleteWordBack],
            "Backspace" => vec![DeleteBack],
            "Delete" => vec![DeleteForward],
            "Enter" => vec![InsertNewline],
            "Tab" => vec![InsertTab],
            _ => vec![],
        }
    }
}

impl Keymap for VscodeKeymap {
    fn mode(&self) -> EditMode {
        EditMode::Insert
    }

    fn mode_label(&self) -> String {
        String::new()
    }

    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if let Some(command) = k.lsp_action() {
            return vec![command];
        }
        if k.mods.ctrl && !k.mods.meta && !k.mods.alt && k.key == " " {
            return vec![TriggerCompletion];
        }
        if k.key == "Escape" {
            return vec![CollapseCarets];
        }
        if let Some(commands) = Self::handle_gui_chord(k) {
            return commands;
        }
        #[cfg(target_os = "macos")]
        if let Some(commands) = Self::handle_macos_chord(k) {
            return commands;
        }
        Self::handle_navigation_key(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mods;

    impl VscodeKeymap {
        fn key(k: &str, mods: Mods) -> KeyInput {
            KeyInput {
                key: k.into(),
                mods,
                repeat: false,
            }
        }
    }

    #[test]
    fn arrow_moves_shift_selects() {
        let mut km = VscodeKeymap;
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowRight", Mods::default())),
            vec![EditCommand::Move(Motion::Right)]
        );
        let shift = Mods {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowRight", shift)),
            vec![EditCommand::Select(Motion::Right)]
        );
    }

    #[test]
    fn the_gui_alt_arrows_add_a_caret() {
        let mut km = VscodeKeymap;
        let gui_alt = Mods {
            meta: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowDown", gui_alt)),
            vec![EditCommand::AddCaretVertically(VerticalDirection::Down)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowUp", gui_alt)),
            vec![EditCommand::AddCaretVertically(VerticalDirection::Up)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowDown", Mods::default())),
            vec![EditCommand::Move(Motion::Down)]
        );
    }

    #[test]
    fn cmd_chords() {
        let mut km = VscodeKeymap;
        let cmd = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("c", cmd)),
            vec![VscodeKeymap::selection_op(Operator::Yank)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("s", cmd)),
            vec![EditCommand::Save]
        );
        let cmd_shift = Mods {
            meta: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("z", cmd_shift)),
            vec![EditCommand::Redo]
        );
    }

    #[test]
    fn cmd_shift_bracket_folds() {
        let mut km = VscodeKeymap;
        let cs = Mods {
            meta: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("[", cs)),
            vec![EditCommand::FoldClose]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("]", cs)),
            vec![EditCommand::FoldOpen]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("J", cs)),
            vec![EditCommand::UnfoldAll]
        );
    }

    #[test]
    fn select_all_composes() {
        let mut km = VscodeKeymap;
        let cmd = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("a", cmd)),
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
            km.handle(&VscodeKeymap::key("Backspace", alt)),
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
            km.handle(&VscodeKeymap::key("a", ctrl)),
            vec![EditCommand::Move(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("e", ctrl)),
            vec![EditCommand::Move(Motion::LineEnd)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("f", ctrl)),
            vec![EditCommand::Move(Motion::Right)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("n", ctrl)),
            vec![EditCommand::Move(Motion::Down)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("p", ctrl)),
            vec![EditCommand::Move(Motion::Up)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("k", ctrl)),
            vec![VscodeKeymap::delete_to_line_end()]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("h", ctrl)),
            vec![EditCommand::DeleteBack]
        );
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
            km.handle(&VscodeKeymap::key("A", cs)),
            vec![EditCommand::Select(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("E", cs)),
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
        assert_eq!(
            km.handle(&VscodeKeymap::key("c", ctrl)),
            Vec::<EditCommand>::new()
        );
        let meta = Mods {
            meta: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("c", meta)),
            vec![VscodeKeymap::selection_op(Operator::Yank)]
        );
    }

    #[test]
    fn lsp_actions() {
        let mut km = VscodeKeymap;
        assert_eq!(
            km.handle(&VscodeKeymap::key("F12", Mods::default())),
            vec![EditCommand::GotoDefinition]
        );
        let shift = Mods {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key("F12", shift)),
            vec![EditCommand::FindReferences]
        );
        let ctrl = Mods {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(
            km.handle(&VscodeKeymap::key(" ", ctrl)),
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
            km.handle(&VscodeKeymap::key("ArrowLeft", cmd)),
            vec![EditCommand::Move(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowRight", cmd)),
            vec![EditCommand::Move(Motion::LineEnd)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowUp", cmd)),
            vec![EditCommand::Move(Motion::DocStart)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowDown", cmd)),
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
            km.handle(&VscodeKeymap::key("ArrowLeft", cs)),
            vec![EditCommand::Select(Motion::LineStart)]
        );
        assert_eq!(
            km.handle(&VscodeKeymap::key("ArrowDown", cs)),
            vec![EditCommand::Select(Motion::DocEnd)]
        );
    }
}
