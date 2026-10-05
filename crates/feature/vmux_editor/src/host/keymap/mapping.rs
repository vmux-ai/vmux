use crate::edit::command::EditMode;
use crate::keymap::{KeyInput, Mods};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MapScope {
    normal: bool,
    insert: bool,
    visual: bool,
}

impl MapScope {
    fn parse(spec: &str) -> Self {
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

    fn covers(self, mode: EditMode) -> bool {
        match mode {
            EditMode::Normal => self.normal,
            EditMode::Insert | EditMode::Replace => self.insert,
            _ if mode.is_visual() => self.visual,
            _ => false,
        }
    }
}

impl KeyInput {
    fn parse(notation: &str, leader: &str) -> Vec<Self> {
        let mut keys = Vec::new();
        let mut rest = notation;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('<')
                && let Some(close) = after.find('>')
            {
                let name = &after[..close];
                if name.eq_ignore_ascii_case("leader") {
                    keys.extend(Self::parse(leader, ""));
                    rest = &after[close + 1..];
                    continue;
                }
                if let Some(key) = Self::named(name) {
                    keys.push(key);
                    rest = &after[close + 1..];
                    continue;
                }
            }
            let character = rest.chars().next().expect("rest is non-empty");
            keys.push(Self::plain(&character.to_string()));
            rest = &rest[character.len_utf8()..];
        }
        keys
    }

    fn plain(key: &str) -> Self {
        Self {
            key: key.to_string(),
            mods: Mods::default(),
            repeat: false,
        }
    }

    fn named(name: &str) -> Option<Self> {
        let lower = name.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("c-") {
            let mut key = Self::plain(rest);
            key.mods.ctrl = true;
            return Some(key);
        }
        if let Some(rest) = lower.strip_prefix("a-").or(lower.strip_prefix("m-")) {
            let mut key = Self::plain(rest);
            key.mods.alt = true;
            return Some(key);
        }
        if let Some(rest) = lower.strip_prefix("d-") {
            let mut key = Self::plain(rest);
            key.mods.meta = true;
            return Some(key);
        }
        if let Some(rest) = name.strip_prefix("S-").or(name.strip_prefix("s-")) {
            let mut key = Self::plain(&rest.to_ascii_uppercase());
            key.mods.shift = true;
            return Some(key);
        }
        Some(match lower.as_str() {
            "esc" => Self::plain("Escape"),
            "cr" | "enter" | "return" => Self::plain("Enter"),
            "tab" => Self::plain("Tab"),
            "space" => Self::plain(" "),
            "bs" => Self::plain("Backspace"),
            "del" => Self::plain("Delete"),
            "up" => Self::plain("ArrowUp"),
            "down" => Self::plain("ArrowDown"),
            "left" => Self::plain("ArrowLeft"),
            "right" => Self::plain("ArrowRight"),
            "lt" => Self::plain("<"),
            "bar" => Self::plain("|"),
            "nop" => Self::plain(""),
            _ => return None,
        })
    }
}

struct Mapping {
    scope: MapScope,
    lhs: Vec<KeyInput>,
    rhs: Vec<KeyInput>,
}

pub(crate) enum MatchResult {
    Pending,
    Expand(Vec<KeyInput>),
    Miss,
}

#[derive(Default)]
pub(crate) struct Mappings {
    entries: Vec<Mapping>,
}

impl Mappings {
    pub(crate) fn new(specs: &[vmux_api::editor::KeyMapping], leader: &str) -> Self {
        let entries = specs
            .iter()
            .filter_map(|spec| {
                let lhs = KeyInput::parse(&spec.lhs, leader);
                if lhs.is_empty() {
                    return None;
                }
                Some(Mapping {
                    scope: MapScope::parse(&spec.mode),
                    lhs,
                    rhs: KeyInput::parse(&spec.rhs, leader),
                })
            })
            .collect();
        Self { entries }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn match_keys(&self, mode: EditMode, pending: &[KeyInput]) -> MatchResult {
        let active = self
            .entries
            .iter()
            .filter(|entry| entry.scope.covers(mode))
            .filter(|entry| {
                entry.lhs.len() >= pending.len()
                    && entry
                        .lhs
                        .iter()
                        .zip(pending)
                        .all(|(left, right)| left == right)
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

    fn spec(mode: &str, lhs: &str, rhs: &str) -> vmux_api::editor::KeyMapping {
        vmux_api::editor::KeyMapping {
            mode: mode.into(),
            lhs: lhs.into(),
            rhs: rhs.into(),
        }
    }

    #[test]
    fn notation_expands_leader_and_named_keys() {
        let keys = KeyInput::parse("<leader>w", " ");
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key, " ");
        assert_eq!(keys[1].key, "w");

        let esc = KeyInput::parse("<Esc>", "");
        assert_eq!(esc[0].key, "Escape");
    }

    #[test]
    fn modifier_notation_sets_mods() {
        let keys = KeyInput::parse("<C-x>", "");
        assert!(keys[0].mods.ctrl);
        assert_eq!(keys[0].key, "x");
    }

    #[test]
    fn an_exact_match_expands_and_a_prefix_pends() {
        let maps = Mappings::new(&[spec("n", "gh", "^"), spec("n", "ghi", "$")], " ");
        let g = KeyInput::parse("g", "");
        assert!(matches!(
            maps.match_keys(EditMode::Normal, &g),
            MatchResult::Pending
        ));
        let gh = KeyInput::parse("gh", "");
        assert!(matches!(
            maps.match_keys(EditMode::Normal, &gh),
            MatchResult::Expand(_)
        ));
        let zz = KeyInput::parse("zz", "");
        assert!(matches!(
            maps.match_keys(EditMode::Normal, &zz),
            MatchResult::Miss
        ));
    }

    #[test]
    fn scope_limits_which_mode_sees_a_mapping() {
        let maps = Mappings::new(&[spec("i", "jk", "<Esc>")], " ");
        let j = KeyInput::parse("j", "");
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
