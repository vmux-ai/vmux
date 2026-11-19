#[cfg(feature = "ecs")]
use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

use super::{Subject, Value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ecs", derive(Component))]
#[cfg_attr(feature = "ecs", component(immutable))]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub environment: String,
    pub generation: String,
    pub flag: String,
    pub subject: Subject,
    pub value: Value,
    pub variant: Option<String>,
}

impl Assignment {
    pub fn validate(&self) -> Result<(), String> {
        for (value, limit) in [
            (&self.environment, 64),
            (&self.generation, 128),
            (&self.flag, 128),
        ] {
            if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
                return Err("invalid experiment assignment identity".into());
            }
        }
        self.subject.validate()?;
        self.value.validate()?;
        if let Some(variant) = &self.variant
            && (variant.is_empty() || variant.len() > 128 || variant.chars().any(char::is_control))
        {
            return Err("invalid experiment variant".into());
        }
        Ok(())
    }

    pub fn matches(
        &self,
        environment: &str,
        generation: &str,
        flag: &str,
        subject: &Subject,
    ) -> bool {
        self.environment == environment
            && self.generation == generation
            && self.flag == flag
            && self.subject == *subject
    }
}
