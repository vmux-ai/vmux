use crate::edit::command::{EditCommand, EditMode, Motion, Operator, Target};
use crate::keymap::{KeyInput, Keymap};

#[derive(Default)]
pub struct VscodeKeymap;

fn selection_op(operator: Operator) -> EditCommand {
    EditCommand::Op {
        operator,
        target: Target::Selection,
        register: None,
    }
}

/// Only the macOS emacs-style `Ctrl-K` binding uses this, so it is dead code elsewhere.
#[cfg(target_os = "macos")]
fn delete_to_line_end() -> EditCommand {
    EditCommand::Op {
        operator: Operator::Delete,
        target: Target::Motion(Motion::LineEnd, 1),
        register: None,
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
        if gui && m.shift && !m.alt {
            match k.key.as_str() {
                "[" | "{" => return vec![FoldClose],
                "]" | "}" => return vec![FoldOpen],
                "0" | ")" => return vec![FoldAll],
                "j" | "J" => return vec![UnfoldAll],
                _ => {}
            }
        }
        if gui && !m.alt {
            let cmd = match k.key.to_ascii_lowercase().as_str() {
                "c" => Some(vec![selection_op(Operator::Yank)]),
                "x" => Some(vec![selection_op(Operator::Delete)]),
                "v" => Some(vec![Put {
                    before: true,
                    count: 1,
                    register: None,
                }]),
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
                "k" | "K" => Some(vec![delete_to_line_end()]),
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
#[path = "vscode.test.rs"]
mod tests;
