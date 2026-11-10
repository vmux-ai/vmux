use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_cef::prelude::{UiEventPlugin, UiInput};
use vmux_api::command_bar::PromptRequest;

use crate::event::{
    GitReviewNoteAddRequest, GitReviewNoteDeleteRequest, GitReviewNoteEditRequest,
    GitReviewNoteSyncRequest, GitReviewSubmitRequest,
};
use crate::state::{
    GitPageControllerState, GitReviewDraft, GitReviewLocation, GitReviewNote, GitReviewNoteReceipt,
    GitReviewNoteRemoved, GitReviewPrompt, GitReviewTarget,
};

use super::state::GitState;

pub(super) struct ReviewPlugin;

#[derive(Component, Default)]
pub(super) struct GitReviewDeltas(Vec<GitReviewDelta>);

pub(super) enum GitReviewDelta {
    Upsert(Box<GitReviewNote>),
    Removed(GitReviewNoteRemoved),
}

impl GitReviewDeltas {
    fn upsert(&mut self, note: &GitReviewNote) {
        self.0.push(GitReviewDelta::Upsert(Box::new(note.clone())));
    }

    fn removed(&mut self, id: vmux_api::Id<crate::state::GitReviewNoteIdentity>) {
        self.0
            .push(GitReviewDelta::Removed(GitReviewNoteRemoved { id }));
    }

    pub(super) fn drain(&mut self) -> impl Iterator<Item = GitReviewDelta> + '_ {
        self.0.drain(..)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
}

impl Plugin for ReviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SubmissionGuard>()
            .init_resource::<GitReviewProviderRegistry>()
            .add_message::<GitReviewSubmission>()
            .add_plugins(UiEventPlugin::<(
                GitReviewNoteAddRequest,
                GitReviewNoteEditRequest,
                GitReviewNoteDeleteRequest,
                GitReviewNoteSyncRequest,
                GitReviewSubmitRequest,
            )>::default())
            .add_message::<GitReviewPublished>()
            .add_message::<GitReviewSubmissionFinished>()
            .add_observer(add_note)
            .add_observer(edit_note)
            .add_observer(delete_note)
            .add_observer(sync_note)
            .add_observer(submit)
            .add_systems(Update, (acknowledge, finish_submission))
            .add_systems(
                Update,
                reanchor
                    .after(super::GitUpdateSet::Jobs)
                    .before(super::controller::ControllerSet::Reconcile),
            );
    }
}

#[derive(Resource, Default)]
pub struct GitReviewProviderRegistry {
    providers: HashMap<String, Vec<GitReviewProviderCapability>>,
}

impl GitReviewProviderRegistry {
    pub fn replace(
        &mut self,
        provider: impl Into<String>,
        mut capabilities: Vec<GitReviewProviderCapability>,
    ) -> bool {
        capabilities.sort();
        capabilities.dedup();
        let provider = provider.into();
        if self.providers.get(&provider) == Some(&capabilities) {
            return false;
        }
        self.providers.insert(provider, capabilities);
        true
    }

    pub fn contains(&self, provider: &str, target: &GitReviewTarget) -> bool {
        self.providers
            .get(provider)
            .is_some_and(|capabilities| capabilities.iter().any(|entry| entry.matches(target)))
    }

    pub fn names(&self, target: Option<&GitReviewTarget>) -> Vec<String> {
        let Some(target) = target else {
            return Vec::new();
        };
        let mut providers = self
            .providers
            .iter()
            .filter_map(|(provider, capabilities)| {
                capabilities
                    .iter()
                    .any(|entry| entry.matches(target))
                    .then_some(provider.clone())
            })
            .collect::<Vec<_>>();
        providers.sort();
        providers
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitReviewProviderCapability {
    pub repository: String,
    pub base_sha: String,
    pub head_sha: String,
}

impl GitReviewProviderCapability {
    fn matches(&self, target: &GitReviewTarget) -> bool {
        matches!(
            target,
            GitReviewTarget::Branch {
                repository,
                base_sha,
                head_sha,
                ..
            } if repository == &self.repository
                && base_sha == &self.base_sha
                && head_sha == &self.head_sha
        )
    }
}

#[derive(Clone, Debug)]
pub struct GitReviewSnapshot {
    pub target: GitReviewTarget,
    pub generation: u64,
    pub notes: Vec<GitReviewNote>,
}

impl GitReviewSnapshot {
    fn capture(draft: &GitReviewDraft, target: GitReviewTarget, generation: u64) -> Option<Self> {
        let notes = draft
            .for_target(&target)
            .filter(|note| note.is_promptable(&target, generation))
            .cloned()
            .collect::<Vec<_>>();
        if notes.is_empty() {
            return None;
        }
        Some(Self {
            target,
            generation,
            notes,
        })
    }

    pub fn prompt(&self, repository: &str) -> Option<String> {
        GitReviewPrompt::render(repository, &self.target, self.generation, &self.notes)
    }

    pub fn provider_notes(&self) -> impl Iterator<Item = &GitReviewNote> {
        let supported = self.target.supports_external_review();
        self.notes
            .iter()
            .filter(move |note| supported && note.is_publishable(&self.target, self.generation))
    }

    fn prompt_fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", self.target).hash(&mut hasher);
        self.generation.hash(&mut hasher);
        for note in &self.notes {
            note.id.as_str().hash(&mut hasher);
            note.revision.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn provider_fingerprint(&self) -> Option<u64> {
        let mut notes = self.provider_notes().peekable();
        notes.peek()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}", self.target).hash(&mut hasher);
        self.generation.hash(&mut hasher);
        for note in notes {
            note.id.as_str().hash(&mut hasher);
            note.revision.hash(&mut hasher);
            note.anchor.path.hash(&mut hasher);
            GitReviewLocationHash::of(note.anchor.location).hash(&mut hasher);
            note.body.hash(&mut hasher);
        }
        Some(hasher.finish())
    }
}

struct GitReviewLocationHash;

impl GitReviewLocationHash {
    fn of(location: GitReviewLocation) -> (u8, u32) {
        match location {
            GitReviewLocation::File => (0, 0),
            GitReviewLocation::Line {
                side: crate::state::GitReviewSide::Left,
                line,
            } => (1, line),
            GitReviewLocation::Line {
                side: crate::state::GitReviewSide::Right,
                line,
            } => (2, line),
        }
    }
}

#[derive(Message, Clone, Debug)]
pub struct GitReviewSubmission {
    pub source: Entity,
    pub repo_root: String,
    pub fingerprint: u64,
    pub snapshot: GitReviewSnapshot,
}

#[derive(Message, Clone, Debug)]
pub struct GitReviewPublished {
    pub source: Entity,
    pub notes: Arc<[GitReviewNoteReceipt]>,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct GitReviewSubmissionFinished {
    pub source: Entity,
    pub fingerprint: u64,
}

#[derive(Resource, Default)]
struct SubmissionGuard {
    prompts: HashMap<Entity, (u64, Instant)>,
    providers: HashMap<(Entity, u64), Instant>,
}

impl SubmissionGuard {
    const WINDOW: Duration = Duration::from_secs(2);
    const PROVIDER_WINDOW: Duration = Duration::from_secs(30);

    fn accept_prompt(&mut self, source: Entity, fingerprint: u64) -> bool {
        let now = Instant::now();
        self.prompts
            .retain(|_, (_, submitted)| now.duration_since(*submitted) < Self::WINDOW);
        if self
            .prompts
            .get(&source)
            .is_some_and(|(held, _)| *held == fingerprint)
        {
            return false;
        }
        self.prompts.insert(source, (fingerprint, now));
        true
    }

    fn accept_provider(&mut self, source: Entity, fingerprint: u64) -> bool {
        let now = Instant::now();
        self.providers
            .retain(|_, submitted| now.duration_since(*submitted) < Self::PROVIDER_WINDOW);
        self.providers.insert((source, fingerprint), now).is_none()
    }

    fn finish_provider(&mut self, source: Entity, fingerprint: u64) {
        self.providers.remove(&(source, fingerprint));
    }
}

fn add_note(
    trigger: On<UiInput<GitReviewNoteAddRequest>>,
    mut pages: Query<(
        &GitState,
        &GitPageControllerState,
        &mut GitReviewDraft,
        &mut GitReviewDeltas,
    )>,
) {
    let Ok((state, controller, mut review, mut deltas)) = pages.get_mut(trigger.event().webview)
    else {
        return;
    };
    if state.snapshot.diff_loading {
        return;
    }
    let Some(repository) = state.repository() else {
        return;
    };
    let Some(viewport) = state.snapshot.diff_viewport.as_ref() else {
        return;
    };
    let target = GitReviewTarget::current(repository, &state.snapshot, controller);
    if !review.prepare_for_add(&target) {
        return;
    }
    let fallback_path = if controller.focused_panel == crate::state::GitPanel::Files {
        controller.selected_path.as_str()
    } else {
        ""
    };
    let mut anchor = trigger.event().payload.anchor.clone();
    anchor.context.clear();
    if !anchor.normalize()
        || !anchor.is_present(&target, viewport.generation, fallback_path, &viewport.lines)
        || !anchor.capture_context(fallback_path, &viewport.lines)
    {
        return;
    }
    if review.notes.iter().any(|note| {
        note.anchor.target == anchor.target
            && note.anchor.path == anchor.path
            && note.anchor.location == anchor.location
            && note.anchor.context == anchor.context
            && note.anchor.generation == anchor.generation
    }) {
        return;
    }
    let note = GitReviewNote {
        id: vmux_api::Id::new(uuid::Uuid::new_v4().to_string()),
        anchor,
        body: String::new(),
        publish_to_provider: false,
        revision: 1,
    };
    deltas.upsert(&note);
    review.notes.push(note);
}

fn edit_note(
    trigger: On<UiInput<GitReviewNoteEditRequest>>,
    mut pages: Query<(&mut GitReviewDraft, &mut GitReviewDeltas)>,
) {
    let Ok((mut review, mut deltas)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    let Some(note) = review.notes.iter_mut().find(|note| note.id == request.id) else {
        return;
    };
    if note.update_body(request.body.clone()) {
        deltas.upsert(note);
    }
}

fn delete_note(
    trigger: On<UiInput<GitReviewNoteDeleteRequest>>,
    mut pages: Query<(&mut GitReviewDraft, &mut GitReviewDeltas)>,
) {
    let Ok((mut review, mut deltas)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let id = &trigger.event().payload.id;
    if !review.notes.iter().any(|note| note.id == *id) {
        return;
    }
    review.notes.retain(|note| note.id != *id);
    deltas.removed(id.clone());
}

fn sync_note(
    trigger: On<UiInput<GitReviewNoteSyncRequest>>,
    mut pages: Query<(&mut GitReviewDraft, &mut GitReviewDeltas)>,
) {
    let Ok((mut review, mut deltas)) = pages.get_mut(trigger.event().webview) else {
        return;
    };
    let request = &trigger.event().payload;
    let Some(note) = review.notes.iter_mut().find(|note| note.id == request.id) else {
        return;
    };
    if note.set_publish_to_provider(request.enabled) {
        deltas.upsert(note);
    }
}

fn submit(
    trigger: On<UiInput<GitReviewSubmitRequest>>,
    pages: Query<(&GitState, &GitPageControllerState, &GitReviewDraft)>,
    mut guard: ResMut<SubmissionGuard>,
    providers: Res<GitReviewProviderRegistry>,
    mut submissions: MessageWriter<GitReviewSubmission>,
    mut commands: Commands,
) {
    let source = trigger.event().webview;
    let Ok((state, controller, review)) = pages.get(source) else {
        return;
    };
    if state.snapshot.diff_loading {
        return;
    }
    let Some(repository) = state.repository() else {
        return;
    };
    let Some(viewport) = state.snapshot.diff_viewport.as_ref() else {
        return;
    };
    let target = GitReviewTarget::current(repository, &state.snapshot, controller);
    let Some(snapshot) = GitReviewSnapshot::capture(review, target, viewport.generation) else {
        return;
    };
    let provider_fingerprint = providers
        .contains("github", &snapshot.target)
        .then(|| snapshot.provider_fingerprint())
        .flatten();
    let accepted = match provider_fingerprint {
        Some(fingerprint) => guard.accept_provider(source, fingerprint),
        None => guard.accept_prompt(source, snapshot.prompt_fingerprint()),
    };
    if !accepted {
        return;
    }
    let Some(text) = snapshot.prompt(&repository.repo_name) else {
        if let Some(fingerprint) = provider_fingerprint {
            guard.finish_provider(source, fingerprint);
        }
        return;
    };
    commands.trigger(UiInput {
        webview: source,
        payload: PromptRequest {
            text,
            target_url: None,
            attachments: Vec::new(),
        },
    });
    let Some(fingerprint) = provider_fingerprint else {
        return;
    };
    submissions.write(GitReviewSubmission {
        source,
        repo_root: repository.repo_root.clone(),
        fingerprint,
        snapshot,
    });
}

fn finish_submission(
    mut finished: MessageReader<GitReviewSubmissionFinished>,
    mut guard: ResMut<SubmissionGuard>,
) {
    for submission in finished.read() {
        guard.finish_provider(submission.source, submission.fingerprint);
    }
}

fn acknowledge(
    mut published: MessageReader<GitReviewPublished>,
    mut pages: Query<(&mut GitReviewDraft, &mut GitReviewDeltas)>,
) {
    for publication in published.read() {
        let Ok((mut review, mut deltas)) = pages.get_mut(publication.source) else {
            continue;
        };
        for receipt in publication.notes.iter() {
            let Some(note) = review.notes.iter_mut().find(|note| note.id == receipt.id) else {
                continue;
            };
            if note.revision == receipt.revision && note.set_publish_to_provider(false) {
                deltas.upsert(note);
            }
        }
    }
}

type ReanchoredPages<'w, 's> = Query<
    'w,
    's,
    (
        &'static GitState,
        &'static GitPageControllerState,
        &'static mut GitReviewDraft,
    ),
    Or<(
        Changed<GitState>,
        Changed<super::state::GitDiffProjectionState>,
    )>,
>;

fn reanchor(mut pages: ReanchoredPages) {
    for (state, controller, mut review) in &mut pages {
        let Some(repository) = state.repository() else {
            if !review.notes.is_empty() {
                review.notes.clear();
            }
            continue;
        };
        let repository_changed = review
            .bypass_change_detection()
            .retain_repository(&repository.repo_root);
        let Some(viewport) = state.snapshot.diff_viewport.as_ref() else {
            if repository_changed {
                review.set_changed();
            }
            continue;
        };
        if state.snapshot.diff_loading || viewport.has_more {
            if repository_changed {
                review.set_changed();
            }
            continue;
        }
        let target = GitReviewTarget::current(repository, &state.snapshot, controller);
        let fallback_path = if controller.focused_panel == crate::state::GitPanel::Files {
            controller.selected_path.as_str()
        } else {
            ""
        };
        let reanchored = review.bypass_change_detection().reanchor(
            &target,
            viewport.generation,
            fallback_path,
            &viewport.lines,
        );
        if repository_changed || reanchored {
            review.set_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{DiffKind, DiffLine, GitDiffViewport, GitRepositorySnapshot, StyledSpan};
    use crate::state::{GitReviewAnchor, GitReviewSide};

    fn target() -> GitReviewTarget {
        GitReviewTarget::Branch {
            repository: "/tmp/repository".into(),
            base: "main".into(),
            base_revision: "refs/remotes/origin/main".into(),
            base_sha: "base-sha".into(),
            head: "feature".into(),
            head_sha: "head-sha".into(),
        }
    }

    fn state() -> GitState {
        GitState {
            snapshot: crate::state::GitPageSnapshot {
                repository: Some(GitRepositorySnapshot {
                    path: "/tmp/repository".into(),
                    repo_root: "/tmp/repository".into(),
                    repo_name: "repository".into(),
                    is_worktree: false,
                    branch: "feature".into(),
                    base_ref: "main".into(),
                    upstream: String::new(),
                    ahead: 0,
                    behind: 0,
                    files: Vec::new(),
                    commits: Vec::new(),
                    branches: Vec::new(),
                    remote_branches: Vec::new(),
                    tags: Vec::new(),
                    stashes: Vec::new(),
                }),
                diff_viewport: Some(GitDiffViewport {
                    generation: 4,
                    revision: 1,
                    first_line: 0,
                    total_lines: 2,
                    has_more: false,
                    lines: vec![
                        DiffLine {
                            kind: DiffKind::Hunk,
                            old_no: None,
                            new_no: None,
                            hunk: None,
                            spans: vec![StyledSpan {
                                text: "src/lib.rs".into(),
                                light_fg: [0; 3],
                                dark_fg: [0; 3],
                                bold: false,
                                italic: false,
                            }],
                        },
                        DiffLine {
                            kind: DiffKind::Add,
                            old_no: None,
                            new_no: Some(12),
                            hunk: Some(1),
                            spans: vec![StyledSpan {
                                text: "let value = 1;".into(),
                                light_fg: [0; 3],
                                dark_fg: [0; 3],
                                bold: false,
                                italic: false,
                            }],
                        },
                    ],
                    markers: Vec::new(),
                    error: String::new(),
                }),
                branch_files_base_ref: "main".into(),
                branch_files_base_revision: "refs/remotes/origin/main".into(),
                branch_files_base_sha: "base-sha".into(),
                branch_files_head_sha: "head-sha".into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn identical_provider_capabilities_do_not_replace_the_registry() {
        let capability = GitReviewProviderCapability {
            repository: "/tmp/repository".into(),
            base_sha: "base".into(),
            head_sha: "head".into(),
        };
        let mut registry = GitReviewProviderRegistry::default();

        assert!(registry.replace("github", vec![capability.clone()]));
        assert!(!registry.replace("github", vec![capability]));
    }

    #[test]
    fn review_notes_are_private_until_explicitly_enabled() {
        let mut app = App::new();
        app.add_plugins(ReviewPlugin);
        let page = app
            .world_mut()
            .spawn((
                state(),
                GitPageControllerState {
                    focused_panel: crate::state::GitPanel::Branches,
                    selected_branch: "feature".into(),
                    ..Default::default()
                },
                GitReviewDraft::default(),
            ))
            .id();
        app.world_mut().trigger(UiInput {
            webview: page,
            payload: GitReviewNoteAddRequest {
                anchor: GitReviewAnchor {
                    target: target(),
                    path: "src/lib.rs".into(),
                    location: GitReviewLocation::Line {
                        side: GitReviewSide::Right,
                        line: 12,
                    },
                    context: "let value = 1;".into(),
                    generation: 4,
                },
            },
        });
        app.update();

        let review = app.world().get::<GitReviewDraft>(page).unwrap();
        assert_eq!(review.notes.len(), 1);
        assert!(!review.notes[0].publish_to_provider);
    }

    #[test]
    fn publication_only_acknowledges_the_submitted_revision() {
        let mut app = App::new();
        app.add_plugins(ReviewPlugin);
        let page = app
            .world_mut()
            .spawn((
                GitReviewDraft {
                    notes: vec![GitReviewNote {
                        id: vmux_api::Id::new("note-1"),
                        anchor: GitReviewAnchor {
                            target: target(),
                            path: "src/lib.rs".into(),
                            location: GitReviewLocation::Line {
                                side: GitReviewSide::Right,
                                line: 12,
                            },
                            context: "let value = 1;".into(),
                            generation: 4,
                        },
                        body: "new body".into(),
                        publish_to_provider: true,
                        revision: 2,
                    }],
                },
                GitReviewDeltas::default(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<GitReviewPublished>>()
            .write(GitReviewPublished {
                source: page,
                notes: vec![GitReviewNoteReceipt {
                    id: vmux_api::Id::new("note-1"),
                    revision: 1,
                }]
                .into(),
            });
        app.update();

        assert!(app.world().get::<GitReviewDraft>(page).unwrap().notes[0].publish_to_provider);
    }

    #[test]
    fn provider_fingerprint_ignores_prompt_only_notes() {
        let provider = GitReviewNote {
            id: vmux_api::Id::new("provider"),
            anchor: GitReviewAnchor {
                target: target(),
                path: "src/lib.rs".into(),
                location: GitReviewLocation::Line {
                    side: GitReviewSide::Right,
                    line: 12,
                },
                context: "let value = 1;".into(),
                generation: 4,
            },
            body: "Publish".into(),
            publish_to_provider: true,
            revision: 1,
        };
        let mut prompt = provider.clone();
        prompt.id = vmux_api::Id::new("prompt");
        prompt.publish_to_provider = false;
        let first = GitReviewSnapshot::capture(
            &GitReviewDraft {
                notes: vec![provider.clone(), prompt.clone()],
            },
            target(),
            4,
        )
        .unwrap();
        prompt.update_body("Updated prompt".into());
        let second = GitReviewSnapshot::capture(
            &GitReviewDraft {
                notes: vec![provider, prompt],
            },
            target(),
            4,
        )
        .unwrap();

        assert_eq!(first.provider_fingerprint(), second.provider_fingerprint());
        assert_ne!(first.prompt_fingerprint(), second.prompt_fingerprint());
    }

    #[test]
    fn provider_submission_remains_guarded_until_completion() {
        let source = Entity::from_bits(7);
        let mut guard = SubmissionGuard::default();

        assert!(guard.accept_provider(source, 42));
        assert!(!guard.accept_provider(source, 42));
        guard.finish_provider(source, 42);
        assert!(guard.accept_provider(source, 42));
    }
}
