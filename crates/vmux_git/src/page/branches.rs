#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::components::dialog::{DialogContent, DialogRoot, DialogTitle};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;

use super::model::{BranchCollection, BranchPrompt, GitPanel};
use super::panel::{HeaderActionButton, PanelHeader, PanelIcon};
use super::workspace::GitWorkspace;

#[component]
pub(super) fn BranchPromptDialog() -> Element {
    let state = use_context::<super::state::GitPageState>();
    let mut branch_prompt = state.branch_prompt;
    let mut draft = state.branch_draft;
    let Some(prompt) = branch_prompt() else {
        return rsx! {};
    };
    let title = match &prompt {
        BranchPrompt::Create { .. } => translate("git-new-branch"),
        BranchPrompt::Delete { .. } => translate("git-delete-branch"),
    };
    let is_create = matches!(prompt, BranchPrompt::Create { .. });
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    branch_prompt.set(None);
                }
            },
            attributes: vec![],
            DialogContent { class: "max-w-[360px] p-4", attributes: vec![],
                DialogTitle { attributes: vec![], "{title}" }
                if is_create {
                    input {
                        class: "w-full rounded-md border border-border bg-foreground/[0.04] px-3 py-2 text-sm text-foreground outline-none transition-colors focus:border-primary/50",
                        autofocus: true,
                        value: "{draft}",
                        placeholder: translate("git-new-branch"),
                        oninput: move |event: Event<FormData>| draft.set(event.value()),
                        onkeydown: {
                            let prompt = prompt.clone();
                            move |event: KeyboardEvent| {
                                event.stop_propagation();
                                if event.key() == Key::Enter
                                    && state.submit_branch_prompt(&prompt)
                                {
                                    event.prevent_default();
                                    branch_prompt.set(None);
                                }
                            }
                        },
                    }
                } else if let BranchPrompt::Delete { branch } = &prompt {
                    code { class: "rounded-md bg-foreground/[0.05] px-2 py-1 text-sm text-foreground", "{branch}" }
                }
                div { class: "flex justify-end gap-2",
                    Button {
                        size: ButtonSize::Xs,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| branch_prompt.set(None),
                        {translate("common-cancel")}
                    }
                    Button {
                        size: ButtonSize::Xs,
                        variant: if is_create { ButtonVariant::Primary } else { ButtonVariant::Destructive },
                        disabled: is_create && draft().trim().is_empty(),
                        onclick: {
                            let prompt = prompt.clone();
                            move |_| {
                                if state.submit_branch_prompt(&prompt) {
                                    branch_prompt.set(None);
                                }
                            }
                        },
                        {if is_create { translate("common-save") } else { translate("common-delete") }}
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn BranchLogCard(branch: String, branch_log: Option<GitBranchLogEvent>) -> Element {
    let log = branch_log.filter(|event| event.branch == branch);
    let commit_count = log.as_ref().map(|event| event.commits.len()).unwrap_or(0);

    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[28rem] border-t-violet-400/30 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-violet-400/[0.065] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-violet-400/10 text-violet-400 ring-1 ring-inset ring-violet-400/15",
                    LineIconView { icon: LineIcon::GitCommit, class: "h-3 w-3" }
                }
                span { class: "font-mono text-[9px] font-semibold text-muted-foreground", "[0]" }
                span { class: "text-[11px] font-semibold tracking-[-0.01em]", {translate("git-log")} }
                span { class: "min-w-0 truncate rounded-full border border-violet-400/20 bg-violet-400/[0.07] px-2 py-0.5 font-mono text-[10px] text-violet-300", "{branch}" }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-3 py-2 sm:px-4",
                if let Some(log) = log {
                    if log.commits.is_empty() {
                        div { class: "flex h-full min-h-64 items-center justify-center text-sm text-muted-foreground", {translate("git-no-commits")} }
                    } else {
                        for (index, commit) in log.commits.into_iter().enumerate() {
                            article { key: "{commit.sha}", class: "relative grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2.5 py-3 first:pt-1",
                                div { class: "relative flex justify-center",
                                    if index > 0 {
                                        span { class: "absolute bottom-1/2 top-0 w-px bg-violet-400/35" }
                                    }
                                    if index + 1 < commit_count {
                                        span { class: "absolute bottom-0 top-1/2 w-px bg-violet-400/35" }
                                    }
                                    span { class: if index == 0 {
                                            "relative mt-1.5 size-2.5 rounded-full border-2 border-violet-300 bg-violet-400 shadow-[0_0_0_3px_color-mix(in_oklab,var(--card)_92%,transparent)]"
                                        } else {
                                            "relative mt-1.5 size-2 rounded-full border border-violet-400 bg-card shadow-[0_0_0_3px_color-mix(in_oklab,var(--card)_92%,transparent)]"
                                        } }
                                }
                                div { class: "min-w-0 border-b border-foreground/[0.06] pb-3",
                                    div { class: "flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1",
                                        code { class: "shrink-0 font-mono text-[11px] font-semibold text-amber-400", "{commit.short_sha}" }
                                        if !commit.references.is_empty() {
                                            span { class: "min-w-0 truncate rounded-full border border-ansi-2/20 bg-ansi-2/[0.07] px-1.5 py-0.5 font-mono text-[9px] text-ansi-2", "{commit.references}" }
                                        }
                                        span { class: "ml-auto shrink-0 text-[10px] text-muted-foreground", "{commit.date}" }
                                    }
                                    div { class: "mt-1.5 text-sm font-medium leading-5 text-foreground", "{commit.summary}" }
                                    div { class: "mt-1 text-[10px] text-muted-foreground", "{commit.author}" }
                                    if !commit.body.is_empty() {
                                        pre { class: "mt-3 whitespace-pre-wrap break-words border-l border-ansi-1/45 pl-3 font-sans text-xs leading-5 text-foreground/80", "{commit.body}" }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "flex h-full min-h-64 flex-col gap-4 p-2",
                        for width in ["w-10/12", "w-7/12", "w-full", "w-9/12", "w-11/12", "w-6/12"] {
                            Skeleton { class: "h-4 {width} bg-foreground/[0.045]" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn BranchesCard(
    repository: GitRepositoryEvent,
    selected_branch: Signal<String>,
    branch_collection: Signal<BranchCollection>,
    branch_prompt: Signal<Option<BranchPrompt>>,
    branch_draft: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let collection = branch_collection();
    let reference_count = collection.references(&repository).len();
    let selected = if collection == BranchCollection::Local {
        repository
            .branches
            .iter()
            .find(|entry| entry.name == selected_branch())
            .cloned()
    } else {
        None
    };
    let branch_actions = selected.map(|branch| {
        let current = branch.current;
        rsx! {
            div { class: "flex shrink-0 items-center gap-0.5",
                HeaderActionButton {
                    icon: LineIcon::Check,
                    shortcut: "space",
                    label: translate("git-checkout"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.clone();
                        move |_| GitWorkspace::select_branch(&repo_root, &branch)
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::Plus,
                    shortcut: "n",
                    label: translate("git-new-branch"),
                    disabled: false,
                    danger: false,
                    onpress: {
                        let base = branch.name.clone();
                        move |_| {
                            branch_draft.set(String::new());
                            branch_prompt.set(Some(BranchPrompt::Create { base: base.clone() }));
                        }
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::Trash,
                    shortcut: "d",
                    label: translate("git-delete-branch"),
                    disabled: current || !branch.checkout.is_empty(),
                    danger: true,
                    onpress: {
                        let branch = branch.name.clone();
                        move |_| branch_prompt.set(Some(BranchPrompt::Delete { branch: branch.clone() }))
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::GitPullRequest,
                    shortcut: "r",
                    label: translate("git-rebase"),
                    disabled: current,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::Rebase { branch: branch.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::GitMerge,
                    shortcut: "M",
                    label: translate("git-merge"),
                    disabled: current,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::Merge { branch: branch.clone() })
                    },
                }
                HeaderActionButton {
                    icon: LineIcon::FastForward,
                    shortcut: "f",
                    label: translate("git-fast-forward"),
                    disabled: current,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        let branch = branch.name.clone();
                        move |_| GitWorkspace::operate(&repo_root, GitOperation::FastForward { branch: branch.clone() })
                    },
                }
            }
        }
    });

    rsx! {
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel() == GitPanel::Branches {
                "order-5 min-h-44 border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-3 sm:min-h-0 sm:order-none"
            } else {
                "order-5 min-h-44 border-t-violet-400/30 sm:col-start-1 sm:row-start-3 sm:min-h-0 sm:order-none"
            },
            onclick: move |_| focused_panel.set(GitPanel::Branches),
            PanelHeader {
                index: 3,
                title: translate("git-branches"),
                count: Some(reference_count),
                icon: PanelIcon::Line(LineIcon::GitBranch),
                icon_class: "bg-violet-400/10 text-violet-400 ring-1 ring-inset ring-violet-400/15",
                badge_class: "border-violet-400/20 bg-violet-400/[0.08] text-violet-400",
                focused: focused_panel() == GitPanel::Branches,
                actions: rsx! {
                    div { class: "flex shrink-0 items-center gap-1",
                        BranchCollectionTabs {
                            repository: repository.clone(),
                            selected_branch,
                            branch_collection,
                        }
                        {branch_actions}
                    }
                },
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-0.5",
                match collection {
                    BranchCollection::Local => rsx! {
                        for (index, branch) in repository.branches.into_iter().enumerate() {
                            BranchRow {
                                key: "local-{branch.name}",
                                branch,
                                index,
                                remote: false,
                                selected_branch,
                                focused_panel,
                            }
                        }
                    },
                    BranchCollection::Remote => rsx! {
                        for (index, branch) in repository.remote_branches.into_iter().enumerate() {
                            BranchRow {
                                key: "remote-{branch.name}",
                                branch,
                                index,
                                remote: true,
                                selected_branch,
                                focused_panel,
                            }
                        }
                    },
                    BranchCollection::Tags => rsx! {
                        for (index, tag) in repository.tags.into_iter().enumerate() {
                            TagRow {
                                key: "tag-{tag.name}",
                                tag,
                                index,
                                selected_branch,
                                focused_panel,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BranchCollectionTabs(
    repository: GitRepositoryEvent,
    selected_branch: Signal<String>,
    branch_collection: Signal<BranchCollection>,
) -> Element {
    rsx! {
        div { class: "flex h-5 shrink-0 items-center rounded-md bg-background/50 p-0.5 ring-1 ring-inset ring-foreground/10",
            for (collection, label) in [
                (BranchCollection::Local, translate("git-local")),
                (BranchCollection::Remote, translate("git-remote")),
                (BranchCollection::Tags, translate("git-tags")),
            ] {
                button {
                    r#type: "button",
                    class: if branch_collection() == collection {
                        "h-4 rounded px-1.5 text-[8px] font-semibold text-foreground shadow-sm bg-foreground/[0.10]"
                    } else {
                        "h-4 rounded px-1.5 text-[8px] font-medium text-muted-foreground hover:text-foreground"
                    },
                    onclick: {
                        let repository = repository.clone();
                        move |event: MouseEvent| {
                            event.stop_propagation();
                            branch_collection.set(collection);
                            selected_branch.set(collection.selected_reference(&repository, ""));
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}

#[component]
fn BranchRow(
    branch: GitBranchEntry,
    index: usize,
    remote: bool,
    selected_branch: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let name = branch.name.clone();
    rsx! {
        button {
            id: "git-branch-row-{index}",
            r#type: "button",
            title: if branch.checkout.is_empty() { branch.upstream.clone() } else { branch.checkout.clone() },
            class: if selected_branch() == branch.name {
                "flex h-5 w-full items-center justify-start gap-1 rounded px-1.5 text-left text-foreground bg-primary/[0.10] shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
            } else {
                "flex h-5 w-full items-center justify-start gap-1 rounded px-1.5 text-left text-foreground hover:bg-foreground/[0.045]"
            },
            onclick: move |_| {
                focused_panel.set(GitPanel::Branches);
                selected_branch.set(name.clone());
            },
            BranchMarker {
                current: branch.current,
                upstream: branch.upstream.clone(),
                worktree: !branch.checkout.is_empty(),
                remote,
            }
            div { class: "flex min-w-0 flex-1 items-center gap-1.5",
                span { class: "min-w-0 flex-1 truncate text-[10px] font-medium", "{branch.name}" }
                if branch.ahead > 0 {
                    span { class: "shrink-0 rounded-full bg-sky-400/10 px-1.5 font-mono text-[8px] text-sky-400", "↑{branch.ahead}" }
                }
                if branch.behind > 0 {
                    span { class: "shrink-0 rounded-full bg-violet-400/10 px-1.5 font-mono text-[8px] text-violet-400", "↓{branch.behind}" }
                }
                if branch.current {
                    span { class: "shrink-0 rounded-full bg-ansi-2/10 px-1.5 text-[8px] font-medium text-ansi-2", {translate("common-current")} }
                } else if !branch.upstream.is_empty() {
                    span { class: "max-w-[42%] shrink truncate font-mono text-[8px] text-muted-foreground", "{branch.upstream}" }
                }
                if !branch.short_sha.is_empty() {
                    code { class: "shrink-0 font-mono text-[8px] text-muted-foreground/60", "{branch.short_sha}" }
                }
            }
        }
    }
}

#[component]
fn TagRow(
    tag: GitTagEntry,
    index: usize,
    selected_branch: Signal<String>,
    focused_panel: Signal<GitPanel>,
) -> Element {
    let name = tag.name.clone();
    rsx! {
        button {
            id: "git-branch-row-{index}",
            r#type: "button",
            title: "{tag.message}",
            class: if selected_branch() == tag.name {
                "flex h-5 w-full items-center justify-start gap-1.5 rounded px-1.5 text-left text-foreground bg-primary/[0.10] shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
            } else {
                "flex h-5 w-full items-center justify-start gap-1.5 rounded px-1.5 text-left text-foreground hover:bg-foreground/[0.045]"
            },
            onclick: move |_| {
                focused_panel.set(GitPanel::Branches);
                selected_branch.set(name.clone());
            },
            LineIconView { icon: LineIcon::Tag, class: "size-3 shrink-0 text-violet-400/75" }
            span { class: "min-w-0 flex-1 truncate text-[10px] font-medium", "{tag.name}" }
            code { class: "shrink-0 font-mono text-[8px] text-muted-foreground", "{tag.short_sha}" }
            span { class: "shrink-0 text-[8px] text-muted-foreground", "{tag.date}" }
        }
    }
}

#[component]
fn BranchMarker(current: bool, upstream: String, worktree: bool, remote: bool) -> Element {
    let icon = if worktree {
        LineIcon::GitFork
    } else {
        LineIcon::GitBranch
    };
    let tone = if current {
        "bg-ansi-2/12 text-ansi-2 ring-ansi-2/20"
    } else if worktree {
        "bg-amber-400/10 text-amber-400 ring-amber-400/20"
    } else if remote || !upstream.is_empty() {
        "bg-sky-400/10 text-sky-400 ring-sky-400/20"
    } else {
        "bg-violet-400/10 text-violet-400 ring-violet-400/20"
    };
    let title = if current {
        translate("common-current")
    } else if worktree {
        translate("layout-worktree")
    } else if remote {
        translate("git-remote")
    } else if !upstream.is_empty() {
        upstream
    } else {
        translate("git-local")
    };
    rsx! {
        span {
            class: "flex size-4 shrink-0 items-center justify-center rounded-md ring-1 ring-inset {tone}",
            title,
            LineIconView { icon, class: "size-2.5" }
        }
    }
}
