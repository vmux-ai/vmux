use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};

use crate::event::{DiffKind, DiffLine, GitDiffRevealRequest, GitLineMarker, GitLineStatus};
use crate::state::{GitPageControllerState, GitPanel};

use super::GitUpdateSet;
use super::job::DiffJob;
use super::job_runner::GitJob;
use super::repository::GitRepository;

const DIFF_WINDOW_ROWS: u32 = 200_000;

pub(super) struct DiffPlugin;

impl Plugin for DiffPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(UiEventPlugin::<(GitDiffRevealRequest,)>::default())
            .add_observer(file_refresh)
            .add_observer(reveal)
            .add_systems(
                Update,
                (sync_page_targets, start_requests)
                    .chain()
                    .in_set(GitUpdateSet::Diff),
            );
    }
}

fn reveal(
    trigger: On<UiInput<GitDiffRevealRequest>>,
    mut states: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
    )>,
    mut files: super::status::FileGitStates,
) {
    let request = &trigger.event().payload;
    let entity = trigger.event().webview;
    if let Ok((_, mut revealed)) = states.get_mut(entity) {
        let range = (request.start as usize, request.end as usize);
        if range.0 < range.1 && !revealed.0.contains(&range) {
            revealed.0.push(range);
        }
        return;
    }
    files.reveal_diff(entity, request.start, request.end);
}

#[derive(Component, Clone, Debug, Default)]
pub struct GitDiffSource {
    pub content: String,
    pub dirty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitDiffTarget {
    repo_root: PathBuf,
    path: PathBuf,
    path_bytes: Vec<u8>,
    reference: String,
}

impl GitDiffTarget {
    fn for_page(
        state: &super::state::GitState,
        controller: &GitPageControllerState,
    ) -> Option<Self> {
        state.repository()?;
        let repo_root = PathBuf::from(state.workspace());
        if repo_root.as_os_str().is_empty() {
            return None;
        }
        match controller.focused_panel {
            GitPanel::Commits if !controller.selected_commit.is_empty() => Some(Self {
                repo_root,
                path: PathBuf::new(),
                path_bytes: Vec::new(),
                reference: controller.selected_commit.clone(),
            }),
            GitPanel::Files | GitPanel::Stash if !controller.selected_abs_path.is_empty() => {
                Some(Self {
                    repo_root,
                    path: PathBuf::from(&controller.selected_abs_path),
                    path_bytes: controller.selected_path_bytes.clone(),
                    reference: String::new(),
                })
            }
            _ => None,
        }
    }

    fn for_file(file: &super::status::FileGit) -> Option<Self> {
        let repo_root = file.repo_root()?;
        Some(Self {
            repo_root,
            path: file.path().into(),
            path_bytes: Vec::new(),
            reference: String::new(),
        })
    }
}

#[derive(Component)]
struct PendingGitDiff {
    target: GitDiffTarget,
    file: Option<FileDiffIdentity>,
    page_revision: Option<u32>,
}

#[derive(Clone, Copy)]
struct FileDiffIdentity {
    document: u64,
    refresh: u64,
}

#[derive(Component)]
pub(super) struct GitDiffQuery {
    target: GitDiffTarget,
    generation: u64,
    file: Option<FileDiffIdentity>,
    page_revision: Option<u32>,
}

impl GitDiffQuery {
    pub(super) fn accepts(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub(super) fn accepts_file(&self, generation: u64, file: &super::status::FileGit) -> bool {
        self.accepts(generation)
            && self
                .file
                .is_some_and(|identity| file.accepts(identity.document, identity.refresh))
    }
}

#[derive(EntityEvent)]
pub(super) struct FileDiffRefresh {
    #[event_target]
    pub(super) entity: Entity,
}

type GitDiffPages<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Ref<'static, super::state::GitState>,
        Ref<'static, GitPageControllerState>,
        Option<&'static PendingGitDiff>,
        Option<&'static GitDiffQuery>,
    ),
>;

fn sync_page_targets(pages: GitDiffPages, mut commands: Commands) {
    for (entity, state, controller, pending, current) in &pages {
        if !state.is_changed() && !controller.is_changed() {
            continue;
        }
        let Some(target) = GitDiffTarget::for_page(&state, &controller) else {
            commands.entity(entity).remove::<PendingGitDiff>();
            continue;
        };
        let page_revision = state.diff_revision();
        if pending.is_some_and(|pending| {
            pending.target == target && pending.page_revision == Some(page_revision)
        }) || current.is_some_and(|current| {
            current.target == target && current.page_revision == Some(page_revision)
        }) {
            continue;
        }
        commands.entity(entity).insert(PendingGitDiff {
            target,
            file: None,
            page_revision: Some(page_revision),
        });
    }
}

fn file_refresh(
    trigger: On<FileDiffRefresh>,
    files: Query<&super::status::FileGit>,
    mut commands: Commands,
) {
    let Ok(file) = files.get(trigger.entity) else {
        return;
    };
    let Some(target) = GitDiffTarget::for_file(file) else {
        return;
    };
    let (document, refresh) = file.identity();
    commands.entity(trigger.entity).insert(PendingGitDiff {
        target,
        file: Some(FileDiffIdentity { document, refresh }),
        page_revision: None,
    });
}

fn start_requests(
    pending: Query<(
        Entity,
        &PendingGitDiff,
        Option<&GitDiffQuery>,
        Option<&GitDiffSource>,
    )>,
    mut pages: Query<(
        &mut super::state::GitState,
        &mut super::state::GitDiffRevealRanges,
    )>,
    mut files: super::status::FileGitStates,
    mut commands: Commands,
) {
    for (entity, pending, current, source) in &pending {
        let target_changed = current.is_none_or(|current| current.target != pending.target);
        let generation = current
            .map(|current| current.generation)
            .unwrap_or_default()
            .wrapping_add(1)
            .max(1);
        if let Ok((mut page, mut revealed)) = pages.get_mut(entity) {
            page.snapshot.diff_loading = target_changed || page.snapshot.diff_viewport.is_none();
            if target_changed {
                page.snapshot.diff_viewport = None;
                page.snapshot.diff_rows.clear();
                revealed.0.clear();
            }
        } else if files.contains(entity) {
            files.start_diff(entity, target_changed);
        } else {
            commands.entity(entity).remove::<PendingGitDiff>();
            continue;
        }
        let target = pending.target.clone();
        let content = source
            .filter(|source| source.dirty)
            .map(|source| source.content.clone());
        commands
            .entity(entity)
            .remove::<PendingGitDiff>()
            .insert(GitDiffQuery {
                target: target.clone(),
                generation,
                file: pending.file,
                page_revision: pending.page_revision,
            });
        commands.spawn((
            GitJob::new(entity),
            GitRepository::at(target.repo_root),
            DiffJob {
                path: target.path,
                reference: target.reference,
                generation,
                top_line: 0,
                rows: DIFF_WINDOW_ROWS,
                content,
            },
        ));
    }
}

pub(super) struct GitDiffMarkers(Vec<GitLineMarker>);

impl GitDiffMarkers {
    pub(super) fn from_lines(lines: &[DiffLine]) -> Self {
        let mut markers = HashMap::new();
        let mut hunk_kinds = HashMap::<u32, (bool, bool)>::new();
        for line in lines {
            let Some(hunk) = line.hunk else {
                continue;
            };
            let kinds = hunk_kinds.entry(hunk).or_default();
            match line.kind {
                DiffKind::Add => kinds.0 = true,
                DiffKind::Remove => kinds.1 = true,
                _ => {}
            }
        }
        let mut replacement_lines = HashSet::new();
        let mut index = 0;
        while index < lines.len() {
            if matches!(lines[index].kind, DiffKind::Context | DiffKind::Staged)
                || lines[index].hunk.is_some()
            {
                index += 1;
                continue;
            }
            let start = index;
            while index < lines.len()
                && !matches!(lines[index].kind, DiffKind::Context | DiffKind::Staged)
                && lines[index].hunk.is_none()
            {
                index += 1;
            }
            let range = start..index;
            let has_add = range
                .clone()
                .any(|line| matches!(lines[line].kind, DiffKind::Add));
            let has_remove = range
                .clone()
                .any(|line| matches!(lines[line].kind, DiffKind::Remove));
            if has_add && has_remove {
                replacement_lines.extend(range);
            }
        }

        for (index, line) in lines.iter().enumerate() {
            match line.kind {
                DiffKind::Add => {
                    let Some(line_number) = line.new_no else {
                        continue;
                    };
                    let modified = line
                        .hunk
                        .and_then(|hunk| hunk_kinds.get(&hunk))
                        .is_some_and(|(added, removed)| *added && *removed)
                        || replacement_lines.contains(&index);
                    Self::insert(
                        &mut markers,
                        line_number,
                        if modified {
                            GitLineStatus::Modified
                        } else {
                            GitLineStatus::Added
                        },
                    );
                }
                DiffKind::Remove => {
                    let next = lines[index + 1..].iter().find_map(|next| next.new_no);
                    let previous = lines[..index]
                        .iter()
                        .rev()
                        .find_map(|previous| previous.new_no);
                    if let Some(line_number) = next.or(previous) {
                        Self::insert(&mut markers, line_number, GitLineStatus::Deleted);
                    }
                }
                DiffKind::Staged => {
                    if let Some(line_number) = line.new_no {
                        Self::insert(&mut markers, line_number, GitLineStatus::Staged);
                    }
                }
                DiffKind::Context | DiffKind::Hunk => {}
            }
        }
        let mut markers = markers
            .into_iter()
            .map(|(line, status)| GitLineMarker { line, status })
            .collect::<Vec<_>>();
        markers.sort_by_key(|marker| marker.line);
        Self(markers)
    }

    pub(super) fn into_inner(self) -> Vec<GitLineMarker> {
        self.0
    }

    fn insert(markers: &mut HashMap<u32, GitLineStatus>, line: u32, status: GitLineStatus) {
        let priority = |status| match status {
            GitLineStatus::Staged => 0,
            GitLineStatus::Deleted => 1,
            GitLineStatus::Added => 2,
            GitLineStatus::Modified => 3,
        };
        match markers.get(&line) {
            Some(current) if priority(*current) >= priority(status) => {}
            _ => {
                markers.insert(line, status);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::StyledSpan;

    fn line(kind: DiffKind, no: u32) -> DiffLine {
        DiffLine {
            kind,
            old_no: Some(no),
            new_no: Some(no),
            hunk: None,
            spans: Vec::<StyledSpan>::new(),
        }
    }

    #[test]
    fn markers_classify_modified_added_and_deleted_lines() {
        let lines = vec![
            DiffLine {
                kind: DiffKind::Remove,
                old_no: Some(2),
                new_no: None,
                hunk: None,
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Add,
                old_no: None,
                new_no: Some(2),
                hunk: None,
                spans: Vec::new(),
            },
            line(DiffKind::Context, 3),
            DiffLine {
                kind: DiffKind::Add,
                old_no: None,
                new_no: Some(8),
                hunk: Some(1),
                spans: Vec::new(),
            },
            DiffLine {
                kind: DiffKind::Remove,
                old_no: Some(12),
                new_no: None,
                hunk: Some(2),
                spans: Vec::new(),
            },
            line(DiffKind::Context, 12),
        ];

        let markers = GitDiffMarkers::from_lines(&lines).into_inner();

        assert!(markers.contains(&GitLineMarker {
            line: 2,
            status: GitLineStatus::Modified,
        }));
        assert!(markers.contains(&GitLineMarker {
            line: 8,
            status: GitLineStatus::Added,
        }));
        assert!(markers.contains(&GitLineMarker {
            line: 12,
            status: GitLineStatus::Deleted,
        }));
    }

    #[test]
    fn a_new_target_supersedes_the_previous_diff_generation() {
        let mut app = App::new();
        app.add_systems(Update, start_requests);
        let entity = app
            .world_mut()
            .spawn((
                super::super::state::GitState::default(),
                PendingGitDiff {
                    target: GitDiffTarget {
                        repo_root: "/repo".into(),
                        path: "/repo/a.rs".into(),
                        path_bytes: Vec::new(),
                        reference: String::new(),
                    },
                    file: None,
                    page_revision: None,
                },
            ))
            .id();

        app.update();
        assert!(
            app.world()
                .get::<GitDiffQuery>(entity)
                .is_some_and(|query| query.accepts(1))
        );

        app.world_mut().entity_mut(entity).insert(PendingGitDiff {
            target: GitDiffTarget {
                repo_root: "/repo".into(),
                path: "/repo/b.rs".into(),
                path_bytes: Vec::new(),
                reference: String::new(),
            },
            file: None,
            page_revision: None,
        });
        app.update();

        let query = app.world().get::<GitDiffQuery>(entity).unwrap();
        assert!(query.accepts(2));
        assert!(!query.accepts(1));
        assert_eq!(query.target.path, PathBuf::from("/repo/b.rs"));
    }

    #[test]
    fn file_diff_query_rejects_previous_document_and_refresh() {
        bevy::tasks::IoTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let file = super::super::status::FileGit::new("/repo/a.rs", 7);
        let mut query = GitDiffQuery {
            target: GitDiffTarget {
                repo_root: "/repo".into(),
                path: "/repo/a.rs".into(),
                path_bytes: Vec::new(),
                reference: String::new(),
            },
            generation: 3,
            file: Some(FileDiffIdentity {
                document: 7,
                refresh: 0,
            }),
            page_revision: None,
        };

        assert!(query.accepts_file(3, &file));

        query.file.as_mut().unwrap().refresh = 1;
        assert!(!query.accepts_file(3, &file));

        let replacement = super::super::status::FileGit::new("/repo/a.rs", 8);
        assert!(!query.accepts_file(3, &replacement));
    }
}
