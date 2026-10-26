use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::event::FileTidyState;
use vmux_ecs::{LastActivatedAt, UiFileWrite};
use vmux_layout::CloseStackRequest;
use vmux_setting::AppSettings;

use super::follow_driver::AgentFileLayout;

#[derive(Component)]
pub(super) struct PendingTidy {
    pub(super) closable: Vec<Entity>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct TidyFiles<'w, 's> {
    layout: AgentFileLayout<'w, 's>,
    last_activated: Query<'w, 's, &'static LastActivatedAt>,
    file_git: Query<'w, 's, &'static vmux_git::FileGit>,
    diff_sources: Query<'w, 's, &'static vmux_git::GitDiffSource>,
    pending: Query<'w, 's, (), With<PendingTidy>>,
    close: MessageWriter<'w, CloseStackRequest>,
    commands: Commands<'w, 's>,
}

pub(super) struct TidyPolicy;

impl TidyFiles<'_, '_> {
    pub(super) fn agent_pane(&self, process: ProcessId) -> Option<Entity> {
        self.layout.agent_pane(process)
    }

    pub(super) fn run(&mut self, agent_pane: Entity, settings: &AppSettings) {
        let Some((follow_pane, stacks)) = self.layout.file_stacks_for(agent_pane) else {
            return;
        };
        if self.pending.get(follow_pane).is_ok() {
            return;
        }
        let rows: Vec<(Entity, i64, bool)> = stacks
            .iter()
            .map(|(stack, page, _url)| {
                let timestamp = self
                    .last_activated
                    .get(*stack)
                    .map(|timestamp| timestamp.milliseconds())
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
                (*stack, timestamp, changed)
            })
            .collect();
        let closable = TidyPolicy::closable(&rows, settings.agent.tidy_files_max);
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
                    .map(|timestamp| timestamp.milliseconds())
                    .unwrap_or(i64::MIN)
            })
            .map(|(_, page, _)| *page);
        if let Some(page) = active_page {
            self.commands.trigger(UiFileWrite::from_event(
                page,
                &FileTidyState { count: Some(count) },
            ));
            self.commands
                .entity(follow_pane)
                .insert(PendingTidy { closable });
        }
    }
}

impl TidyPolicy {
    pub(super) fn closable(stacks: &[(Entity, i64, bool)], max: usize) -> Vec<Entity> {
        if stacks.len() <= max {
            return Vec::new();
        }
        let active = stacks
            .iter()
            .max_by_key(|(_, timestamp, _)| *timestamp)
            .map(|(stack, _, _)| *stack);
        stacks
            .iter()
            .filter(|(stack, _, changed)| Some(*stack) != active && !changed)
            .map(|(stack, _, _)| *stack)
            .collect()
    }
}
