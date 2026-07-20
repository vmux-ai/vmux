#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;

pub const MAX_SELECTION_CONTEXT_CHARS: usize = 32_768;

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct AgentSelectionContext {
    pub kind: String,
    pub label: String,
    pub source: String,
    pub text: String,
}

impl AgentSelectionContext {
    pub fn new(
        kind: impl Into<String>,
        label: impl Into<String>,
        source: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            label: label.into(),
            source: source.into(),
            text: truncate_selection_text(text.into()),
        }
    }
}

pub fn truncate_selection_text(text: String) -> String {
    if text.chars().count() <= MAX_SELECTION_CONTEXT_CHARS {
        return text;
    }
    let mut truncated = text
        .chars()
        .take(MAX_SELECTION_CONTEXT_CHARS)
        .collect::<String>();
    truncated.push_str("\n\n[Selection truncated]");
    truncated
}

pub fn render_selection_contexts(contexts: &[AgentSelectionContext]) -> Option<String> {
    if contexts.is_empty() {
        return None;
    }
    let mut rendered = String::new();
    for context in contexts {
        if !rendered.is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str("Selected ");
        rendered.push_str(&context.kind);
        rendered.push_str(" context: ");
        rendered.push_str(&context.label);
        if !context.source.is_empty() {
            rendered.push_str(" (");
            rendered.push_str(&context.source);
            rendered.push(')');
        }
        rendered.push_str("\n\n");
        rendered.push_str(&context.text);
    }
    Some(rendered)
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone, Debug)]
pub struct CaptureSelectionRequest {
    pub request_id: u64,
    pub source_tab: Option<Entity>,
    pub source_pane: Option<Entity>,
    pub source_stack: Option<Entity>,
    pub webview: Option<Entity>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone, Debug)]
pub struct DomSelectionRequest {
    pub capture: CaptureSelectionRequest,
    pub fallback: Option<AgentSelectionContext>,
    pub kind: String,
    pub label: String,
    pub source: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Message, Clone, Debug)]
pub struct SelectionCaptured {
    pub request_id: u64,
    pub source_tab: Option<Entity>,
    pub source_pane: Option<Entity>,
    pub source_stack: Option<Entity>,
    pub context: Option<AgentSelectionContext>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct SelectionRequestCounter(pub u64);

#[cfg(not(target_arch = "wasm32"))]
impl SelectionRequestCounter {
    pub fn next_id(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1);
        self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaptureSelectionSet;

#[cfg(not(target_arch = "wasm32"))]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteSelectionSet;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_context_rendering_preserves_sources() {
        let rendered = render_selection_contexts(&[
            AgentSelectionContext::new("file", "main.rs:3-4", "/tmp/main.rs", "let x = 1;"),
            AgentSelectionContext::new("browser", "Docs", "https://example.com", "API text"),
        ])
        .unwrap();

        assert!(rendered.contains("main.rs:3-4 (/tmp/main.rs)"));
        assert!(rendered.contains("Docs (https://example.com)"));
        assert!(rendered.contains("let x = 1;"));
        assert!(rendered.contains("API text"));
    }

    #[test]
    fn long_selection_contexts_have_visible_truncation() {
        let context = AgentSelectionContext::new(
            "file",
            "large.rs",
            "/tmp/large.rs",
            "x".repeat(MAX_SELECTION_CONTEXT_CHARS + 1),
        );

        assert!(context.text.ends_with("[Selection truncated]"));
    }
}
