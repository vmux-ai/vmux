use vmux_command::island::{IslandActivity, IslandActivityKind, IslandState};

#[derive(Clone, Debug)]
pub enum IslandInput {
    ExpandSearch,
    Collapse,
    Activity(IslandActivity),
    ActivityEnded(IslandActivityKind),
}

/// Pure morph reducer. Search has top priority; otherwise the most-recently-started live activity
/// is shown; otherwise idle. Notices are transient and handled by the ECS layer (timer), not here.
#[derive(Default)]
pub struct IslandMachine {
    searching: bool,
    activities: Vec<IslandActivity>,
}

impl IslandMachine {
    pub fn apply(&mut self, input: IslandInput) {
        match input {
            IslandInput::ExpandSearch => self.searching = true,
            IslandInput::Collapse => self.searching = false,
            IslandInput::Activity(a) => {
                self.activities.retain(|x| x.kind != a.kind);
                self.activities.push(a);
            }
            IslandInput::ActivityEnded(kind) => self.activities.retain(|x| x.kind != kind),
        }
    }

    pub fn render_state(&self) -> IslandState {
        if self.searching {
            IslandState::Search
        } else if let Some(a) = self.activities.last() {
            IslandState::Activity(a.clone())
        } else {
            IslandState::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(kind: IslandActivityKind) -> IslandActivity {
        IslandActivity {
            kind,
            label: "x".into(),
            progress: None,
        }
    }

    #[test]
    fn search_overrides_activity_then_restores() {
        let mut m = IslandMachine::default();
        m.apply(IslandInput::Activity(act(IslandActivityKind::Agent)));
        assert!(matches!(m.render_state(), IslandState::Activity(_)));
        m.apply(IslandInput::ExpandSearch);
        assert!(matches!(m.render_state(), IslandState::Search));
        m.apply(IslandInput::Collapse);
        assert!(matches!(m.render_state(), IslandState::Activity(_)));
    }

    #[test]
    fn idle_when_nothing_active() {
        let mut m = IslandMachine::default();
        m.apply(IslandInput::Activity(act(IslandActivityKind::Terminal)));
        m.apply(IslandInput::ActivityEnded(IslandActivityKind::Terminal));
        assert!(matches!(m.render_state(), IslandState::Idle));
    }

    #[test]
    fn latest_activity_wins() {
        let mut m = IslandMachine::default();
        m.apply(IslandInput::Activity(act(IslandActivityKind::Agent)));
        m.apply(IslandInput::Activity(act(IslandActivityKind::Terminal)));
        match m.render_state() {
            IslandState::Activity(a) => assert_eq!(a.kind, IslandActivityKind::Terminal),
            other => panic!("expected activity, got {other:?}"),
        }
    }
}
