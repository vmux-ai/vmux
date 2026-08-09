use crate::edit::command::EditMode;
use crate::keymap::{KeyInput, Mods};

/// Which modes a mapping applies to, from the leading letters of a `:map` command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapScope {
    pub normal: bool,
    pub insert: bool,
    pub visual: bool,
}

impl MapScope {
    pub fn parse(spec: &str) -> Self {
        let spec = spec.trim();
        if spec.is_empty() {
            return Self {
                normal: true,
                insert: false,
                visual: true,
            };
        }
        Self {
            normal: spec.contains('n'),
            insert: spec.contains('i'),
            visual: spec.contains('v') || spec.contains('x'),
        }
    }

    pub fn covers(self, mode: EditMode) -> bool {
        match mode {
            EditMode::Normal => self.normal,
            EditMode::Insert | EditMode::Replace => self.insert,
            _ if mode.is_visual() => self.visual,
            _ => false,
        }
    }
}

/// Translate vim key notation into the key names the keymap dispatches on.
///
/// Understands `<leader>`, `<C-x>`, `<S-x>`, `<A-x>`, and the usual `<Esc>`/`<CR>`/`<Tab>`/
/// `<Space>`/`<BS>` names. Anything else is taken one character at a time.
pub fn parse_keys(notation: &str, leader: &str) -> Vec<KeyInput> {
    let mut out = Vec::new();
    let mut rest = notation;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('<')
            && let Some(close) = after.find('>')
        {
            let name = &after[..close];
            if name.eq_ignore_ascii_case("leader") {
                out.extend(parse_keys(leader, ""));
                rest = &after[close + 1..];
                continue;
            }
            if let Some(key) = named_key(name) {
                out.push(key);
                rest = &after[close + 1..];
                continue;
            }
        }
        let c = rest.chars().next().expect("rest is non-empty");
        out.push(plain(&c.to_string()));
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn plain(key: &str) -> KeyInput {
    KeyInput {
        key: key.to_string(),
        mods: Mods::default(),
        repeat: false,
    }
}

fn named_key(name: &str) -> Option<KeyInput> {
    let lower = name.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("c-") {
        let mut key = plain(rest);
        key.mods.ctrl = true;
        return Some(key);
    }
    if let Some(rest) = lower.strip_prefix("a-").or(lower.strip_prefix("m-")) {
        let mut key = plain(rest);
        key.mods.alt = true;
        return Some(key);
    }
    if let Some(rest) = lower.strip_prefix("d-") {
        let mut key = plain(rest);
        key.mods.meta = true;
        return Some(key);
    }
    if let Some(rest) = name.strip_prefix("S-").or(name.strip_prefix("s-")) {
        let mut key = plain(&rest.to_ascii_uppercase());
        key.mods.shift = true;
        return Some(key);
    }
    Some(match lower.as_str() {
        "esc" => plain("Escape"),
        "cr" | "enter" | "return" => plain("Enter"),
        "tab" => plain("Tab"),
        "space" => plain(" "),
        "bs" => plain("Backspace"),
        "del" => plain("Delete"),
        "up" => plain("ArrowUp"),
        "down" => plain("ArrowDown"),
        "left" => plain("ArrowLeft"),
        "right" => plain("ArrowRight"),
        "lt" => plain("<"),
        "bar" => plain("|"),
        "nop" => plain(""),
        _ => return None,
    })
}

fn same_key(a: &KeyInput, b: &KeyInput) -> bool {
    a.key == b.key && a.mods == b.mods
}

pub struct Mapping {
    scope: MapScope,
    lhs: Vec<KeyInput>,
    rhs: Vec<KeyInput>,
}

/// How a pending key sequence lines up with the configured mappings.
pub enum MatchResult {
    /// The sequence so far could still grow into a mapping.
    Pending,
    /// The sequence expands to these keys.
    Expand(Vec<KeyInput>),
    /// No mapping can match; dispatch the buffered keys as typed.
    Miss,
}

#[derive(Default)]
pub struct Mappings {
    entries: Vec<Mapping>,
}

impl Mappings {
    pub fn new(specs: &[vmux_core::editor::KeyMapping], leader: &str) -> Self {
        let entries = specs
            .iter()
            .filter_map(|spec| {
                let lhs = parse_keys(&spec.lhs, leader);
                if lhs.is_empty() {
                    return None;
                }
                Some(Mapping {
                    scope: MapScope::parse(&spec.mode),
                    lhs,
                    rhs: parse_keys(&spec.rhs, leader),
                })
            })
            .collect();
        Self { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Match `pending` against the mappings active in `mode`.
    ///
    /// An exact match wins immediately even when a longer mapping shares the prefix. Vim would
    /// wait out `timeoutlen` before committing; resolving now keeps the keymap free of timers.
    pub fn match_keys(&self, mode: EditMode, pending: &[KeyInput]) -> MatchResult {
        let active = self
            .entries
            .iter()
            .filter(|entry| entry.scope.covers(mode))
            .filter(|entry| {
                entry.lhs.len() >= pending.len()
                    && entry.lhs.iter().zip(pending).all(|(a, b)| same_key(a, b))
            });
        let mut longer = false;
        for entry in active {
            if entry.lhs.len() == pending.len() {
                return MatchResult::Expand(entry.rhs.clone());
            }
            longer = true;
        }
        if longer {
            MatchResult::Pending
        } else {
            MatchResult::Miss
        }
    }
}

#[cfg(test)]
#[path = "mapping.test.rs"]
mod tests;
