use crate::event::{ProviderSessionTarget, SessionRepository};
pub(super) use crate::manager::*;
use vmux_session::{
    AgentFamily, SessionAge, SessionSummary, SessionSummaryProjection, StageSummary,
};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum RowIdentity {
    Provider(AgentFamily, String),
    Local(String),
}

impl RowIdentity {
    pub(super) fn of(session: &SessionSummary) -> Self {
        if !session.provider_kind.is_empty() && !session.provider_sid.is_empty() {
            return Self::Provider(
                AgentFamily::of(session.provider_kind.trim_start_matches("acp:")),
                session.provider_sid.clone(),
            );
        }
        if !session.managed && !session.kind.is_empty() && !session.sid.is_empty() {
            return Self::Provider(
                AgentFamily::of(session.kind.trim_start_matches("acp:")),
                session.sid.clone(),
            );
        }
        Self::Local(session.id.clone())
    }

    pub(super) fn key(&self) -> String {
        format!("{self:?}")
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SessionFilterStep {
    Field,
    Status,
    Agent,
    Project,
    Updated,
    Date(UpdatedComparison),
    Within,
}

#[derive(Clone, PartialEq)]
pub(super) struct StageGroupPresentation {
    pub dot: &'static str,
    pub archivable: Vec<ArchiveTarget>,
    pub restorable: Vec<ArchiveTarget>,
}

impl StageGroupPresentation {
    pub fn new(stage: &StageSummary, sessions: &[SessionSummary]) -> Self {
        let mut archivable = Vec::new();
        let mut restorable = Vec::new();
        for session in sessions {
            let target = ArchiveTarget::from_session(session);
            if target.archived {
                restorable.push(target);
            } else {
                archivable.push(target);
            }
        }
        Self {
            dot: StageVisual::for_id(&stage.id).dot,
            archivable,
            restorable,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct ArchiveTarget {
    pub id: String,
    pub archived: bool,
}

impl ArchiveTarget {
    pub fn from_session(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            archived: session.archived_at > 0,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct DeleteTarget {
    pub id: String,
    pub provider: Option<ProviderSessionTarget>,
}

impl DeleteTarget {
    pub fn from_session(session: &SessionSummary) -> Self {
        let provider = (!session.provider_kind.is_empty() && !session.provider_sid.is_empty())
            .then(|| ProviderSessionTarget {
                kind: session.provider_kind.clone(),
                sid: session.provider_sid.clone(),
            });
        Self {
            id: session.id.clone(),
            provider,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct StageTarget {
    pub id: String,
    pub kind: String,
    pub sid: String,
    pub stage: String,
}

impl StageTarget {
    pub fn from_session(session: &SessionSummary) -> Self {
        Self {
            id: session.id.clone(),
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            stage: session.stage.clone(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct SessionContext {
    pub project_identity: String,
    pub project: String,
    pub project_url: String,
    pub branch: String,
    pub git_url: String,
    pub editor_url: String,
}

impl SessionContext {
    fn from(repository: Option<SessionRepository>, cwd: &str) -> Self {
        let editor_url = url::Url::from_directory_path(cwd)
            .map(|mut url| {
                url.query_pairs_mut().append_pair("vmux-entry", "1");
                url.to_string()
            })
            .unwrap_or_default();
        match repository {
            Some(repository) => Self {
                project_identity: repository.project_identity,
                project: repository.project,
                project_url: repository.project_url,
                branch: repository.branch,
                git_url: repository.url,
                editor_url,
            },
            None => Self {
                project_identity: String::new(),
                project: String::new(),
                project_url: String::new(),
                branch: String::new(),
                git_url: String::new(),
                editor_url,
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct AgentPresentation {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: String,
}

impl AgentPresentation {
    fn from_session(session: &SessionSummary) -> Self {
        Self {
            id: session.agent.clone(),
            name: if session.agent_name.is_empty() {
                session.agent.clone()
            } else {
                session.agent_name.clone()
            },
            icon: (!session.agent_icon.is_empty()).then(|| session.agent_icon.clone()),
            color: session.agent_color.clone(),
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct SessionPresentation {
    pub context: SessionContext,
    pub stage_target: StageTarget,
    pub archived: bool,
    pub runtime: RuntimeVisual,
    pub agent: AgentPresentation,
    pub stage_dot: &'static str,
    pub title: String,
    pub preview: String,
    pub compact_id: String,
    pub updated: String,
}

impl SessionPresentation {
    pub fn new(
        session: &SessionSummary,
        repository: Option<SessionRepository>,
        stages: &[StageSummary],
        stage: &str,
    ) -> Self {
        let summary = SessionSummaryProjection::new(session, stages, vmux_ecs::UnixMillis::now().0);
        Self {
            context: SessionContext::from(repository, &session.cwd),
            stage_target: StageTarget::from_session(session),
            archived: session.archived_at > 0,
            runtime: RuntimeVisual::for_id(&session.runtime),
            agent: AgentPresentation::from_session(session),
            stage_dot: StageVisual::for_id(stage).dot,
            title: summary.title,
            preview: summary.preview,
            compact_id: SessionIdentity::new(&session.sid).compact(),
            updated: RelativeTime(summary.age).label(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct StageVisual {
    pub dot: &'static str,
}

impl StageVisual {
    pub fn for_id(id: &str) -> Self {
        let dot = match id {
            "in_progress" => "bg-amber-400",
            "stale" => "bg-muted-foreground/55",
            "in_review" => "bg-violet-400",
            "done" => "bg-success",
            "canceled" => "bg-muted-foreground/45",
            _ => "bg-primary/75",
        };
        Self { dot }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct RuntimeVisual {
    pub dot: &'static str,
    pub visible: bool,
}

impl RuntimeVisual {
    pub fn for_id(id: &str) -> Self {
        let dot = match id {
            "streaming" => "bg-success",
            "awaiting" => "bg-amber-400",
            "installing" => "bg-sky-400",
            "errored" => "bg-ansi-1",
            "idle" => "bg-primary/65",
            _ => "bg-muted-foreground/35",
        };
        let visible = matches!(id, "streaming" | "awaiting" | "installing" | "errored");
        Self { dot, visible }
    }
}

pub(super) struct RelativeTime(pub SessionAge);

impl RelativeTime {
    pub fn label(self) -> String {
        let (id, count) = if self.0.0 < SessionAge::MINUTE {
            ("resume-age-now", 0)
        } else if self.0.0 < SessionAge::HOUR {
            ("resume-age-minutes", self.0.0 / SessionAge::MINUTE)
        } else if self.0.0 < SessionAge::DAY {
            ("resume-age-hours", self.0.0 / SessionAge::HOUR)
        } else if self.0.0 < SessionAge::WEEK {
            ("resume-age-days", self.0.0 / SessionAge::DAY)
        } else if self.0.0 < SessionAge::MONTH {
            ("resume-age-weeks", self.0.0 / SessionAge::WEEK)
        } else if self.0.0 < SessionAge::YEAR {
            ("resume-age-months", self.0.0 / SessionAge::MONTH)
        } else {
            ("resume-age-years", self.0.0 / SessionAge::YEAR)
        };
        if count == 0 {
            return translate(id);
        }
        translate_with(id, &[("count", TranslationValue::Number(count as i64))])
    }
}

pub(super) struct SessionIdentity<'a>(&'a str);

impl<'a> SessionIdentity<'a> {
    pub fn new(id: &'a str) -> Self {
        Self(id)
    }

    pub fn compact(&self) -> String {
        let characters = self.0.chars().collect::<Vec<_>>();
        if characters.len() <= 13 {
            return self.0.to_string();
        }
        let start = characters[..6].iter().collect::<String>();
        let end = characters[characters.len() - 4..]
            .iter()
            .collect::<String>();
        format!("{start}…{end}")
    }
}

pub(super) fn runtime_label(runtime: &str) -> String {
    let id = match runtime {
        "idle" => "session-runtime-idle",
        "installing" => "session-runtime-installing",
        "streaming" => "session-runtime-streaming",
        "awaiting" => "session-runtime-awaiting",
        "errored" => "session-runtime-errored",
        _ => "session-runtime-inactive",
    };
    translate(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_directory_links_preserve_the_path_and_request_a_code_entry_file() {
        let path = "/tmp/a project/日本語";
        let context = SessionContext::from(None, path);
        let url = url::Url::parse(&context.editor_url).unwrap();
        assert_eq!(url.to_file_path().unwrap(), std::path::PathBuf::from(path));
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "vmux-entry" && value == "1")
        );
    }
}
