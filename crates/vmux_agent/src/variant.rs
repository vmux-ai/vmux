#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    bevy::prelude::Reflect,
)]
pub enum AgentVariant {
    Page,
    Cli,
}

impl AgentVariant {
    pub fn as_url_segment(self) -> Option<&'static str> {
        match self {
            AgentVariant::Page => None,
            AgentVariant::Cli => Some("cli"),
        }
    }

    pub fn from_url_segment(segment: Option<&str>) -> Option<Self> {
        match segment {
            None | Some("") => Some(AgentVariant::Page),
            Some("cli") => Some(AgentVariant::Cli),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "variant.test.rs"]
mod tests;
