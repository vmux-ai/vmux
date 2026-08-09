use std::collections::BTreeMap;

/// How a register's text is reinserted by a put.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RegisterKind {
    #[default]
    Charwise,
    Linewise,
    /// A rectangle: each line of the text is one row of the block.
    Blockwise,
}

/// Text captured by a yank or delete, tagged with the shape it should be put back in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegisterValue {
    pub text: String,
    pub kind: RegisterKind,
}

impl RegisterValue {
    pub fn charwise(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: RegisterKind::Charwise,
        }
    }
    pub fn linewise(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: RegisterKind::Linewise,
        }
    }
}

pub const UNNAMED: char = '"';
pub const BLACKHOLE: char = '_';
pub const SMALL_DELETE: char = '-';

/// Vim's register file: unnamed, named `a`-`z`, numbered `0`-`9`, and the small-delete register.
///
/// `clipboard_shadow` records the text vmux last pushed to the system clipboard so the host can
/// tell an external copy from its own, and avoid clobbering a linewise register on every put.
#[derive(Default)]
pub struct Registers {
    slots: BTreeMap<char, RegisterValue>,
    pub clipboard_shadow: String,
}

fn is_clipboard(name: char) -> bool {
    name == '+' || name == '*'
}

impl Registers {
    pub fn read(&self, name: Option<char>) -> Option<&RegisterValue> {
        let name = name.unwrap_or(UNNAMED);
        if name == BLACKHOLE {
            return None;
        }
        let name = if is_clipboard(name) { UNNAMED } else { name };
        self.slots.get(&name.to_ascii_lowercase())
    }

    pub fn set(&mut self, name: char, value: RegisterValue) {
        self.slots.insert(name, value);
    }

    /// Replace the unnamed register without disturbing the numbered ring.
    pub fn set_unnamed(&mut self, value: RegisterValue) {
        self.slots.insert(UNNAMED, value);
    }

    /// Store a yank: unnamed plus `"0`, or the requested register. An uppercase name appends.
    pub fn write_yank(&mut self, name: Option<char>, value: RegisterValue) {
        if name == Some(BLACKHOLE) {
            return;
        }
        match name {
            Some(name) if !is_clipboard(name) => {
                self.write_named(name, value.clone());
            }
            _ => {
                self.slots.insert('0', value.clone());
            }
        }
        self.slots.insert(UNNAMED, value);
    }

    /// Store a delete: unnamed plus either the numbered ring (linewise or multi-line) or `"-`.
    pub fn write_delete(&mut self, name: Option<char>, value: RegisterValue) {
        if name == Some(BLACKHOLE) {
            return;
        }
        match name {
            Some(name) if !is_clipboard(name) => {
                self.write_named(name, value.clone());
            }
            _ => {
                let ring = value.kind == RegisterKind::Linewise || value.text.contains('\n');
                if ring {
                    self.shift_numbered();
                    self.slots.insert('1', value.clone());
                } else {
                    self.slots.insert(SMALL_DELETE, value.clone());
                }
            }
        }
        self.slots.insert(UNNAMED, value);
    }

    fn write_named(&mut self, name: char, value: RegisterValue) {
        let lower = name.to_ascii_lowercase();
        if name.is_ascii_uppercase()
            && let Some(existing) = self.slots.get(&lower)
        {
            let mut merged = existing.clone();
            if merged.kind == RegisterKind::Linewise && !merged.text.ends_with('\n') {
                merged.text.push('\n');
            }
            merged.text.push_str(&value.text);
            if value.kind == RegisterKind::Linewise {
                merged.kind = RegisterKind::Linewise;
            }
            self.slots.insert(lower, merged);
            return;
        }
        self.slots.insert(lower, value);
    }

    fn shift_numbered(&mut self) {
        for slot in ('1'..='8').rev() {
            let next = char::from(slot as u8 + 1);
            if let Some(value) = self.slots.get(&slot).cloned() {
                self.slots.insert(next, value);
            }
        }
    }
}

#[cfg(test)]
#[path = "register.test.rs"]
mod tests;
