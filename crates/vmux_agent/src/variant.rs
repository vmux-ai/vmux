#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgentVariant {
    Gui,
    Cli,
}

impl AgentVariant {
    pub fn as_url_segment(self) -> Option<&'static str> {
        match self {
            AgentVariant::Gui => None,
            AgentVariant::Cli => Some("cli"),
        }
    }

    pub fn from_url_segment(segment: Option<&str>) -> Option<Self> {
        match segment {
            None | Some("") => Some(AgentVariant::Gui),
            Some("cli") => Some(AgentVariant::Cli),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_segment_round_trips() {
        for v in [AgentVariant::Gui, AgentVariant::Cli] {
            assert_eq!(AgentVariant::from_url_segment(v.as_url_segment()), Some(v));
        }
    }

    #[test]
    fn empty_segment_resolves_to_gui() {
        assert_eq!(
            AgentVariant::from_url_segment(Some("")),
            Some(AgentVariant::Gui)
        );
        assert_eq!(
            AgentVariant::from_url_segment(None),
            Some(AgentVariant::Gui)
        );
    }

    #[test]
    fn unknown_segment_returns_none() {
        assert_eq!(AgentVariant::from_url_segment(Some("nope")), None);
    }
}
