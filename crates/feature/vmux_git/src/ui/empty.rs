use dioxus::prelude::*;
use vmux_ecs::event::space::ProjectActivateRequest;
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

use super::state::GitPageState;
use crate::event::GitRepositoryPickerRequest;

#[component]
pub(super) fn EmptyRepository() -> Element {
    let GitPageState {
        snapshot, projects, ..
    } = use_context::<GitPageState>();
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
                        TypeIcon {
                            path: current_path.clone(),
                            is_dir: true,
                            class: "size-4 shrink-0 opacity-80"
                        }
                        span { class: "min-w-0 flex-1 truncate font-medium", "{current_label}" }
                        LineIconView { icon: LineIcon::ChevronDown, class: "size-3.5 shrink-0 text-muted-foreground" }
                    }
                    if open() {
                        div { class: "absolute left-0 right-0 top-11 z-40 rounded-lg border-0 bg-background p-1.5 text-left shadow-[0_16px_48px_rgba(0,0,0,0.28)] outline-none",
                            if rows.is_empty() {
                                div { class: "px-2 py-2 text-ui text-muted-foreground", {translate("agent-project-none")} }
                            }
                            for project in rows.into_iter() {
                                {
                                    let path = project.path.clone();
                                    let pick_path = project.path.clone();
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
                                                let _ = send(&ProjectActivateRequest {
                                                    path: pick_path.clone(),
                                                    branch: String::new(),
                                                    checkout: String::new(),
                                                    pane_id: None,
                                                });
                                            },
                                            TypeIcon { path, is_dir: true, class: "size-4 shrink-0 opacity-80" }
                                            span { class: "min-w-0 flex-1 truncate font-medium", "{project.label}" }
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
        main { class: "flex min-h-0 flex-1 items-center justify-center bg-background px-6 py-10 text-foreground",
            div { class: "flex w-full max-w-sm flex-col items-center text-center",
                LineIconView { icon: LineIcon::GitBranch, class: "h-7 w-7 text-muted-foreground" }
                Skeleton { class: "mt-4 h-4 w-48 bg-foreground/[0.07]" }
                div { class: "mt-5 flex h-10 w-full items-center gap-2 rounded-lg bg-foreground/[0.055] px-3",
                    Skeleton { class: "size-4 shrink-0 bg-foreground/[0.07]" }
                    Skeleton { class: "h-3 w-28 bg-foreground/[0.08]" }
                    LineIconView { icon: LineIcon::ChevronDown, class: "ml-auto size-3.5 shrink-0 text-muted-foreground/60" }
                }
            }
        }
    }
}
