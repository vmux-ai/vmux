#[cfg(test)]
use std::path::PathBuf;

use bevy::ecs::relationship::Relationship;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_ecs::event::{FileTidyRequest, FileTidyState, TidyChoice};
use vmux_ecs::notify::AgentAttention;
use vmux_ecs::team::Agent;
use vmux_ecs::{FileUiStateWrite, LastActivatedAt, ProcessAnchor, ProcessId};
use vmux_layout::CloseStackRequest;
use vmux_layout::stack::ComputeFocusSet;
#[cfg(test)]
use vmux_path::FileUrl;
use vmux_session::RunState;
use vmux_setting::{AppSettings, SettingsSaveRequest};

use super::follow_driver::AgentFileLayout;

pub(super) struct TidyPlugin;

impl Plugin for TidyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AgentAttention>()
            .add_message::<CloseStackRequest>()
            .add_message::<SettingsSaveRequest>()
            .add_plugins(UiEventPlugin::<(FileTidyRequest,)>::default())
            .add_observer(request)
            .add_systems(
                Update,
                attention
                    .in_set(TidySet)
                    .after(ComputeFocusSet)
                    .after(crate::host::attention::TurnEndedSet),
            )
            .add_systems(Update, idle.after(ComputeFocusSet));
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TidySet;

#[derive(Component)]
struct PendingTidy {
    closable: Vec<Entity>,
}

impl PendingTidy {
    fn select_closable(stacks: &[(Entity, i64, bool)], max: usize) -> Vec<Entity> {
        if stacks.len() <= max {
            return Vec::new();
        }
        let active = stacks
            .iter()
            .max_by_key(|(_, timestamp, _)| *timestamp)
            .map(|(stack, _, _)| *stack);
        let mut closable = Vec::new();
        for (stack, _, changed) in stacks {
            if Some(*stack) != active && !*changed {
                closable.push(*stack);
            }
        }
        closable
    }
}

#[derive(SystemParam)]
struct TidyFiles<'w, 's> {
    layout: AgentFileLayout<'w, 's>,
    last_activated: Query<'w, 's, &'static LastActivatedAt>,
    file_git: Query<'w, 's, &'static vmux_git::FileGit>,
    diff_sources: Query<'w, 's, &'static vmux_git::GitDiffSource>,
    pending: Query<'w, 's, (), With<PendingTidy>>,
    close: MessageWriter<'w, CloseStackRequest>,
    commands: Commands<'w, 's>,
}

impl TidyFiles<'_, '_> {
    fn agent_pane(&self, process: ProcessId) -> Option<Entity> {
        self.layout.agent_pane(process)
    }

    fn run(&mut self, agent_pane: Entity, settings: &AppSettings) {
        let Some((follow_pane, stacks)) = self.layout.file_stacks_for(agent_pane) else {
            return;
        };
        if self.pending.get(follow_pane).is_ok() {
            return;
        }
        let mut rows = Vec::with_capacity(stacks.len());
        for (stack, page, _) in &stacks {
            let timestamp = self
                .last_activated
                .get(*stack)
                .map(|timestamp| timestamp.0)
                .unwrap_or(i64::MIN);
            let changed = self
                .diff_sources
                .get(*page)
                .is_ok_and(|source| source.dirty)
                || self
                    .file_git
                    .get(*page)
                    .ok()
                    .and_then(vmux_git::FileGit::has_changes)
                    .unwrap_or(true);
            rows.push((*stack, timestamp, changed));
        }
        let closable = PendingTidy::select_closable(&rows, settings.agent.tidy_files_max);
        if closable.is_empty() {
            return;
        }
        if settings.agent.tidy_files_auto {
            for stack in closable {
                self.close.write(CloseStackRequest::tidying(stack));
            }
            return;
        }
        let count = closable.len() as u32;
        let active_page = stacks
            .iter()
            .max_by_key(|(stack, _, _)| {
                self.last_activated
                    .get(*stack)
                    .map(|timestamp| timestamp.0)
                    .unwrap_or(i64::MIN)
            })
            .map(|(_, page, _)| *page);
        if let Some(page) = active_page {
            self.commands.trigger(FileUiStateWrite::from_event(
                page,
                &FileTidyState { count: Some(count) },
            ));
            self.commands
                .entity(follow_pane)
                .insert(PendingTidy { closable });
        }
    }
}

fn request(
    trigger: On<UiInput<FileTidyRequest>>,
    child_of: Query<&ChildOf>,
    pending: Query<&PendingTidy>,
    settings: Option<ResMut<AppSettings>>,
    mut save: MessageWriter<SettingsSaveRequest>,
    mut close: MessageWriter<CloseStackRequest>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    commands.trigger(FileUiStateWrite::from_event(
        webview,
        &FileTidyState::default(),
    ));
    let Some(mut settings) = settings else {
        return;
    };
    let Ok(stack) = child_of.get(webview).map(Relationship::get) else {
        return;
    };
    let Ok(pane) = child_of.get(stack).map(Relationship::get) else {
        return;
    };
    let Ok(pending_tidy) = pending.get(pane) else {
        return;
    };
    let closable = pending_tidy.closable.clone();
    commands.entity(pane).remove::<PendingTidy>();
    match trigger.event().payload.choice {
        TidyChoice::Dismiss => {}
        TidyChoice::Always => {
            settings.agent.tidy_files_auto = true;
            save.write(SettingsSaveRequest);
            for stack in closable {
                close.write(CloseStackRequest::tidying(stack));
            }
        }
        TidyChoice::Tidy => {
            for stack in closable {
                close.write(CloseStackRequest::tidying(stack));
            }
        }
    }
}

fn attention(
    mut reader: MessageReader<AgentAttention>,
    settings: Option<Res<AppSettings>>,
    agents: Query<&ProcessId, With<Agent>>,
    mut tidy: TidyFiles,
) {
    let Some(settings) = settings else {
        for _ in reader.read() {}
        return;
    };
    if !settings.agent.tidy_files {
        for _ in reader.read() {}
        return;
    }
    for attention in reader.read() {
        let Ok(process) = agents.get(attention.entity) else {
            continue;
        };
        let Some(agent_pane) = tidy.agent_pane(*process) else {
            continue;
        };
        tidy.run(agent_pane, &settings);
    }
}

fn idle(
    settings: Option<Res<AppSettings>>,
    sessions: Query<(&ProcessAnchor, &RunState), Changed<RunState>>,
    mut tidy: TidyFiles,
) {
    let Some(settings) = settings else {
        return;
    };
    if !settings.agent.tidy_files {
        return;
    }
    for (anchor, state) in &sessions {
        if !matches!(state, RunState::Idle) {
            continue;
        }
        let Some(agent_pane) = tidy.agent_pane(anchor.0) else {
            continue;
        };
        tidy.run(agent_pane, &settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_file_urls() {
        assert_eq!(
            FileUrl::parse("file:///a/b.rs#L3:1-4").and_then(|url| url.path()),
            Some(PathBuf::from("/a/b.rs"))
        );
        assert_eq!(
            FileUrl::parse("file:///a/my%20file.rs").and_then(|url| url.path()),
            Some(PathBuf::from("/a/my file.rs"))
        );
        assert!(FileUrl::parse("file:/rel#x").is_none());
        assert!(FileUrl::parse("https://x/y").is_none());
        assert_eq!(FileUrl::parse("file://").and_then(|url| url.path()), None);
    }

    #[test]
    fn decide_closable_below_threshold_is_empty() {
        let mut w = World::new();
        let ids: Vec<Entity> = (0..3).map(|_| w.spawn_empty().id()).collect();
        let stacks = vec![
            (ids[0], 10, false),
            (ids[1], 20, false),
            (ids[2], 30, false),
        ];
        assert!(PendingTidy::select_closable(&stacks, 5).is_empty());
    }

    #[test]
    fn decide_closable_keeps_changed_and_active() {
        let mut w = World::new();
        let ids: Vec<Entity> = (0..6).map(|_| w.spawn_empty().id()).collect();
        let stacks = vec![
            (ids[0], 10, false),
            (ids[1], 20, true),
            (ids[2], 30, false),
            (ids[3], 40, true),
            (ids[4], 50, false),
            (ids[5], 60, false),
        ];
        let mut got = PendingTidy::select_closable(&stacks, 5);
        got.sort();
        let mut want = vec![ids[0], ids[2], ids[4]];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn decide_closable_empty_when_all_changed() {
        let mut w = World::new();
        let ids: Vec<Entity> = (0..6).map(|_| w.spawn_empty().id()).collect();
        let stacks: Vec<(Entity, i64, bool)> = ids
            .iter()
            .enumerate()
            .map(|(i, &e)| (e, i as i64, true))
            .collect();
        assert!(PendingTidy::select_closable(&stacks, 5).is_empty());
    }
}
