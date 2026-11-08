use dioxus::prelude::*;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::dropdown::{Dropdown, DropdownPopup};
use vmux_ui::components::manager::ManagerSearchRow;
use vmux_ui::components::page::PageToolbarRow;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::tree_row::FileTreeSkeleton;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{BuiltinIconView, LineIcon, LineIconView};

use super::dashboard::Split;
use super::state::GitPageState;
use crate::event::{GitRepositoryPickerRequest, GitRepositoryRequest};

#[component]
pub(super) fn EmptyRepository() -> Element {
    let page = use_context::<GitPageState>();
    let snapshot = page.snapshot;
    let projects = page.projects;
    let ui = snapshot();
    let loading = ui.loading;
    let workspace = ui.workspace;
    let message = ui.message;
    let mut open = use_signal(|| false);
    if loading {
        return rsx! { GitLoadingSkeleton {} };
    }
    let rows = projects().rows;
    let current = rows.iter().find(|project| project.is_active).cloned();
    let current_label = current
        .as_ref()
        .map(|project| project.label.clone())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| translate("agent-project-select"));
    let current_path = current
        .as_ref()
        .map(|project| project.path.clone())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| workspace.clone());
    rsx! {
        main { class: "flex min-h-0 flex-1 items-center justify-center bg-background px-6 py-10 text-foreground",
            div { class: "flex w-full max-w-sm flex-col items-center text-center",
                LineIconView { icon: LineIcon::GitBranch, class: "h-7 w-7 text-muted-foreground" }
                h2 { class: "mt-4 text-base font-semibold", {translate("git-no-repository")} }
                Dropdown {
                    class: "mt-5 w-full".to_string(),
                    open: open(),
                    on_dismiss: move |_| open.set(false),
                    button {
                        r#type: "button",
                        class: "flex h-10 w-full items-center gap-2 rounded-lg bg-foreground/[0.055] px-3 text-left text-ui text-foreground outline-none transition-colors hover:bg-foreground/[0.085] focus-visible:bg-foreground/[0.085]",
                        onclick: move |_| open.set(!open()),
                        BuiltinIconView {
                            icon: vmux_api::BuiltinIcon::Project,
                            class: "size-4 shrink-0 text-muted-foreground".to_string(),
                        }
                        span { class: "min-w-0 flex-1 truncate font-medium", "{current_label}" }
                    }
                    if open() {
                        DropdownPopup { match_width: true, class: "p-1.5 text-left outline-none",
                            if rows.is_empty() {
                                div { class: "px-2 py-2 text-ui text-muted-foreground", {translate("agent-project-none")} }
                            }
                            for project in rows.into_iter() {
                                {
                                    let path = project.path.clone();
                                    let pick_path = project.path.clone();
                                    let pick_label = project.label.clone();
                                    let pick_branch = project.branch.clone();
                                    let label = project.label.clone();
                                    let active = project.is_active;
                                    rsx! {
                                        button {
                                            key: "git-project-{path}",
                                            r#type: "button",
                                            disabled: project.missing,
                                            class: if active {
                                                "flex h-9 w-full items-center gap-2 rounded-md bg-foreground/[0.065] px-2 text-ui text-foreground outline-none hover:bg-foreground/[0.09] disabled:opacity-40"
                                            } else {
                                                "flex h-9 w-full items-center gap-2 rounded-md px-2 text-ui text-muted-foreground outline-none hover:bg-foreground/[0.055] hover:text-foreground disabled:opacity-40"
                                            },
                                            onclick: move |_| {
                                                open.set(false);
                                                let request = GitRepositoryRequest::project(
                                                    pick_path.clone(),
                                                    pick_label.clone(),
                                                    pick_branch.clone(),
                                                );
                                                page.request_repository(request);
                                                let _ = send(&ProjectActivateRequest {
                                                    path: pick_path.clone(),
                                                    branch: String::new(),
                                                    checkout: String::new(),
                                                    pane_id: None,
                                                });
                                            },
                                            span { class: "min-w-0 flex-1 truncate text-left font-medium", "{label}" }
                                            if active {
                                                LineIconView { icon: LineIcon::Check, class: "size-3.5 shrink-0" }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "my-1 h-px bg-foreground/[0.08]" }
                            button {
                                r#type: "button",
                                class: "flex h-9 w-full items-center gap-2 border-0 px-2 text-ui text-muted-foreground outline-none hover:text-foreground",
                                onclick: move |_| {
                                    open.set(false);
                                    let _ = send(&GitRepositoryPickerRequest { path: workspace.clone() });
                                },
                                LineIconView { icon: LineIcon::Plus, class: "size-4 shrink-0" }
                                span { {translate("agent-project-choose-another")} }
                            }
                        }
                    }
                }
                if !message.is_empty() {
                    p { class: "mt-3 text-ui text-destructive", "{message}" }
                } else if !current_path.is_empty() {
                    p { class: "mt-3 max-w-full truncate font-mono text-[10px] text-muted-foreground", "{current_path}" }
                }
            }
        }
    }
}

#[component]
fn GitLoadingSkeleton() -> Element {
    rsx! {
        main { class: "flex min-h-0 flex-1 flex-col bg-transparent text-foreground", "data-vmux-loading-surface": "",
            Split {
                width: f64::from(vmux_api::editor::TreeWidth::DEFAULT),
                sidebar: rsx! {
                aside { class: "flex min-h-0 flex-col overflow-hidden bg-transparent",
                    ManagerSearchRow {
                        div { class: "flex h-7 min-w-0 flex-1 items-center gap-1.5 px-1",
                            LineIconView { icon: LineIcon::Search, class: "size-3.5 shrink-0 text-muted-foreground/70" }
                            Skeleton { class: "h-2 w-20 bg-foreground/[0.045]" }
                        }
                    }
                    div { class: "min-h-0 flex-1 overflow-hidden px-2 pb-2",
                        FileTreeSkeleton { rows: 4, class: "py-2" }
                    }
                }
                },
                div { class: "flex min-h-0 min-w-0 flex-col",
                    PageToolbarRow { class: "justify-end gap-2 px-3".to_string(),
                        Skeleton { class: "h-2 w-16 bg-foreground/[0.045]" }
                        Skeleton { class: "h-7 w-16 rounded-md bg-foreground/[0.055]" }
                    }
                    section { class: "min-h-0 min-w-0 flex-1 overflow-hidden bg-transparent pb-2.5 pr-2.5",
                    div { class: "flex h-full min-h-0 flex-col gap-3 overflow-hidden",
                        for (path_width, rows) in [("w-52", 8usize), ("w-44", 5usize)] {
                            div { class: "flex min-h-44 flex-col overflow-hidden rounded-lg bg-foreground/[0.025]",
                                div { class: "flex h-9 shrink-0 items-center gap-2 px-3",
                                    LineIconView { icon: LineIcon::File, class: "size-3.5 shrink-0 text-muted-foreground" }
                                    Skeleton { class: "h-3 {path_width} bg-foreground/[0.06]" }
                                }
                                div { class: "flex flex-col gap-2 px-3 py-3",
                                    for index in 0..rows {
                                        Skeleton { class: if index % 3 == 1 { "h-3 w-9/12 bg-foreground/[0.045]" } else { "h-3 w-full bg-foreground/[0.045]" } }
                                    }
                                }
                            }
                        }
                    }
                    }
                }
            }
        }
    }
}
