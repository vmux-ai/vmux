#![allow(non_snake_case)]

use crate::event::{
    SpaceAttachRequest, SpaceCreateRequest, SpaceDeleteRequest, SpaceFormCloseRequest,
    SpaceFormInputRequest, SpaceFormOpenRequest, SpaceFormState, SpaceRenameRequest, SpaceRow,
    SpacesUiState,
};
use dioxus::prelude::*;
use vmux_api::input::KeyContextRequest;
use vmux_ecs::event::team::TeamProfileSwitchRequest;
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::components::manager::{ManagerSelect, ManagerSelectItem};
use vmux_ui::hooks::Unclaimed;
use vmux_ui::hooks::{send, use_key_claim, use_theme, use_ui_state};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};

#[vmux_page::page(
    component = Page,
    states = ["ThemeUiState", "SpacesUiState", "KeyClaimsUiState"]
)]
pub(crate) struct SpacesPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<SpacesUiState>().state;
    let keys = use_key_claim(Unclaimed::Types, || vec!["spaces".to_string()]);
    use_drop(move || {
        let _ = send(&KeyContextRequest { keys: Vec::new() });
    });

    let current = state();
    let spaces = current.snapshot.spaces;
    let count = spaces.len();
    let selected = current.snapshot.selected as usize;
    let profiles = current.team.profiles;
    let form = current.form;
    let creating = form.open && form.space_id.is_none();
    let active_profile = profiles
        .iter()
        .find(|profile| profile.is_active)
        .map(|profile| profile.id.clone());
    let profile_items = profiles
        .iter()
        .map(|profile| ManagerSelectItem {
            value: profile.id.clone(),
            label: profile.name.clone(),
        })
        .collect::<Vec<_>>();

    rsx! {
        div {
            id: "spaces-root",
            tabindex: "0",
            class: "flex h-full min-h-0 flex-col bg-background text-foreground outline-none",
            onmounted: move |event| async move {
                if let Err(error) = event.set_focus(true).await {
                    dioxus::logger::tracing::warn!("focusing the space page failed: {error:?}");
                }
            },
            onkeydown: move |event| keys.on_keydown(&event, |_| false),
            header { class: "flex min-h-14 flex-wrap items-center gap-2 bg-foreground/[0.018] px-4 py-2.5",
                h1 { class: "text-base font-semibold tracking-tight", {translate("spaces-title")} }
                span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-[10px] tabular-nums text-muted-foreground",
                    {translate_with(
                        "common-items",
                        &[("count", TranslationValue::Number(count as i64))],
                    )}
                }
                div { class: "min-w-0 flex-1" }
                if !profile_items.is_empty() {
                    div { class: "w-36 max-w-full shrink-0",
                        ManagerSelect {
                            items: profile_items,
                            value: active_profile,
                            placeholder: translate("team-profile-name"),
                            onselect: move |profile_id| {
                                let _ = send(&TeamProfileSwitchRequest { profile_id });
                            },
                        }
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-4 py-4",
                div { class: "mx-auto w-full max-w-5xl rounded-lg bg-foreground/[0.025] p-1",
                    if spaces.is_empty() && !creating {
                        div { class: "flex min-h-56 items-center justify-center text-sm text-muted-foreground", {translate("spaces-empty")} }
                    } else {
                        for (index, space) in spaces.iter().enumerate() {
                            SpaceRowView {
                                key: "{space.id}",
                                space: space.clone(),
                                selected: index == selected,
                                deletable: count > 1,
                                form: form.clone(),
                            }
                        }
                    }
                    if creating {
                        NewSpaceRow { form: form.clone() }
                    } else {
                        button {
                            r#type: "button",
                            class: "flex h-11 w-full items-center gap-2 rounded-md px-3 text-left text-xs font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.04] hover:text-foreground",
                            onclick: move |_| {
                                let _ = send(&SpaceFormOpenRequest {
                                    space_id: None,
                                    draft: String::new(),
                                });
                            },
                            LineIconView { icon: LineIcon::Plus, class: "size-3.5".to_string() }
                            span { {translate("common-add")} }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SpaceRowView(space: SpaceRow, selected: bool, deletable: bool, form: SpaceFormState) -> Element {
    let editing = form.open && form.space_id.as_deref() == Some(space.id.as_str());
    let active = space.is_active;
    let nav_id = space.id.clone();
    let rename_id = space.id.clone();
    let rename_name = space.display_name();
    let action_id = space.id.clone();
    let action_name = space.display_name();
    let profile = space.profile.clone();
    let row_class = if active {
        "group relative flex min-h-14 items-center rounded-md bg-foreground/[0.055] px-3 shadow-[inset_2px_0_0_var(--primary)]"
    } else if selected {
        "group relative flex min-h-14 items-center rounded-md bg-foreground/[0.04] px-3"
    } else {
        "group relative flex min-h-14 items-center rounded-md px-3 transition-colors hover:bg-foreground/[0.04]"
    };
    let tab_label = translate_with(
        "spaces-tabs",
        &[("count", TranslationValue::Number(space.tab_count as i64))],
    );

    rsx! {
        div { class: "{row_class}",
            button {
                r#type: "button",
                class: if editing {
                    "pointer-events-none absolute inset-0 z-0 rounded-md outline-none"
                } else {
                    "absolute inset-0 z-0 rounded-md outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/50"
                },
                title: space.display_name(),
                aria_label: space.display_name(),
                disabled: editing,
                onclick: move |_| {
                    let _ = send(&SpaceAttachRequest { space_id: nav_id.clone() });
                },
            }
            div { class: "pointer-events-none relative z-10 flex min-w-0 flex-1 items-center gap-3",
                LineIconView {
                    icon: LineIcon::Planet,
                    class: if active {
                        "size-4 shrink-0 text-primary".to_string()
                    } else {
                        "size-4 shrink-0 text-muted-foreground".to_string()
                    },
                }
                div { class: "flex min-w-0 flex-1 items-center gap-2",
                    if editing {
                        SpaceFormView { form: form.clone() }
                    } else {
                        button {
                            r#type: "button",
                            class: "pointer-events-auto min-w-0 cursor-text truncate rounded px-1 py-0.5 text-left text-sm font-medium text-foreground hover:bg-foreground/[0.06]",
                            title: translate("common-rename"),
                            onclick: move |event| {
                                event.stop_propagation();
                                let _ = send(&SpaceFormOpenRequest {
                                    space_id: Some(rename_id.clone()),
                                    draft: rename_name.clone(),
                                });
                            },
                            {space.display_name()}
                        }
                    }
                    if active {
                        span { class: "inline-flex shrink-0 items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[9px] font-medium text-primary",
                            span { class: "size-1.5 rounded-full bg-primary" }
                            {translate("common-active")}
                        }
                    }
                }
                span { class: "hidden shrink-0 rounded-full bg-foreground/[0.045] px-2 py-0.5 text-[10px] text-muted-foreground sm:inline-flex", "{profile}" }
                span { class: "w-14 shrink-0 text-right text-[10px] tabular-nums text-muted-foreground", "{tab_label}" }
                div { class: "pointer-events-auto",
                    SpaceActions {
                        space_id: action_id,
                        name: action_name,
                        deletable,
                    }
                }
            }
        }
    }
}

#[component]
fn SpaceActions(space_id: String, name: String, deletable: bool) -> Element {
    let mut open = use_signal(|| false);
    let rename_id = space_id.clone();
    let delete_id = space_id;
    rsx! {
        Dropdown {
            open: open(),
            on_dismiss: move |_| open.set(false),
            button {
                r#type: "button",
                class: if open() {
                    "flex size-7 shrink-0 items-center justify-center rounded-md bg-foreground/[0.08] text-foreground"
                } else {
                    "flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity hover:bg-foreground/[0.08] hover:text-foreground group-hover:opacity-100 focus-visible:opacity-100"
                },
                title: translate("common-open"),
                aria_label: translate("common-open"),
                onclick: move |event| {
                    event.stop_propagation();
                    open.set(!open());
                },
                LineIconView { icon: LineIcon::Ellipsis, class: "size-3.5".to_string() }
            }
            if open() {
                div {
                    class: "absolute right-0 top-7 z-30 w-36 rounded-md bg-background p-1 shadow-xl ring-1 ring-inset ring-foreground/[0.12]",
                    onclick: move |event| event.stop_propagation(),
                    button {
                        r#type: "button",
                        class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
                        onclick: move |_| {
                            open.set(false);
                            let _ = send(&SpaceFormOpenRequest {
                                space_id: Some(rename_id.clone()),
                                draft: name.clone(),
                            });
                        },
                        LineIconView { icon: LineIcon::Pencil, class: "size-3.5 shrink-0".to_string() }
                        span { {translate("common-rename")} }
                    }
                    if deletable {
                        button {
                            r#type: "button",
                            class: "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] text-muted-foreground hover:bg-destructive/10 hover:text-destructive",
                            onclick: move |_| {
                                open.set(false);
                                let _ = send(&SpaceDeleteRequest { space_id: delete_id.clone() });
                            },
                            LineIconView { icon: LineIcon::Trash, class: "size-3.5 shrink-0".to_string() }
                            span { {translate("common-delete")} }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NewSpaceRow(form: SpaceFormState) -> Element {
    rsx! {
        div { class: "m-2 flex min-h-16 items-center gap-3 rounded-md bg-foreground/[0.055] px-4 py-3 shadow-[inset_2px_0_0_color-mix(in_oklab,var(--primary)_55%,transparent)]",
            LineIconView { icon: LineIcon::Planet, class: "size-4 shrink-0 text-primary".to_string() }
            SpaceFormView { form }
        }
    }
}

#[component]
fn SpaceFormView(form: SpaceFormState) -> Element {
    let mut finished = use_signal(|| false);
    let submission = SpaceFormSubmission(form.clone());
    let enter = submission.clone();
    let placeholder = if form.suggestion.is_empty() {
        translate("spaces-new-placeholder")
    } else {
        SpaceRow {
            name: form.suggestion.clone(),
            ..Default::default()
        }
        .display_name()
    };
    rsx! {
        input {
            r#type: "text",
            class: "pointer-events-auto h-8 min-w-0 flex-1 rounded-md bg-background/70 px-2.5 text-sm text-foreground outline-none ring-1 ring-inset ring-foreground/[0.10] focus:ring-primary/35",
            placeholder: "{placeholder}",
            aria_label: translate("spaces-new-placeholder"),
            value: form.draft,
            autofocus: true,
            autocomplete: "off",
            onclick: move |event| event.stop_propagation(),
            oninput: move |event: FormEvent| {
                let _ = send(&SpaceFormInputRequest { draft: event.value() });
            },
            onkeydown: move |event: KeyboardEvent| match event.key() {
                Key::Enter if !finished() => {
                    event.prevent_default();
                    finished.set(true);
                    enter.submit();
                }
                Key::Escape if !finished() => {
                    event.prevent_default();
                    finished.set(true);
                    let _ = send(&SpaceFormCloseRequest);
                }
                _ => {}
            },
            onblur: move |_| {
                if !finished() {
                    finished.set(true);
                    let _ = send(&SpaceFormCloseRequest);
                }
            },
        }
    }
}

#[derive(Clone)]
struct SpaceFormSubmission(SpaceFormState);

impl SpaceFormSubmission {
    fn submit(&self) {
        let mut name = self.0.draft.trim().to_string();
        if name.is_empty() && self.0.space_id.is_none() {
            name.clone_from(&self.0.suggestion);
        }
        match &self.0.space_id {
            Some(space_id) if !name.is_empty() => {
                let _ = send(&SpaceRenameRequest {
                    space_id: space_id.clone(),
                    name,
                });
            }
            Some(_) => {}
            None => {
                let _ = send(&SpaceCreateRequest { name });
            }
        }
        let _ = send(&SpaceFormCloseRequest);
    }
}
