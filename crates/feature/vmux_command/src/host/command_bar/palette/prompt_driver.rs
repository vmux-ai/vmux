#[derive(Clone, PartialEq, Eq)]
pub(super) struct PromptContext {
    pub(super) agent: String,
    pub(super) cwd: String,
}

impl PromptContext {
    pub(super) fn new(agent: &str, cwd: &str) -> Option<Self> {
        if agent.is_empty() || cwd.is_empty() {
            return None;
        }
        Some(Self {
            agent: agent.to_string(),
            cwd: cwd.to_string(),
        })
    }
}
