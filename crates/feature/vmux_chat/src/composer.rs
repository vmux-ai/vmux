use bevy_ecs::prelude::*;
use vmux_api::prompt_media::inline_media_query;

use crate::event::ChatComposerEffect;
use crate::selector::{SelectorMode, selector_mode};

#[derive(Component, Default)]
pub struct ComposerState {
    revision: u64,
    draft: String,
    media_query: String,
    resume_active: bool,
    resume_query: String,
    mcp_open: bool,
}

impl ComposerState {
    pub fn update(&mut self, draft: impl Into<String>) -> ComposerQueryChanges {
        self.draft = draft.into();
        let media_query = inline_media_query(&self.draft)
            .map(|query| query.query.to_string())
            .unwrap_or_default();
        let (resume_active, resume_query) = match selector_mode(&self.draft) {
            SelectorMode::Resume(query) => (true, query.to_string()),
            SelectorMode::Commands(query)
                if !query.is_empty() && "resume".starts_with(&query.to_lowercase()) =>
            {
                (true, String::new())
            }
            _ => (false, String::new()),
        };
        let mcp_open = matches!(selector_mode(&self.draft), SelectorMode::Mcp(_));
        let changes = ComposerQueryChanges {
            media: (self.media_query != media_query).then_some(media_query.clone()),
            resume: (self.resume_active != resume_active || self.resume_query != resume_query)
                .then_some(ResumeQuery {
                    active: resume_active,
                    query: resume_query.clone(),
                }),
            open_mcp: !self.mcp_open && mcp_open,
        };
        self.media_query = media_query;
        self.resume_active = resume_active;
        self.resume_query = resume_query;
        self.mcp_open = mcp_open;
        changes
    }

    pub fn effect(
        &mut self,
        draft: impl Into<String>,
        focus: bool,
    ) -> (ChatComposerEffect, ComposerQueryChanges) {
        let changes = self.update(draft);
        self.revision = self.revision.wrapping_add(1).max(1);
        (
            ChatComposerEffect {
                revision: self.revision,
                draft: self.draft.clone(),
                focus,
            },
            changes,
        )
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ComposerQueryChanges {
    media: Option<String>,
    resume: Option<ResumeQuery>,
    open_mcp: bool,
}

impl ComposerQueryChanges {
    pub fn media(&self) -> Option<&str> {
        self.media.as_deref()
    }

    pub fn resume(&self) -> Option<&ResumeQuery> {
        self.resume.as_ref()
    }

    pub fn opens_mcp(&self) -> bool {
        self.open_mcp
    }

    fn changed(&self) -> bool {
        self.media.is_some() || self.resume.is_some() || self.open_mcp
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ResumeQuery {
    pub active: bool,
    pub query: String,
}

#[derive(EntityEvent)]
pub struct ComposerQueriesChanged {
    #[event_target]
    target: Entity,
    changes: ComposerQueryChanges,
}

impl ComposerQueriesChanged {
    pub fn new(target: Entity, changes: ComposerQueryChanges) -> Option<Self> {
        changes.changed().then_some(Self { target, changes })
    }

    pub fn media(&self) -> Option<&str> {
        self.changes.media()
    }

    pub fn resume(&self) -> Option<&ResumeQuery> {
        self.changes.resume()
    }

    pub fn opens_mcp(&self) -> bool {
        self.changes.opens_mcp()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::chat::SlashCommand;

    #[test]
    fn effects_are_revisioned() {
        let mut state = ComposerState::default();
        let (resume, resume_queries) = state.effect(SlashCommand::Resume.draft(), true);
        let (upload, upload_queries) = state.effect(SlashCommand::Upload.draft(), true);
        assert_eq!(resume.revision, 1);
        assert_eq!(resume.draft, "/resume ");
        assert_eq!(
            resume_queries.resume(),
            Some(&ResumeQuery {
                active: true,
                query: String::new(),
            })
        );
        assert_eq!(upload.revision, 2);
        assert!(upload.draft.is_empty());
        assert!(upload.focus);
        assert_eq!(
            upload_queries.resume(),
            Some(&ResumeQuery {
                active: false,
                query: String::new(),
            })
        );
    }

    #[test]
    fn draft_changes_drive_media_resume_and_mcp_queries() {
        let mut state = ComposerState::default();

        let resume = state.update("/res");
        assert_eq!(
            resume.resume(),
            Some(&ResumeQuery {
                active: true,
                query: String::new(),
            })
        );

        let media = state.update("show @src");
        assert_eq!(media.media(), Some("src"));
        assert_eq!(
            media.resume(),
            Some(&ResumeQuery {
                active: false,
                query: String::new(),
            })
        );

        let mcp = state.update("/mcp ");
        assert!(mcp.opens_mcp());
        assert_eq!(mcp.media(), Some(""));
    }
}
