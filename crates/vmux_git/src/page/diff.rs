#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_ui::components::card::{Card, CardVariant};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use crate::event::{GitDiffViewportEvent, GitRepositoryEvent};
use crate::ui::DiffView;
use crate::view::EditorDiffMarker;

#[component]
pub(super) fn CommitDiffCard(
    repository: GitRepositoryEvent,
    repo_root: Signal<String>,
    selected_commit: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
    diff_viewport: Signal<Option<GitDiffViewportEvent>>,
) -> Element {
    let empty_path = use_signal(String::new);
    let selected = repository
        .commits
        .iter()
        .find(|commit| commit.sha == selected_commit())
        .cloned();
    let title = selected
        .as_ref()
        .map(|commit| format!("{}  {}", commit.short_sha, commit.summary))
        .unwrap_or_else(|| translate("git-no-commits"));
    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[28rem] border-t-sky-400/25 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-sky-400/[0.055] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-sky-400/10 text-sky-400 ring-1 ring-inset ring-sky-400/15",
                    LineIconView { icon: LineIcon::GitCommit, class: "size-3" }
                }
                span { class: "min-w-0 flex-1 truncate text-[11px] font-medium", "{title}" }
                span { class: "rounded-full border border-sky-400/20 bg-sky-400/[0.08] px-2 py-0.5 text-[9px] font-semibold uppercase tracking-[0.08em] text-sky-400", "Diff" }
            }
            if selected.is_none() {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground",
                    {translate("git-no-commits")}
                }
            } else {
                DiffView {
                    repo_root,
                    path: empty_path,
                    reference: selected_commit(),
                    nonce,
                    viewport: diff_viewport,
                    visible: true,
                    markers,
                }
            }
        }
    }
}

#[component]
pub(super) fn DiffCard(
    repo_root: Signal<String>,
    selected_path: Signal<String>,
    selected_path_bytes: Signal<Vec<u8>>,
    selected_abs_path: Signal<String>,
    nonce: Signal<u32>,
    markers: Signal<HashMap<u32, EditorDiffMarker>>,
    diff_viewport: Signal<Option<GitDiffViewportEvent>>,
) -> Element {
    rsx! {
        Card { variant: CardVariant::Panel, class: "order-2 min-h-[28rem] border-t-emerald-400/25 sm:col-start-2 sm:row-start-1 sm:row-span-4 sm:min-h-0 sm:order-none",
            div { class: "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-emerald-400/[0.055] to-transparent px-2",
                div { class: "flex size-5 shrink-0 items-center justify-center rounded-md bg-emerald-400/10 text-emerald-400 ring-1 ring-inset ring-emerald-400/15",
                    TypeIcon { path: selected_path(), is_dir: false, class: "h-3 w-3" }
                }
                span { class: "min-w-0 flex-1 truncate font-mono text-[11px] font-medium",
                    if selected_path().is_empty() { {translate("git-select-file")} } else { "{selected_path}" }
                }
                span { class: "rounded-full border border-emerald-400/20 bg-emerald-400/[0.08] px-2 py-0.5 text-[9px] font-semibold uppercase tracking-[0.08em] text-emerald-400", "Diff" }
            }
            if selected_abs_path().is_empty() {
                div { class: "flex min-h-64 flex-1 items-center justify-center p-6 text-center text-sm text-muted-foreground",
                    {translate("git-select-file")}
                }
            } else {
                DiffView { repo_root, path: selected_abs_path, path_bytes: selected_path_bytes(), nonce, viewport: diff_viewport, visible: true, markers }
            }
        }
    }
}
