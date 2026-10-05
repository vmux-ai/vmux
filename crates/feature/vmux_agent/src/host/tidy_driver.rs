use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::event::FileTidyState;
use vmux_ecs::{FileUiStateWrite, LastActivatedAt};
use vmux_git::GitRepository;
use vmux_layout::CloseStackRequest;
use vmux_path::FileUrl;
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
        let mut repos: Vec<(PathBuf, HashSet<String>)> = Vec::new();
        let rows: Vec<(Entity, i64, bool)> = stacks
            .iter()
            .map(|(stack, _page, url)| {
                let timestamp = self
                    .last_activated
                    .get(*stack)
                    .map(|timestamp| timestamp.0)
                    .unwrap_or(i64::MIN);
                let changed = FileUrl::parse(url)
                    .and_then(|url| url.path())
                    .map(|path| TidyPolicy::changed(&path, &mut repos))
                    .unwrap_or(false);
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

    fn changed(abs: &Path, repos: &mut Vec<(PathBuf, HashSet<String>)>) -> bool {
        let abs = abs.canonicalize().unwrap_or_else(|_| abs.to_path_buf());
        if let Some((root, set)) = repos.iter().find(|(root, _)| abs.starts_with(root)) {
            return set.contains(&Self::relative(root, &abs));
        }
        match GitRepository::discover(&abs)
            .and_then(|repository| repository.dirty_paths().map(|set| (repository, set)))
        {
            Ok((repository, set)) => {
                let root = repository.path().to_path_buf();
                let changed = set.contains(&Self::relative(&root, &abs));
                repos.push((root, set));
                changed
            }
            Err(_) => false,
        }
    }

    fn relative(root: &Path, abs: &Path) -> String {
        abs.strip_prefix(root)
            .map(|relative| relative.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
