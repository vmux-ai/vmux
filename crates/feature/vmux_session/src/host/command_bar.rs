use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::UiInput;
use vmux_api::chat::{ResumableSessionEntry, ResumableSessions, ResumeListRequest, ResumeSession};
use vmux_api::command_bar::{CommandBarResultItem, CommandBarSection, CommandBarUiStatePatch};
use vmux_ecs::UiStateWrite;
use vmux_ecs::{CommandBarContribution, CommandBarContributionActivated, CommandBarQueryChanged};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

pub(super) struct Plugin;

impl bevy::app::Plugin for Plugin {
    fn build(&self, app: &mut App) {
        app.add_observer(query)
            .add_observer(receive)
            .add_observer(activate);
    }
}

#[derive(Component, Default)]
struct ResumeState {
    request_id: u64,
    filter: String,
    offset: u32,
    total: u32,
    sessions: Vec<ResumableSessionEntry>,
}

#[derive(Component)]
struct ResumeContribution(ResumableSessionEntry);

#[derive(Component)]
struct ResumePending;

type ResumeEntities<'w, 's> =
    Query<'w, 's, (Entity, &'static ChildOf), Or<(With<ResumeContribution>, With<ResumePending>)>>;

fn query(
    trigger: On<CommandBarQueryChanged>,
    states: Query<&ResumeState>,
    mut results: ResumeResults,
) {
    let event = trigger.event();
    results.clear(event.target);
    let Some(filter) = ResumeQuery::parse(&event.query).filter(|_| event.start) else {
        results
            .commands
            .entity(event.target)
            .remove::<ResumeState>();
        return;
    };
    let request_id = states
        .get(event.target)
        .map(|state| state.request_id.wrapping_add(1).max(1))
        .unwrap_or(1);
    results.commands.entity(event.target).insert(ResumeState {
        request_id,
        filter: filter.to_string(),
        offset: 0,
        total: 0,
        sessions: Vec::new(),
    });
    results.pending(event.target, 0);
    results.commands.trigger(UiInput {
        webview: event.target,
        payload: ResumeListRequest {
            request_id,
            query: String::new(),
            offset: 0,
        },
    });
}

fn receive(
    trigger: On<UiStateWrite<vmux_api::command_bar::CommandBarUiState>>,
    mut states: Query<&mut ResumeState>,
    mut results: ResumeResults,
) {
    let Some(response) =
        <CommandBarUiStatePatch as vmux_api::UiStatePatch<ResumableSessions>>::payload(
            trigger.event().update(),
        )
    else {
        return;
    };
    let target = trigger.event().webview();
    let Ok(mut state) = states.get_mut(target) else {
        return;
    };
    if response.request_id != state.request_id
        || !response.query.is_empty()
        || response.offset != state.offset
    {
        return;
    }
    if response.offset == 0 {
        state.sessions.clone_from(&response.sessions);
    } else {
        state.sessions.extend(response.sessions.clone());
    }
    state.total = response.total;
    results.clear(target);
    results.spawn(target, &state.sessions, &state.filter);
    let loaded = state.sessions.len() as u32;
    if loaded >= state.total {
        return;
    }
    state.offset = loaded;
    results.pending(target, loaded as i32);
    results.commands.trigger(UiInput {
        webview: target,
        payload: ResumeListRequest {
            request_id: state.request_id,
            query: String::new(),
            offset: loaded,
        },
    });
}

fn activate(
    trigger: On<CommandBarContributionActivated>,
    contributions: Query<&ResumeContribution>,
    mut commands: Commands,
) {
    let event = trigger.event();
    let Ok(contribution) = contributions.get(event.target) else {
        return;
    };
    let session = &contribution.0;
    commands.trigger(UiInput {
        webview: event.webview,
        payload: ResumeSession {
            kind: session.kind.clone(),
            sid: session.sid.clone(),
            cwd: session.cwd.clone(),
        },
    });
}

#[derive(SystemParam)]
struct ResumeResults<'w, 's> {
    existing: ResumeEntities<'w, 's>,
    commands: Commands<'w, 's>,
}

impl ResumeResults<'_, '_> {
    fn clear(&mut self, target: Entity) {
        for (entity, parent) in &self.existing {
            if parent.parent() == target {
                self.commands.entity(entity).despawn();
            }
        }
    }

    fn pending(&mut self, target: Entity, rank: i32) {
        for row in 0..7 {
            self.commands.spawn((
                Name::new(format!("Resume pending row {row}")),
                CommandBarContribution {
                    row: CommandBarResultItem {
                        key: format!("resume:pending:{row}"),
                        leading: "\u{21ba}".to_string(),
                        pending: true,
                        disabled: true,
                        ..Default::default()
                    },
                    rank: rank.saturating_add(row),
                    pre_filtered: true,
                    ..Default::default()
                },
                ResumePending,
                ChildOf(target),
            ));
        }
    }

    fn spawn(&mut self, target: Entity, sessions: &[ResumableSessionEntry], filter: &str) {
        for (rank, (row, session)) in ResumeRows::rows(sessions, filter).into_iter().enumerate() {
            self.commands.spawn((
                Name::new(format!("Resume command-bar row: {}", session.sid)),
                CommandBarContribution {
                    row,
                    rank: rank as i32,
                    close: true,
                    pre_filtered: true,
                    ..Default::default()
                },
                ResumeContribution(session),
                ChildOf(target),
            ));
        }
    }
}

struct ResumeQuery;

impl ResumeQuery {
    fn parse(query: &str) -> Option<&str> {
        let query = query.trim();
        let command = query.strip_prefix('/')?;
        let (name, rest) = command.split_once(' ').unwrap_or((command, ""));
        "resume"
            .starts_with(&name.to_ascii_lowercase())
            .then_some(rest.trim())
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ResumeSection {
    agent: String,
    project: String,
    branch: String,
    count: usize,
}

pub(super) struct ResumeRows;

impl ResumeRows {
    pub(super) fn project(sessions: &[ResumableSessionEntry]) -> Vec<CommandBarResultItem> {
        Self::rows(sessions, "")
            .into_iter()
            .map(|(row, _)| row)
            .collect()
    }

    fn rows(
        sessions: &[ResumableSessionEntry],
        filter: &str,
    ) -> Vec<(CommandBarResultItem, ResumableSessionEntry)> {
        let needle = filter.to_ascii_lowercase();
        let mut groups: Vec<(ResumeSection, Vec<ResumableSessionEntry>)> = Vec::new();
        for session in sessions {
            if !needle.is_empty() && !ResumeRow(session).matches(&needle) {
                continue;
            }
            let section = ResumeSection::of(session);
            if let Some((_, entries)) = groups.iter_mut().find(|(held, _)| *held == section) {
                entries.push(session.clone());
            } else {
                groups.push((section, vec![session.clone()]));
            }
        }
        let mut rows = Vec::new();
        for (mut section, entries) in groups {
            section.count = entries.len();
            for (index, session) in entries.into_iter().enumerate() {
                let row = ResumeRow(&session).project((index == 0).then_some(&section));
                rows.push((row, session));
            }
        }
        rows
    }
}

impl ResumeSection {
    fn of(session: &ResumableSessionEntry) -> Self {
        Self {
            agent: if session.agent_name.is_empty() {
                session.kind.clone()
            } else {
                session.agent_name.clone()
            },
            project: if session.project.is_empty() {
                session.subtitle.clone()
            } else {
                session.project.clone()
            },
            branch: session.branch.clone(),
            count: 0,
        }
    }
}

struct ResumeRow<'a>(&'a ResumableSessionEntry);

impl ResumeRow<'_> {
    fn matches(&self, needle: &str) -> bool {
        let session = self.0;
        session.title.to_ascii_lowercase().contains(needle)
            || session.latest.to_ascii_lowercase().contains(needle)
            || session.subtitle.to_ascii_lowercase().contains(needle)
            || session.agent_name.to_ascii_lowercase().contains(needle)
            || session.project.to_ascii_lowercase().contains(needle)
            || session.branch.to_ascii_lowercase().contains(needle)
    }

    fn project(&self, section: Option<&ResumeSection>) -> CommandBarResultItem {
        let session = self.0;
        CommandBarResultItem {
            key: format!("resume:{}:{}", session.kind, session.sid),
            leading: "\u{21ba}".to_string(),
            title: session.title.clone(),
            subtitle: ResumePreview::after_title(&session.title, &session.latest)
                .unwrap_or_default()
                .to_string(),
            trailing: SessionWhen::new(session.age_seconds, &session.updated_at).label(),
            section: section.map(|section| CommandBarSection {
                labels: [
                    section.agent.as_str(),
                    section.project.as_str(),
                    section.branch.as_str(),
                ]
                .into_iter()
                .filter(|label| !label.is_empty())
                .map(str::to_string)
                .collect(),
                count: section.count as u32,
            }),
            ..Default::default()
        }
    }
}

struct ResumePreview;

impl ResumePreview {
    fn after_title<'a>(title: &str, latest: &'a str) -> Option<&'a str> {
        let title = title.trim();
        let latest = latest.trim();
        if latest.is_empty() || latest == title {
            return None;
        }
        if title.is_empty() {
            return Some(latest);
        }
        let Some(remainder) = latest.strip_prefix(title) else {
            return Some(latest);
        };
        let remainder = remainder.trim_start_matches(|character: char| {
            character.is_whitespace()
                || matches!(character, '.' | ':' | '-' | '\u{2014}' | '\u{00b7}' | '|')
        });
        (!remainder.is_empty()).then_some(remainder)
    }
}

struct SessionWhen<'a> {
    age_seconds: u64,
    updated_at: &'a str,
}

impl<'a> SessionWhen<'a> {
    fn new(age_seconds: u64, updated_at: &'a str) -> Self {
        Self {
            age_seconds,
            updated_at,
        }
    }

    fn label(self) -> String {
        if self.age_seconds >= SessionAge::DAY && !self.updated_at.is_empty() {
            return self
                .updated_at
                .split_whitespace()
                .next()
                .unwrap_or(self.updated_at)
                .to_string();
        }
        SessionAge(self.age_seconds).label()
    }
}

struct SessionAge(u64);

impl SessionAge {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * Self::MINUTE;
    const DAY: u64 = 24 * Self::HOUR;
    const WEEK: u64 = 7 * Self::DAY;
    const MONTH: u64 = 30 * Self::DAY;
    const YEAR: u64 = 365 * Self::DAY;

    fn label(self) -> String {
        let (id, count) = if self.0 < Self::MINUTE {
            ("resume-age-now", 0)
        } else if self.0 < Self::HOUR {
            ("resume-age-minutes", self.0 / Self::MINUTE)
        } else if self.0 < Self::DAY {
            ("resume-age-hours", self.0 / Self::HOUR)
        } else if self.0 < Self::WEEK {
            ("resume-age-days", self.0 / Self::DAY)
        } else if self.0 < Self::MONTH {
            ("resume-age-weeks", self.0 / Self::WEEK)
        } else if self.0 < Self::YEAR {
            ("resume-age-months", self.0 / Self::MONTH)
        } else {
            ("resume-age-years", self.0 / Self::YEAR)
        };
        if count == 0 {
            return translate(id);
        }
        translate_with(id, &[("count", TranslationValue::Number(count as i64))])
    }
}
