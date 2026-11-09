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
    loading: bool,
    visible: std::collections::HashMap<String, usize>,
}

#[derive(Component)]
struct ResumeContribution(ResumableSessionEntry);

#[derive(Component)]
struct ResumePending;

#[derive(Component)]
struct ResumeMore(String, usize);

type ResumeEntities<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static ChildOf),
    Or<(
        With<ResumeContribution>,
        With<ResumePending>,
        With<ResumeMore>,
    )>,
>;

fn query(
    trigger: On<CommandBarQueryChanged>,
    mut states: Query<&mut ResumeState>,
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
    if let Ok(mut state) = states.get_mut(event.target) {
        if state.filter != filter {
            state.visible.clear();
        }
        state.filter = filter.to_string();
        results.spawn(event.target, &state.sessions, &state.filter, &state.visible);
        if state.loading {
            results.pending(event.target, state.sessions.len() as i32);
        }
        return;
    }
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
        loading: true,
        visible: Default::default(),
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
    results.spawn(target, &state.sessions, &state.filter, &state.visible);
    let loaded = state.sessions.len() as u32;
    if loaded >= state.total {
        state.loading = false;
        return;
    }
    state.offset = loaded;
    state.loading = true;
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
    more: Query<&ResumeMore>,
    mut states: Query<&mut ResumeState>,
    mut results: ResumeResults,
    mut commands: Commands,
) {
    let event = trigger.event();
    if let Ok(more) = more.get(event.target) {
        let Ok(mut state) = states.get_mut(event.webview) else {
            return;
        };
        let count = state.visible.entry(more.0.clone()).or_insert(more.1);
        *count = count.saturating_add(5);
        results.clear(event.webview);
        results.spawn(
            event.webview,
            &state.sessions,
            &state.filter,
            &state.visible,
        );
        return;
    }
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
            open: None,
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

    fn spawn(
        &mut self,
        target: Entity,
        sessions: &[ResumableSessionEntry],
        filter: &str,
        limits: &std::collections::HashMap<String, usize>,
    ) {
        let mut rank = 0i32;
        let mut counts = std::collections::HashMap::<String, usize>::new();
        let all = ResumeRows::rows(sessions, filter);
        for (_, session) in &all {
            *counts.entry(ResumeSection::of(session).key()).or_default() += 1;
        }
        let mut emitted = std::collections::HashMap::<String, usize>::new();
        let mut more_done = std::collections::HashSet::new();
        for (mut row, session) in all {
            let section = ResumeSection::of(&session);
            let key = section.key();
            let limit = limits
                .get(&key)
                .copied()
                .unwrap_or(if section.collapsed { 0 } else { 5 });
            let shown = emitted.entry(key.clone()).or_default();
            if *shown >= limit {
                if more_done.insert(key.clone()) {
                    self.commands.spawn((
                        CommandBarContribution {
                            row: CommandBarResultItem {
                                key: format!("resume:more:{key}"),
                                title: translate("common-show-more"),
                                trailing: counts[&key].saturating_sub(limit).to_string(),
                                section: (*shown == 0).then(|| CommandBarSection {
                                    labels: vec![section.label.clone()],
                                    count: counts[&key] as u32,
                                }),
                                ..Default::default()
                            },
                            rank,
                            close: false,
                            pre_filtered: true,
                            ..Default::default()
                        },
                        ResumeMore(key, limit),
                        ChildOf(target),
                    ));
                    rank += 1;
                }
                continue;
            }
            if *shown == 0 {
                row.section = Some(CommandBarSection {
                    labels: vec![section.label.clone()],
                    count: counts[&key] as u32,
                });
            }
            *shown += 1;
            self.commands.spawn((
                Name::new(format!("Resume command-bar row: {}", session.sid)),
                CommandBarContribution {
                    row,
                    rank,
                    close: true,
                    pre_filtered: true,
                    ..Default::default()
                },
                ResumeContribution(session),
                ChildOf(target),
            ));
            rank += 1;
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
    status: String,
    label: String,
    order: (u8, u32),
    collapsed: bool,
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
        groups.sort_by_key(|(section, _)| section.order);
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
    fn key(&self) -> String {
        self.status.clone()
    }

    fn of(session: &ResumableSessionEntry) -> Self {
        Self {
            status: if session.stale {
                "stale".to_string()
            } else if session.stage.is_empty() {
                "recent".to_string()
            } else {
                session.stage.clone()
            },
            label: if session.stale {
                translate("sessions-stale")
            } else if session.stage_name.is_empty() {
                translate("sessions-title")
            } else {
                vmux_ui::i18n::translate_humanized(&session.stage_name)
            },
            order: (
                if session.stale {
                    3
                } else if session.terminal {
                    2
                } else {
                    0
                },
                if session.stale {
                    0
                } else {
                    session.stage_order
                },
            ),
            collapsed: session.stale || session.terminal,
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
            || session.stage_name.to_ascii_lowercase().contains(needle)
            || session.runtime.to_ascii_lowercase().contains(needle)
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
            badge: if session.stage_name.is_empty() {
                String::new()
            } else {
                vmux_ui::i18n::translate_humanized(&session.stage_name)
            },
            detail: [
                session.agent_name.as_str(),
                session.project.as_str(),
                session.branch.as_str(),
            ]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
            section: section.map(|section| CommandBarSection {
                labels: vec![section.label.clone()],
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

#[cfg(test)]
mod tests {
    use super::*;

    struct ResumePicker {
        app: App,
        view: Entity,
    }

    impl ResumePicker {
        fn loaded() -> Self {
            let mut app = App::new();
            app.add_plugins(Plugin);
            let view = app.world_mut().spawn_empty().id();
            let mut picker = Self { app, view };
            picker.query("/resume");
            let mut sessions = Vec::new();
            for (stage, count, terminal, stale) in [
                ("active", 8, false, false),
                ("waiting", 7, false, false),
                ("done", 4, true, false),
                ("old", 4, false, true),
            ] {
                for index in 0..count {
                    sessions.push(ResumableSessionEntry {
                        kind: "session".to_string(),
                        sid: format!("{stage}-{index}"),
                        title: format!("Task {stage} {index}"),
                        stage: stage.to_string(),
                        stage_name: stage.to_string(),
                        stage_order: if stale { index } else { 0 },
                        terminal,
                        stale,
                        ..Default::default()
                    });
                }
            }
            picker.app.world_mut().trigger(
                UiStateWrite::<vmux_api::command_bar::CommandBarUiState>::from_event(
                    picker.view,
                    &ResumableSessions {
                        request_id: 1,
                        total: sessions.len() as u32,
                        sessions,
                        ..Default::default()
                    },
                ),
            );
            picker.app.update();
            picker
        }

        fn query(&mut self, query: &str) {
            self.app.world_mut().trigger(CommandBarQueryChanged {
                target: self.view,
                open_id: vmux_api::command_bar::OpenId(1),
                query: query.to_string(),
                start: true,
                open: None,
                picker: None,
                numbered: false,
            });
            self.app.update();
        }

        fn rows(&mut self) -> Vec<CommandBarContribution> {
            let mut rows = Vec::new();
            let mut query = self
                .app
                .world_mut()
                .query::<(&ChildOf, &CommandBarContribution)>();
            for (parent, contribution) in query.iter(self.app.world()) {
                if parent.parent() == self.view {
                    rows.push(contribution.clone());
                }
            }
            rows.sort_by_key(|row| row.rank);
            rows
        }

        fn more(&mut self, key: &str) {
            let mut query = self
                .app
                .world_mut()
                .query::<(Entity, &ChildOf, &ResumeMore)>();
            let target = query
                .iter(self.app.world())
                .find(|(_, parent, more)| parent.parent() == self.view && more.0 == key)
                .unwrap()
                .0;
            self.app
                .world_mut()
                .trigger(CommandBarContributionActivated {
                    target,
                    webview: self.view,
                    query: "/resume".to_string(),
                    open: None,
                });
            self.app.update();
        }
    }

    #[test]
    fn resume_limits_selectable_rows_and_expands_only_the_requested_status() {
        let mut picker = ResumePicker::loaded();
        let rows = picker.rows();
        assert_eq!(rows.len(), 14);
        let mut totals = Vec::new();
        for row in &rows {
            if let Some(section) = &row.row.section {
                totals.push(section.count);
            }
            assert!(!row.row.pending);
        }
        assert_eq!(totals, vec![8, 7, 4, 4]);
        assert!(
            !rows
                .iter()
                .any(|row| row.row.key == "resume:session:active-5")
        );
        assert!(
            !rows
                .iter()
                .any(|row| row.row.key == "resume:session:done-0")
        );
        assert!(!rows.iter().any(|row| row.row.key == "resume:session:old-0"));
        picker.more("active");
        let rows = picker.rows();
        assert_eq!(rows.len(), 16);
        assert!(
            rows.iter()
                .any(|row| row.row.key == "resume:session:active-7")
        );
        assert!(
            !rows
                .iter()
                .any(|row| row.row.key == "resume:session:waiting-5")
        );
        assert!(!rows.iter().any(|row| row.row.key == "resume:more:active"));
        assert!(
            rows.iter()
                .filter(|row| row.row.key.starts_with("resume:more:"))
                .all(|row| !row.close)
        );
        picker.more("done");
        assert!(
            picker
                .rows()
                .iter()
                .any(|row| row.row.key == "resume:session:done-3")
        );
    }

    #[test]
    fn changing_the_resume_filter_resets_paging_and_keeps_status_counts_accurate() {
        let mut picker = ResumePicker::loaded();
        picker.more("active");
        picker.query("/resume waiting");
        let rows = picker.rows();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].row.section.as_ref().unwrap().count, 7);
        assert!(rows.iter().all(|row| row.row.key.contains("waiting")));
        picker.query("/resume");
        assert_eq!(picker.rows().len(), 14);
    }
}
