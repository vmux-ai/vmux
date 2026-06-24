use crate::edit::command::{EditCommand, EditMode, Motion};
use crate::keymap::{KeyInput, Keymap};

#[derive(Default)]
pub struct VimKeymap {
    mode: EditMode,
    count: Option<usize>,
    pending_op: Option<char>,
    g_pending: bool,
    ex: Option<String>,
}

fn rep(cmd: EditCommand, n: usize) -> Vec<EditCommand> {
    std::iter::repeat_n(cmd, n.max(1)).collect()
}

fn motion_for(key: &str) -> Option<Motion> {
    Some(match key {
        "h" => Motion::Left,
        "l" => Motion::Right,
        "j" => Motion::Down,
        "k" => Motion::Up,
        "w" => Motion::WordNext,
        "b" => Motion::WordPrev,
        "e" => Motion::WordEnd,
        "0" => Motion::LineStart,
        "^" => Motion::FirstNonBlank,
        "$" => Motion::LineEnd,
        _ => return None,
    })
}

impl VimKeymap {
    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }
    fn reset(&mut self) {
        self.count = None;
        self.pending_op = None;
        self.g_pending = false;
    }

    fn normal(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let key = k.key.as_str();

        if key.len() == 1 {
            let c = key.chars().next().unwrap();
            if c.is_ascii_digit() && !(c == '0' && self.count.is_none()) {
                let d = c as usize - '0' as usize;
                self.count = Some(self.count.unwrap_or(0) * 10 + d);
                return vec![];
            }
        }

        if let Some(op) = self.pending_op {
            self.pending_op = None;
            let _n = self.take_count();
            if key.len() == 1 && key.starts_with(op) {
                return match op {
                    'd' => vec![DeleteLine],
                    'y' => vec![Move(Motion::LineStart), Select(Motion::LineEnd), Yank],
                    'c' => {
                        self.mode = EditMode::Insert;
                        vec![
                            Move(Motion::LineStart),
                            DeleteToLineEnd,
                            SetMode(EditMode::Insert),
                        ]
                    }
                    _ => vec![],
                };
            }
            if let Some(m) = motion_for(key) {
                return match op {
                    'd' => vec![DeleteRange(m)],
                    'y' => vec![YankRange(m)],
                    'c' => {
                        self.mode = EditMode::Insert;
                        vec![DeleteRange(m), SetMode(EditMode::Insert)]
                    }
                    _ => vec![],
                };
            }
            return vec![];
        }

        if self.g_pending {
            self.g_pending = false;
            if key == "g" {
                self.count = None;
                return vec![Move(Motion::DocStart)];
            }
            return vec![];
        }

        if key == "r" && k.mods.ctrl {
            let n = self.take_count();
            return rep(Redo, n);
        }

        if let Some(m) = motion_for(key) {
            let n = self.take_count();
            return rep(Move(m), n);
        }

        match key {
            "g" => {
                self.g_pending = true;
                vec![]
            }
            "G" => {
                let cmd = match self.count.take() {
                    Some(n) => Move(Motion::GotoLine(n.saturating_sub(1) as u32)),
                    None => Move(Motion::DocEnd),
                };
                vec![cmd]
            }
            "i" => {
                self.mode = EditMode::Insert;
                vec![SetMode(EditMode::Insert)]
            }
            "a" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::Right), SetMode(EditMode::Insert)]
            }
            "I" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::FirstNonBlank), SetMode(EditMode::Insert)]
            }
            "A" => {
                self.mode = EditMode::Insert;
                vec![Move(Motion::LineEnd), SetMode(EditMode::Insert)]
            }
            "o" => {
                self.mode = EditMode::Insert;
                vec![
                    Move(Motion::LineEnd),
                    InsertNewline,
                    SetMode(EditMode::Insert),
                ]
            }
            "O" => {
                self.mode = EditMode::Insert;
                vec![
                    Move(Motion::LineStart),
                    InsertNewline,
                    Move(Motion::Up),
                    SetMode(EditMode::Insert),
                ]
            }
            "x" => {
                let n = self.take_count();
                rep(DeleteForward, n)
            }
            "p" => vec![Paste],
            "P" => vec![PasteBefore],
            "u" => {
                let n = self.take_count();
                rep(Undo, n)
            }
            "d" | "c" | "y" => {
                self.pending_op = key.chars().next();
                vec![]
            }
            "v" => {
                self.mode = EditMode::Visual;
                vec![SetMode(EditMode::Visual)]
            }
            "V" => {
                self.mode = EditMode::VisualLine;
                vec![SetMode(EditMode::VisualLine)]
            }
            ":" => {
                self.ex = Some(String::new());
                vec![]
            }
            "Escape" => {
                self.reset();
                vec![]
            }
            _ => vec![],
        }
    }

    fn visual(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        let key = k.key.as_str();
        if let Some(m) = motion_for(key) {
            return vec![Select(m)];
        }
        match key {
            "d" | "x" => {
                self.mode = EditMode::Normal;
                vec![DeleteSelection, SetMode(EditMode::Normal)]
            }
            "c" => {
                self.mode = EditMode::Insert;
                vec![DeleteSelection, SetMode(EditMode::Insert)]
            }
            "y" => {
                self.mode = EditMode::Normal;
                vec![Yank]
            }
            "v" | "V" | "Escape" => {
                self.mode = EditMode::Normal;
                vec![SetMode(EditMode::Normal)]
            }
            _ => vec![],
        }
    }

    fn insert(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        use EditCommand::*;
        match k.key.as_str() {
            "Escape" => {
                self.mode = EditMode::Normal;
                vec![Move(Motion::Left), SetMode(EditMode::Normal)]
            }
            "Backspace" => vec![DeleteBack],
            "Delete" => vec![DeleteForward],
            "Enter" => vec![InsertNewline],
            "Tab" => vec![InsertTab],
            "ArrowLeft" => vec![Move(Motion::Left)],
            "ArrowRight" => vec![Move(Motion::Right)],
            "ArrowUp" => vec![Move(Motion::Up)],
            "ArrowDown" => vec![Move(Motion::Down)],
            _ => vec![],
        }
    }

    fn ex_key(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        match k.key.as_str() {
            "Enter" => {
                let cmd = self.ex.take().unwrap_or_default();
                match cmd.as_str() {
                    "w" | "wq" | "x" => vec![EditCommand::Save],
                    _ => vec![],
                }
            }
            "Escape" => {
                self.ex = None;
                vec![]
            }
            "Backspace" => {
                if let Some(buf) = self.ex.as_mut() {
                    buf.pop();
                }
                vec![]
            }
            key if key.len() == 1 => {
                if let Some(buf) = self.ex.as_mut() {
                    buf.push_str(key);
                }
                vec![]
            }
            _ => vec![],
        }
    }
}

impl Keymap for VimKeymap {
    fn mode(&self) -> EditMode {
        self.mode
    }
    fn handle(&mut self, k: &KeyInput) -> Vec<EditCommand> {
        if self.ex.is_some() {
            return self.ex_key(k);
        }
        match self.mode {
            EditMode::Insert => self.insert(k),
            EditMode::Visual | EditMode::VisualLine => self.visual(k),
            EditMode::Normal => self.normal(k),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Mods;

    fn k(key: &str) -> KeyInput {
        KeyInput {
            key: key.into(),
            mods: Mods::default(),
            repeat: false,
        }
    }
    fn ctrl(key: &str) -> KeyInput {
        KeyInput {
            key: key.into(),
            mods: Mods {
                ctrl: true,
                ..Default::default()
            },
            repeat: false,
        }
    }

    #[test]
    fn dw_deletes_word() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&k("d")), vec![]);
        assert_eq!(
            km.handle(&k("w")),
            vec![EditCommand::DeleteRange(Motion::WordNext)]
        );
    }

    #[test]
    fn dd_deletes_line() {
        let mut km = VimKeymap::default();
        km.handle(&k("d"));
        assert_eq!(km.handle(&k("d")), vec![EditCommand::DeleteLine]);
    }

    #[test]
    fn count_repeats_motion() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&k("3")), vec![]);
        assert_eq!(
            km.handle(&k("j")),
            vec![
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down),
                EditCommand::Move(Motion::Down)
            ]
        );
    }

    #[test]
    fn i_enters_insert() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.handle(&k("i")),
            vec![EditCommand::SetMode(EditMode::Insert)]
        );
        assert_eq!(km.mode(), EditMode::Insert);
    }

    #[test]
    fn esc_in_insert_returns_normal_and_steps_left() {
        let mut km = VimKeymap::default();
        km.handle(&k("i"));
        assert_eq!(
            km.handle(&k("Escape")),
            vec![
                EditCommand::Move(Motion::Left),
                EditCommand::SetMode(EditMode::Normal)
            ]
        );
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn visual_select_and_yank() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.handle(&k("v")),
            vec![EditCommand::SetMode(EditMode::Visual)]
        );
        assert_eq!(km.handle(&k("l")), vec![EditCommand::Select(Motion::Right)]);
        assert_eq!(km.handle(&k("y")), vec![EditCommand::Yank]);
        assert_eq!(km.mode(), EditMode::Normal);
    }

    #[test]
    fn o_opens_line_below() {
        let mut km = VimKeymap::default();
        assert_eq!(
            km.handle(&k("o")),
            vec![
                EditCommand::Move(Motion::LineEnd),
                EditCommand::InsertNewline,
                EditCommand::SetMode(EditMode::Insert)
            ]
        );
    }

    #[test]
    fn ctrl_r_redo() {
        let mut km = VimKeymap::default();
        assert_eq!(km.handle(&ctrl("r")), vec![EditCommand::Redo]);
    }

    #[test]
    fn ex_write_saves() {
        let mut km = VimKeymap::default();
        km.handle(&k(":"));
        km.handle(&k("w"));
        assert_eq!(km.handle(&k("Enter")), vec![EditCommand::Save]);
    }
}
