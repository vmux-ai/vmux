#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::directory::{DirectoryNavigator, DirectoryNavigatorEvent, visible_directory_entries};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use super::state::GitPageState;
use super::workspace::GitWorkspace;

#[component]
pub(super) fn EmptyRepository() -> Element {
    let GitPageState {
        snapshot,
        directory_selected: mut selected,
        directory_preview_path: mut preview_path,
        directory_came_from: mut came_from,
        directory_show_hidden: mut show_hidden,
        ..
    } = use_context::<GitPageState>();
    let ui = snapshot();
    let loading = ui.loading;
    let workspace = ui.workspace;
    let message = ui.message;
    let directory = ui.directory;
    let Some(directory) = directory else {
        return rsx! {
            if loading {
                GitLoadingSkeleton {}
            } else {
                main { class: "flex min-h-0 flex-1 items-center justify-center p-8",
                    div { class: "text-center",
                        LineIconView { icon: LineIcon::GitBranch, class: "mx-auto h-7 w-7 text-muted-foreground" }
                        h2 { class: "mt-4 text-lg font-semibold", {translate("git-no-repository")} }
                        if !workspace.is_empty() {
                            div { class: "mt-2 break-all font-mono text-xs text-muted-foreground", "{workspace}" }
                        }
                    }
                }
            }
        };
    };
    let path = directory.path.clone();
    let entries = directory.entries.clone();
    let action_directory = directory.clone();
    let children = ui
        .directory_preview
        .filter(|preview| preview.path == preview_path())
        .map(|preview| preview.entries);

    rsx! {
        main { class: "flex min-h-0 flex-1 flex-col overflow-hidden bg-background bg-[radial-gradient(120%_80%_at_50%_-10%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_60%)] font-mono text-sm leading-normal",
            div { class: "relative z-20 flex h-7 shrink-0 items-center gap-1 overflow-hidden border-b border-foreground/[0.07] bg-background/40 px-4 font-sans text-ui text-muted-foreground",
                TypeIcon { path: path.clone(), is_dir: true, class: "h-3.5 w-3.5 shrink-0 opacity-80" }
                span { class: "truncate text-foreground/90", "{path}" }
                if loading {
                    LineIconView { icon: LineIcon::RefreshCw, class: "ml-auto h-3.5 w-3.5 shrink-0 animate-spin" }
                } else if !message.is_empty() {
                    span { class: "ml-auto truncate text-ansi-1", "{message}" }
                }
            }
            DirectoryNavigator {
                path,
                parent_entries: directory.parent_entries,
                entries,
                children,
                selected: selected(),
                thumbs: HashMap::new(),
                show_hidden: show_hidden(),
                preview: rsx! { div { class: "text-xs text-muted-foreground opacity-60", "" } },
                on_event: move |event| match event {
                    DirectoryNavigatorEvent::Select { index, entry } => {
                        selected.set(index);
                        preview_path.set(String::new());
                        if entry.is_dir {
                            preview_path.set(entry.path.clone());
                            GitWorkspace::browse(&entry.path, true);
                        }
                    }
                    DirectoryNavigatorEvent::Ascend { target } => {
                        if action_directory.parent_path.is_empty() {
                            return;
                        }
                        came_from.set(target);
                        GitWorkspace::browse(&action_directory.parent_path, false);
                    }
                    DirectoryNavigatorEvent::Descend { target } => {
                        let Some(entry) = action_directory.entries.get(selected()) else {
                            return;
                        };
                        if !entry.is_dir {
                            return;
                        }
                        came_from.set(target);
                        GitWorkspace::browse(&entry.path, false);
                    }
                    DirectoryNavigatorEvent::Open { entry } => {
                        if entry.is_dir {
                            came_from.set(String::new());
                            GitWorkspace::browse(&entry.path, false);
                        }
                    }
                    DirectoryNavigatorEvent::ToggleHidden => {
                        let next = !show_hidden();
                        show_hidden.set(next);
                        let entries = visible_directory_entries(&action_directory.entries, next);
                        let index = selected().min(entries.len().saturating_sub(1));
                        selected.set(index);
                        preview_path.set(String::new());
                        if let Some(entry) = entries.get(index).filter(|entry| entry.is_dir) {
                            preview_path.set(entry.path.clone());
                            GitWorkspace::browse(&entry.path, true);
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn GitLoadingSkeleton() -> Element {
    rsx! {
        main { class: "min-h-0 flex-1 overflow-hidden bg-[radial-gradient(120%_90%_at_50%_-20%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_55%)] p-3",
            div { class: "grid h-full grid-cols-1 gap-3 sm:grid-cols-[minmax(17rem,0.78fr)_minmax(0,1.72fr)] sm:grid-rows-3",
                div { class: "flex min-h-52 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:row-span-1",
                    Skeleton { class: "h-5 w-28 bg-foreground/[0.08]" }
                    Skeleton { class: "h-9 w-full bg-foreground/[0.05]" }
                    Skeleton { class: "h-9 w-4/5 bg-foreground/[0.04]" }
                    Skeleton { class: "mt-auto h-16 w-full bg-foreground/[0.04]" }
                }
                div { class: "flex min-h-44 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:row-start-2",
                    Skeleton { class: "h-5 w-24 bg-foreground/[0.08]" }
                    Skeleton { class: "h-8 w-full bg-foreground/[0.05]" }
                    Skeleton { class: "h-8 w-3/4 bg-foreground/[0.04]" }
                }
                div { class: "flex min-h-48 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:row-start-3",
                    Skeleton { class: "h-5 w-20 bg-foreground/[0.08]" }
                    for width in ["w-full", "w-11/12", "w-4/5", "w-10/12"] {
                        Skeleton { class: "h-7 {width} bg-foreground/[0.045]" }
                    }
                }
                div { class: "hidden min-h-0 flex-col gap-3 rounded-xl border border-foreground/[0.08] bg-card/70 p-4 sm:col-start-2 sm:row-start-1 sm:row-span-3 sm:flex",
                    Skeleton { class: "h-5 w-40 bg-foreground/[0.08]" }
                    for width in ["w-10/12", "w-full", "w-9/12", "w-11/12", "w-8/12", "w-full", "w-7/12"] {
                        Skeleton { class: "h-4 {width} bg-foreground/[0.04]" }
                    }
                }
            }
        }
    }
}
