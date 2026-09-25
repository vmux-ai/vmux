#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::directory::{DirectoryNavigator, DirectoryNavigatorEvent};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use super::state::GitPageState;
use crate::event::{
    GitDirectoryAscendRequest, GitDirectoryDescendRequest, GitDirectoryOpenRequest,
    GitDirectorySelectRequest, GitDirectoryToggleHiddenRequest,
};

#[component]
pub(super) fn EmptyRepository() -> Element {
    let GitPageState {
        snapshot,
        directory,
        ..
    } = use_context::<GitPageState>();
    let ui = snapshot();
    let loading = ui.loading;
    let workspace = ui.workspace;
    let message = ui.message;
    let directory = directory();
    if directory.path.is_empty() {
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
    }
    let selected = usize::try_from(directory.selected).unwrap_or_default();

    rsx! {
        main { class: "flex min-h-0 flex-1 flex-col overflow-hidden bg-background bg-[radial-gradient(120%_80%_at_50%_-10%,color-mix(in_oklab,var(--primary)_5%,transparent),transparent_60%)] font-mono text-sm leading-normal",
            div { class: "relative z-20 flex h-7 shrink-0 items-center gap-1 overflow-hidden border-b border-foreground/[0.07] bg-background/40 px-4 font-sans text-ui text-muted-foreground",
                TypeIcon { path: directory.path.clone(), is_dir: true, class: "h-3.5 w-3.5 shrink-0 opacity-80" }
                span { class: "truncate text-foreground/90", "{directory.path}" }
                if loading {
                    LineIconView { icon: LineIcon::RefreshCw, class: "ml-auto h-3.5 w-3.5 shrink-0 animate-spin" }
                } else if !message.is_empty() {
                    span { class: "ml-auto truncate text-ansi-1", "{message}" }
                }
            }
            DirectoryNavigator {
                path: directory.path,
                parent_entries: directory.parent_entries,
                entries: directory.entries,
                children: directory.children,
                selected,
                thumbs: HashMap::new(),
                preview: rsx! { div { class: "text-xs text-muted-foreground opacity-60", "" } },
                on_event: move |event| match event {
                    DirectoryNavigatorEvent::Select { index, .. } => {
                        let Ok(index) = u32::try_from(index) else {
                            return;
                        };
                        let _ = send(&GitDirectorySelectRequest { index });
                    }
                    DirectoryNavigatorEvent::Ascend { target } => {
                        let _ = send(&GitDirectoryAscendRequest { target });
                    }
                    DirectoryNavigatorEvent::Descend { target } => {
                        let _ = send(&GitDirectoryDescendRequest { target });
                    }
                    DirectoryNavigatorEvent::Open { entry } => {
                        if entry.is_dir {
                            let _ = send(&GitDirectoryOpenRequest { path: entry.path });
                        }
                    }
                    DirectoryNavigatorEvent::ToggleHidden => {
                        let _ = send(&GitDirectoryToggleHiddenRequest);
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
