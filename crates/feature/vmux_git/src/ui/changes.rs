#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;
use crate::state::{GitOperationEligibility, GitPanel};

use super::model::FileStatusView;
use super::operation::HeaderOperationButton;
use super::shared::ListPager;

#[component]
pub(super) fn ChangesCard(
    repository: GitRepositorySnapshot,
    selected_path_bytes: Vec<u8>,
    confirm_discard: Vec<u8>,
    commit_message: String,
    commit_pending: String,
    operations: GitOperationEligibility,
) -> Element {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    for (index, entry) in repository.files.iter().enumerate() {
        if entry.staged {
            staged.push((index, entry.clone()));
        }
        if entry.unstaged {
            unstaged.push((index, entry.clone()));
        }
    }
    let staged_count = staged.len() as u32;
    let staged_total = staged.len();
    let unstaged_total = unstaged.len();
    let total_rows = staged_total + unstaged_total;
    let pager = ListPager::use_state();
    let selected_row = staged
        .iter()
        .position(|(_, entry)| entry.path_bytes == selected_path_bytes)
        .or_else(|| {
            unstaged
                .iter()
                .position(|(_, entry)| entry.path_bytes == selected_path_bytes)
                .map(|index| staged_total + index)
        });
    let visible_rows = pager
        .visible(total_rows)
        .max(selected_row.map(|index| index + 1).unwrap_or_default());
    let visible_staged = visible_rows.min(staged_total);
    let visible_unstaged = visible_rows
        .saturating_sub(staged_total)
        .min(unstaged_total);
    let has_unstaged = !unstaged.is_empty();
    let file_operations = rsx! {
        div { class: "flex shrink-0 items-center gap-0.5",
            if has_unstaged {
                HeaderOperationButton {
                    icon: LineIcon::Plus,
                    shortcut: "",
                    label: translate("git-stage-all"),
                    disabled: !operations.stage_all,
                    danger: false,
                    onpress: {
                        let repo_root = repository.repo_root.clone();
                        move |_| {
                            send(&GitStageAllRequest { path: repo_root.clone() });
                        }
                    },
                }
            }
            HeaderOperationButton {
                icon: LineIcon::Package,
                shortcut: "s",
                label: translate("git-stash"),
                disabled: !operations.stash,
                danger: false,
                onpress: {
                    let repo_root = repository.repo_root.clone();
                    move |_| GitOperation::StashPush.send(repo_root.clone())
                },
            }
            HeaderOperationButton {
                icon: LineIcon::Pencil,
                shortcut: "A",
                label: translate("git-amend"),
                disabled: !operations.amend,
                danger: false,
                onpress: {
                    let repo_root = repository.repo_root.clone();
                    move |_| GitOperation::Amend.send(repo_root.clone())
                },
            }
        }
    };

    rsx! {
        section { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10 shrink-0 items-center justify-end px-3",
                {file_operations}
            }
            div {
                class: "min-h-0 flex-1 overflow-y-auto px-2 py-1.5",
                onscroll: move |event| pager.load_more(event, total_rows),
                if repository.files.is_empty() {
                    div { class: "flex h-full min-h-16 items-center justify-center gap-2 px-3 text-center text-xs text-muted-foreground",
                        LineIconView { icon: LineIcon::ShieldCheck, class: "h-4 w-4 shrink-0 text-ansi-2" }
                        div { class: "truncate font-medium text-foreground", {translate("git-repository-clean")} }
                    }
                } else {
                    if !staged.is_empty() {
                        FileSection {
                            title: translate("git-staged-changes"),
                            files: staged.into_iter().take(visible_staged).collect(),
                            total: staged_total,
                            repo_root: repository.repo_root.clone(),
                            staged_view: true,
                            selected_path_bytes: selected_path_bytes.clone(),
                            confirm_discard: confirm_discard.clone(),
                        }
                    }
                    if !unstaged.is_empty() {
                        FileSection {
                            title: translate("git-unstaged-changes"),
                            files: unstaged.into_iter().take(visible_unstaged).collect(),
                            total: unstaged_total,
                            repo_root: repository.repo_root.clone(),
                            staged_view: false,
                            selected_path_bytes: selected_path_bytes.clone(),
                            confirm_discard: confirm_discard.clone(),
                        }
                    }
                }
            }
            CommitPanel {
                staged_count,
                commit_message,
                commit_pending,
                can_commit: operations.commit,
            }
        }
    }
}

#[component]
fn FileSection(
    title: String,
    files: Vec<(usize, GitFileEntry)>,
    total: usize,
    repo_root: String,
    staged_view: bool,
    selected_path_bytes: Vec<u8>,
    confirm_discard: Vec<u8>,
) -> Element {
    rsx! {
        div { class: "pb-1 last:pb-0",
            div { class: "flex h-8 items-center justify-between px-2.5 text-[10px] font-semibold text-muted-foreground",
                span { "{title}" }
                span { class: "tabular-nums", "{total}" }
            }
            div {
                for (row_index, entry) in files {
                    FileRow {
                        key: "{staged_view}-{entry.path_bytes:?}",
                        row_index,
                        entry,
                        repo_root: repo_root.clone(),
                        staged_view,
                        selected_path_bytes: selected_path_bytes.clone(),
                        confirm_discard: confirm_discard.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn FileRow(
    row_index: usize,
    entry: GitFileEntry,
    repo_root: String,
    staged_view: bool,
    selected_path_bytes: Vec<u8>,
    confirm_discard: Vec<u8>,
) -> Element {
    let absolute = entry.absolute_path(&repo_root);
    let selected = selected_path_bytes == entry.path_bytes;
    let file_path_bytes = entry.path_bytes.clone();
    let file_name = entry.name().to_string();
    let parent = entry.parent().to_string();
    let status_label = entry.status.label();
    let status_code = entry.status.code();
    let status_class = entry.status.class();
    let can_discard = entry.can_discard() && !staged_view;
    let confirming = confirm_discard == entry.path_bytes;
    let section = if staged_view { "staged" } else { "unstaged" };
    let row_id = format!("git-file-{section}-row-{row_index}");

    rsx! {
        div {
            id: "{row_id}",
            class: if selected {
                "group flex min-h-9 cursor-default items-center gap-2 rounded-md bg-primary/[0.10] px-2.5 text-foreground"
            } else {
                "group flex min-h-9 cursor-default items-center gap-2 rounded-md px-2.5 hover:bg-foreground/[0.045]"
            },
            onclick: {
                let file_path_bytes = file_path_bytes.clone();
                move |_| {
                    send(&GitFileSelectRequest {
                        path_bytes: file_path_bytes.clone(),
                    });
                }
            },
            span { class: "w-4 shrink-0 text-center font-mono text-xs font-semibold {status_class}", title: "{status_label}", "{status_code}" }
            TypeIcon { path: entry.path.clone(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-80" }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-[13px]", "{file_name}" }
                if !parent.is_empty() {
                    div { class: "truncate text-[10px] text-muted-foreground", "{parent}" }
                }
            }
            div { class: "flex shrink-0 items-center gap-0.5 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100",
                Button {
                    variant: ButtonVariant::Ghost,
                    class: "h-7 w-7 p-0 text-muted-foreground hover:bg-foreground/[0.08] hover:text-foreground",
                    title: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    aria_label: if staged_view { translate("git-unstage") } else { translate("git-stage") },
                    onclick: {
                        let absolute = absolute.clone();
                        let path_bytes = file_path_bytes.clone();
                        let repo_root = repo_root.clone();
                        move |event: Event<MouseData>| {
                            event.stop_propagation();
                            send(&GitPanelSelectRequest { panel: GitPanel::Files });
                            if staged_view {
                                send(&GitUnstageRequest { repo_root: repo_root.clone(), path: absolute.clone(), path_bytes: path_bytes.clone() });
                            } else {
                                send(&GitStageRequest { repo_root: repo_root.clone(), path: absolute.clone(), path_bytes: path_bytes.clone() });
                            }
                        }
                    },
                    LineIconView { icon: if staged_view { LineIcon::Minus } else { LineIcon::Plus }, class: "h-3.5 w-3.5" }
                }
                if can_discard {
                    Button {
                        variant: ButtonVariant::Ghost,
                        class: if confirming {
                            "h-7 w-7 bg-ansi-1/15 p-0 text-ansi-1 hover:bg-ansi-1/20 hover:text-ansi-1"
                        } else {
                            "h-7 w-7 p-0 text-muted-foreground hover:bg-ansi-1/10 hover:text-ansi-1"
                        },
                        title: if confirming { translate("git-confirm-discard") } else { translate("git-discard") },
                        aria_label: if confirming { translate("git-confirm-discard") } else { translate("git-discard") },
                        onclick: {
                            let path_bytes = file_path_bytes.clone();
                            move |event: Event<MouseData>| {
                                event.stop_propagation();
                                send(&GitDiscardFileRequest {
                                    path_bytes: path_bytes.clone(),
                                });
                            }
                        },
                        LineIconView { icon: if confirming { LineIcon::AlertCircle } else { LineIcon::RotateCcw }, class: "h-3.5 w-3.5" }
                    }
                }
            }
        }
    }
}

#[component]
fn CommitPanel(
    staged_count: u32,
    commit_message: String,
    commit_pending: String,
    can_commit: bool,
) -> Element {
    let can_commit = can_commit && !commit_message.trim().is_empty() && commit_pending.is_empty();

    rsx! {
        div { class: "shrink-0 bg-card/70 p-3",
            Textarea {
                variant: TextareaVariant::Outline,
                class: "min-h-8 w-full resize-none rounded-md border-0 bg-foreground/[0.035] px-2 py-1.5 text-[11px] shadow-none outline-none placeholder:text-muted-foreground focus:bg-foreground/[0.055] focus:ring-0",
                placeholder: translate("git-commit-message"),
                value: "{commit_message}",
                oninput: move |event: Event<FormData>| {
                    send(&GitCommitDraftRequest {
                        message: event.value(),
                    });
                },
                onkeydown: move |event: KeyboardEvent| event.stop_propagation(),
            }
            div { class: "mt-1.5 flex items-center gap-1",
                span { class: "truncate text-[9px] text-muted-foreground", {translate("git-staged-changes")} " · {staged_count}" }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "ml-auto h-6 shrink-0 rounded-md px-2 text-[10px] font-medium shadow-sm disabled:opacity-40",
                    disabled: !can_commit,
                    onclick: move |_| {
                        send(&GitCommitSubmitRequest);
                    },
                    {translate_with("git-commit", &[("count", TranslationValue::Number(staged_count as i64))])}
                }
            }
        }
    }
}
