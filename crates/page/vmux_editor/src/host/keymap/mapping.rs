use crate::edit::command::EditMode;
use crate::keymap::{KeyInput, Mods};

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

pub enum MatchResult {
    Pending,
    Expand(Vec<KeyInput>),
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
mod tests {
    use super::*;

    fn spec(mode: &str, lhs: &str, rhs: &str) -> vmux_core::editor::KeyMapping {
        vmux_core::editor::KeyMapping {
            mode: mode.into(),
            lhs: lhs.into(),
            rhs: rhs.into(),
        }
    }

    #[test]
    fn notation_expands_leader_and_named_keys() {
        let keys = parse_keys("<leader>w", " ");
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key, " ");
        assert_eq!(keys[1].key, "w");

        let esc = parse_keys("<Esc>", "");
        assert_eq!(esc[0].key, "Escape");
    }

    #[test]
    fn modifier_notation_sets_mods() {
        let keys = parse_keys("<C-x>", "");
        assert!(keys[0].mods.ctrl);
        assert_eq!(keys[0].key, "x");
    }

    #[test]
    fn an_exact_match_expands_and_a_prefix_pends() {
        let maps = Mappings::new(&[spec("n", "gh", "^"), spec("n", "ghi", "$")], " ");
        let g = parse_keys("g", "");
        assert!(matches!(
            maps.match_keys(EditMode::Normal, &g),
            MatchResult::Pending
        ));
        let gh = parse_keys("gh", "");
        assert!(matches!(
            maps.match_keys(EditMode::Normal, &gh),
            MatchResult::Expand(_)
        ));
        let zz = parse_keys("zz", "");
        assert!(matches!(
            maps.match_keys(EditMode::Normal, &zz),
            MatchResult::Miss
        ));
    }

    #[test]
    fn scope_limits_which_mode_sees_a_mapping() {
        let maps = Mappings::new(&[spec("i", "jk", "<Esc>")], " ");
        let j = parse_keys("j", "");
        assert!(matches!(
            maps.match_keys(EditMode::Insert, &j),
            MatchResult::Pending
        ));
        assert!(matches!(
            maps.match_keys(EditMode::Normal, &j),
            MatchResult::Miss
        ));
    }

    #[test]
    fn an_empty_mode_spec_covers_normal_and_visual() {
        let scope = MapScope::parse("");
        assert!(scope.covers(EditMode::Normal));
        assert!(scope.covers(EditMode::Visual));
        assert!(!scope.covers(EditMode::Insert));
    }
}
