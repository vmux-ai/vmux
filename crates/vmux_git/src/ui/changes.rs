#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::*;
use crate::state::{GitOperationEligibility, GitPanel};

use super::model::FileStatusView;
use super::panel::{HeaderOperationButton, PanelHeader, PanelIcon};
use super::workspace::GitWorkspace;

#[component]
pub(super) fn ChangesCard(
    repository: GitRepositorySnapshot,
    selected_path_bytes: Vec<u8>,
    confirm_discard: Vec<u8>,
    commit_message: Signal<String>,
    pending_commit_message: Signal<String>,
    focused_panel: GitPanel,
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
    let file_operations = rsx! {
        div { class: "flex shrink-0 items-center gap-0.5",
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
        Card {
            variant: CardVariant::Panel,
            class: if focused_panel == GitPanel::Files {
                if repository.files.is_empty() {
                    "order-4 min-h-[13rem] border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                } else {
                    "order-4 min-h-[19rem] border-ansi-2/55 shadow-[0_0_0_1px_color-mix(in_oklab,var(--ansi-2)_18%,transparent)] sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                }
            } else {
                if repository.files.is_empty() {
                    "order-4 min-h-[13rem] border-t-amber-400/30 sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                } else {
                    "order-4 min-h-[19rem] border-t-amber-400/30 sm:col-start-1 sm:row-start-2 sm:min-h-0 sm:order-none"
                }
            },
            onclick: move |_| {
                let _ = send(&GitPanelSelectRequest { panel: GitPanel::Files });
            },
            PanelHeader {
                index: 2,
                title: translate("git-files"),
                count: Some(repository.files.len()),
                icon: PanelIcon::Line(LineIcon::File),
                icon_class: "bg-amber-400/10 text-amber-500 ring-1 ring-inset ring-amber-400/15",
                badge_class: "border-amber-400/20 bg-amber-400/[0.08] text-amber-500",
                focused: focused_panel == GitPanel::Files,
                operations: file_operations,
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-1 py-1",
                if repository.files.is_empty() {
                    div { class: "flex h-full min-h-16 items-center justify-center gap-2 px-3 text-center text-xs text-muted-foreground",
                        LineIconView { icon: LineIcon::ShieldCheck, class: "h-4 w-4 shrink-0 text-ansi-2" }
                        div { class: "truncate font-medium text-foreground", {translate("git-repository-clean")} }
                    }
                } else {
                    if !staged.is_empty() {
                        FileSection {
                            title: translate("git-staged-changes"),
                            files: staged,
                            repo_root: repository.repo_root.clone(),
                            staged_view: true,
                            selected_path_bytes: selected_path_bytes.clone(),
                            confirm_discard: confirm_discard.clone(),
                        }
                    }
                    if !unstaged.is_empty() {
                        FileSection {
                            title: translate("git-unstaged-changes"),
                            files: unstaged,
                            repo_root: repository.repo_root.clone(),
                            staged_view: false,
                            selected_path_bytes: selected_path_bytes.clone(),
                            confirm_discard: confirm_discard.clone(),
                        }
                    }
                }
            }
            CommitPanel {
                repo_root: repository.repo_root,
                staged_count,
                commit_message,
                pending_commit_message,
                can_commit: operations.commit,
            }
        }
    }
}

#[component]
fn FileSection(
    title: String,
    files: Vec<(usize, GitFileEntry)>,
    repo_root: String,
    staged_view: bool,
    selected_path_bytes: Vec<u8>,
    confirm_discard: Vec<u8>,
) -> Element {
    rsx! {
        div { class: "pb-1 last:pb-0",
            div { class: "flex h-7 items-center justify-between px-2.5 text-[9px] font-semibold uppercase tracking-[0.08em] text-muted-foreground",
                span { "{title}" }
                span { class: "tabular-nums", "{files.len()}" }
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
    let absolute = GitWorkspace::absolute_path(&repo_root, &entry.path);
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
                "group mx-1 flex min-h-8 cursor-default items-center gap-2 rounded-md bg-primary/[0.10] px-2 text-foreground shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--primary)_16%,transparent)]"
            } else {
                "group mx-1 flex min-h-8 cursor-default items-center gap-2 rounded-md px-2 hover:bg-foreground/[0.045]"
            },
            onclick: {
                let file_path_bytes = file_path_bytes.clone();
                move |_| {
                    let _ = send(&GitFileSelectRequest {
                        path_bytes: file_path_bytes.clone(),
                    });
                }
            },
            span { class: "w-4 shrink-0 text-center font-mono text-xs font-semibold {status_class}", title: "{status_label}", "{status_code}" }
            TypeIcon { path: entry.path.clone(), is_dir: false, class: "h-4 w-4 shrink-0 opacity-80" }
            div { class: "min-w-0 flex-1",
                div { class: "truncate text-xs", "{file_name}" }
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
                            let _ = send(&GitPanelSelectRequest { panel: GitPanel::Files });
                            if staged_view {
                                let _ = send(&GitUnstageRequest { repo_root: repo_root.clone(), path: absolute.clone(), path_bytes: path_bytes.clone() });
                            } else {
                                let _ = send(&GitStageRequest { repo_root: repo_root.clone(), path: absolute.clone(), path_bytes: path_bytes.clone() });
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
                                let _ = send(&GitDiscardFileRequest {
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
    repo_root: String,
    staged_count: u32,
    commit_message: Signal<String>,
    pending_commit_message: Signal<String>,
    can_commit: bool,
) -> Element {
    let can_commit =
        can_commit && !commit_message().trim().is_empty() && pending_commit_message().is_empty();

    rsx! {
        div { class: "shrink-0 border-t border-foreground/[0.07] bg-foreground/[0.015] p-1.5",
            Textarea {
                variant: TextareaVariant::Outline,
                class: "min-h-8 w-full resize-none rounded-md border border-foreground/[0.09] bg-background/65 px-2 py-1.5 text-[11px] shadow-inner outline-none placeholder:text-muted-foreground focus:border-primary/40 focus:ring-2 focus:ring-primary/10",
                placeholder: translate("git-commit-message"),
                value: "{commit_message}",
                oninput: move |event: Event<FormData>| commit_message.set(event.value()),
                onkeydown: move |event: KeyboardEvent| event.stop_propagation(),
            }
            div { class: "mt-1.5 flex items-center gap-1",
                span { class: "truncate text-[9px] text-muted-foreground", {translate("git-staged-changes")} " · {staged_count}" }
                Button {
                    variant: ButtonVariant::Primary,
                    class: "ml-auto h-6 shrink-0 rounded-md px-2 text-[10px] font-medium shadow-sm disabled:opacity-40",
                    disabled: !can_commit,
                    onclick: move |_| {
                        let text = commit_message().trim().to_string();
                        if text.is_empty() {
                            return;
                        }
                        if send(&GitCommitRequest {
                            path: repo_root.clone(),
                            message: text.clone(),
                        })
                        .is_ok()
                        {
                            pending_commit_message.set(text);
                        }
                    },
                    {translate_with("git-commit", &[("count", TranslationValue::Number(staged_count as i64))])}
                }
            }
        }
    }
}
