use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use vmux_api::protocol::ProcessId;
use vmux_ecs::PageMetadata;
use vmux_ecs::page::PagePlacementCatalog;
use vmux_git::GitDiffSource;
use vmux_layout::pane::Pane;
use vmux_layout::stack::Stack;
use vmux_layout::tab::Tab;

#[cfg(test)]
use std::path::Path;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct AgentFileLayout<'w, 's> {
    agent_terms: Query<'w, 's, (Entity, &'static ProcessId, &'static ChildOf)>,
    child_of: Query<'w, 's, &'static ChildOf>,
    file_pages: Query<
        'w,
        's,
        (
            Entity,
            &'static ChildOf,
            &'static PageMetadata,
            Option<&'static GitDiffSource>,
        ),
    >,
    pane_children: Query<'w, 's, &'static Children, With<Pane>>,
    stack_q: Query<'w, 's, Entity, With<Stack>>,
    tabs: Query<'w, 's, (), With<Tab>>,
    placements: PagePlacementCatalog<'w, 's>,
}

#[derive(Clone, Copy)]
pub(super) struct FilePageTarget {
    pub(super) stack: Entity,
    pub(super) pane: Entity,
    pub(super) navigate: bool,
}

impl AgentFileLayout<'_, '_> {
    fn is_file(&self, url: &str) -> bool {
        self.placements
            .same_group(url, vmux_editor::EditorPlugin::URL)
    }

    pub(super) fn reuses(&self, request_url: &str, existing_url: &str) -> bool {
        self.placements.reuses(request_url, existing_url)
    }

    pub(super) fn agent_pane(&self, anchor: ProcessId) -> Option<Entity> {
        let (_, _, term_co) = self
            .agent_terms
            .iter()
            .find(|(_, pid, _)| **pid == anchor)?;
        self.child_of.get(term_co.get()).ok().map(|co| co.get())
    }

    pub(super) fn ancestor_tab(&self, entity: Entity) -> Option<Entity> {
        let mut current = entity;
        loop {
            if self.tabs.contains(current) {
                return Some(current);
            }
            current = self.child_of.get(current).ok()?.get();
        }
    }

    fn stack_has_file_page(&self, stack: Entity) -> bool {
        self.file_pages.iter().any(|(_, child_of, metadata, _)| {
            child_of.get() == stack && self.is_file(&metadata.url)
        })
    }

    fn pane_has_only_file_stacks(&self, pane: Entity) -> bool {
        let Ok(children) = self.pane_children.get(pane) else {
            return false;
        };
        let mut found = false;
        for stack in children
            .iter()
            .filter(|stack| self.stack_q.contains(*stack))
        {
            found = true;
            if !self.stack_has_file_page(stack) {
                return false;
            }
        }
        found
    }

    fn file_panes_for(&self, agent_pane: Entity) -> Vec<Entity> {
        let Some(agent_tab) = self.ancestor_tab(agent_pane) else {
            return Vec::new();
        };
        let agent_parent = self.child_of.get(agent_pane).ok().map(Relationship::get);
        let mut panes = Vec::new();
        for (_, page_child, metadata, _) in self.file_pages.iter() {
            if !self.is_file(&metadata.url) {
                continue;
            }
            let stack = page_child.get();
            let Ok(pane_child) = self.child_of.get(stack) else {
                continue;
            };
            let pane = pane_child.get();
            if pane == agent_pane
                || self.ancestor_tab(pane) != Some(agent_tab)
                || !self.pane_has_only_file_stacks(pane)
                || panes.contains(&pane)
            {
                continue;
            }
            panes.push(pane);
        }
        panes.sort_by_key(|pane| {
            let direct = self.child_of.get(*pane).ok().map(Relationship::get) == agent_parent;
            !direct
        });
        panes
    }

    pub(super) fn file_page_for(&self, agent_pane: Entity) -> Option<(Entity, Entity)> {
        let pane = self.file_panes_for(agent_pane).into_iter().next()?;
        for (page, page_co, meta, _) in self.file_pages.iter() {
            if !self.is_file(&meta.url) {
                continue;
            }
            let Ok(pane_co) = self.child_of.get(page_co.get()) else {
                continue;
            };
            if pane_co.get() == pane {
                return Some((page, pane));
            }
        }
        None
    }

    pub(super) fn file_page_target(&self, agent_pane: Entity, url: &str) -> Option<FilePageTarget> {
        let panes = self.file_panes_for(agent_pane);
        for pane in &panes {
            for (_, page_co, meta, diff) in self.file_pages.iter() {
                let stack = page_co.get();
                if !self.is_file(&meta.url)
                    || self.child_of.get(stack).ok().map(Relationship::get) != Some(*pane)
                    || !self.reuses(url, &meta.url)
                {
                    continue;
                }
                let dirty = diff.is_some_and(|source| source.dirty);
                return Some(FilePageTarget {
                    stack,
                    pane: *pane,
                    navigate: !dirty && meta.url != url,
                });
            }
        }
        for pane in panes {
            for (_, page_co, meta, diff) in self.file_pages.iter() {
                let stack = page_co.get();
                if !self.is_file(&meta.url)
                    || self.child_of.get(stack).ok().map(Relationship::get) != Some(pane)
                    || diff.is_some_and(|source| source.dirty)
                {
                    continue;
                }
                return Some(FilePageTarget {
                    stack,
                    pane,
                    navigate: true,
                });
            }
        }
        None
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn file_stacks_for(
        &self,
        agent_pane: Entity,
    ) -> Option<(Entity, Vec<(Entity, Entity, String)>)> {
        let follow_pane = self.file_panes_for(agent_pane).into_iter().next()?;
        let mut stacks = Vec::new();
        for (page, page_co, meta, _) in self.file_pages.iter() {
            if !self.is_file(&meta.url) {
                continue;
            }
            let stack = page_co.get();
            let Ok(pane_co) = self.child_of.get(stack) else {
                continue;
            };
            let pane = pane_co.get();
            if pane != follow_pane {
                continue;
            }
            stacks.push((stack, page, meta.url.clone()));
        }
        Some((follow_pane, stacks))
    }
}

#[cfg(test)]
pub(super) struct TestRepo(tempfile::TempDir);

#[cfg(test)]
impl TestRepo {
    pub(super) fn new(name: &str) -> Self {
        let prefix = format!("vmux-agent-{name}-");
        let directory = tempfile::Builder::new().prefix(&prefix).tempdir().unwrap();
        let repo = Self(directory);
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "t@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.path().join("seed.txt"), "seed\n").unwrap();
        repo.git(&["add", "seed.txt"]);
        repo.git(&["commit", "-qm", "init"]);
        repo
    }

    pub(super) fn path(&self) -> &Path {
        self.0.path()
    }

    fn git(&self, args: &[&str]) {
        let status = std::process::Command::new("git")
            .current_dir(self.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
