#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Hints,
    ScrollDownLine,
    ScrollUpLine,
    ScrollDownHalf,
    ScrollUpHalf,
    ScrollTop,
    ScrollBottom,
    HistoryBack,
    HistoryForward,
    Reload,
    OpenFind,
    FindNext,
    FindPrev,
    OpenBar,
    EnterInsert,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    Action(Action),
    Pending,
    None,
}

#[derive(Default)]
pub struct Matcher {
    pending_g: bool,
}

impl Matcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, key: &str) -> MatchResult {
        if self.pending_g {
            self.pending_g = false;
            return match key {
                "g" => MatchResult::Action(Action::ScrollTop),
                _ => MatchResult::None,
            };
        }
        match key {
            "f" => MatchResult::Action(Action::Hints),
            "j" => MatchResult::Action(Action::ScrollDownLine),
            "k" => MatchResult::Action(Action::ScrollUpLine),
            "d" => MatchResult::Action(Action::ScrollDownHalf),
            "u" => MatchResult::Action(Action::ScrollUpHalf),
            "g" => {
                self.pending_g = true;
                MatchResult::Pending
            }
            "G" => MatchResult::Action(Action::ScrollBottom),
            "H" => MatchResult::Action(Action::HistoryBack),
            "L" => MatchResult::Action(Action::HistoryForward),
            "r" => MatchResult::Action(Action::Reload),
            "/" => MatchResult::Action(Action::OpenFind),
            "n" => MatchResult::Action(Action::FindNext),
            "N" => MatchResult::Action(Action::FindPrev),
            "o" => MatchResult::Action(Action::OpenBar),
            "i" => MatchResult::Action(Action::EnterInsert),
            "Escape" => MatchResult::Action(Action::Escape),
            _ => MatchResult::None,
        }
    }

    pub fn clear_pending(&mut self) {
        self.pending_g = false;
    }

    pub fn has_pending(&self) -> bool {
        self.pending_g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_keys_map_to_actions() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("f"), MatchResult::Action(Action::Hints));
        assert_eq!(m.feed("j"), MatchResult::Action(Action::ScrollDownLine));
        assert_eq!(m.feed("G"), MatchResult::Action(Action::ScrollBottom));
        assert_eq!(m.feed("H"), MatchResult::Action(Action::HistoryBack));
        assert_eq!(m.feed("/"), MatchResult::Action(Action::OpenFind));
        assert_eq!(m.feed("o"), MatchResult::Action(Action::OpenBar));
    }

    #[test]
    fn gg_scrolls_to_top() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("g"), MatchResult::Pending);
        assert!(m.has_pending());
        assert_eq!(m.feed("g"), MatchResult::Action(Action::ScrollTop));
        assert!(!m.has_pending());
    }

    #[test]
    fn g_then_other_key_cancels() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("g"), MatchResult::Pending);
        assert_eq!(m.feed("x"), MatchResult::None);
        assert!(!m.has_pending());
    }

    #[test]
    fn timeout_clears_pending_g() {
        let mut m = Matcher::new();
        m.feed("g");
        m.clear_pending();
        assert!(!m.has_pending());
        assert_eq!(m.feed("g"), MatchResult::Pending);
    }

    #[test]
    fn unbound_keys_are_none() {
        let mut m = Matcher::new();
        assert_eq!(m.feed("q"), MatchResult::None);
        assert_eq!(m.feed("1"), MatchResult::None);
    }
}
