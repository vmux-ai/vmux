#![allow(non_snake_case)]

use crate::event::{
    SPACE_KEY_EVENT, SPACES_LIST_EVENT, SpaceCommandEvent, SpaceKey, SpaceRow, SpacesListEvent,
};
use dioxus::prelude::*;
use vmux_core::event::team::{TEAM_EVENT, TeamCommandEvent, TeamEvent};
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::inline_edit::{EditableText, InlineEdit};
use vmux_ui::components::manager::{ManagerSelect, ManagerSelectItem, ManagerSelectItemKind};
use vmux_ui::hooks::{MenuDirection, send, use_event, use_key_claim, use_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::platform::sleep_ms;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(SpacesListEvent::default);
    let mut selected = use_signal(|| 0usize);
    let team = use_event::<TeamEvent>(TEAM_EVENT, TeamEvent::default);

    let _listener = use_listener::<SpacesListEvent, _>(SPACES_LIST_EVENT, move |data| {
        let active = data
            .spaces
            .iter()
            .position(|space| space.is_active)
            .unwrap_or(0);
        selected.set(active);
        state.set(data);
    });

    let keys = use_key_claim(Unclaimed::Types, || vec!["spaces".to_string()]);
    SpaceKeys { state, selected }.listen();
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });

    let spaces = state.read().spaces.clone();
    let count = spaces.len();
    let sel = selected().min(count.saturating_sub(1));
    let active_name = spaces
        .iter()
        .find(|space| space.is_active)
        .map(|space| space.name.clone())
        .unwrap_or_else(|| {
            translate_with(
                "spaces-default-name",
                &[("number", TranslationValue::Number(1))],
            )
        });
    let profiles = team().profiles;
    let active_profile = profiles
        .iter()
        .find(|profile| profile.is_active)
        .map(|profile| profile.id.clone());
    let profile_items = profiles
        .iter()
        .map(|profile| ManagerSelectItem {
            value: profile.id.clone(),
            label: profile.name.clone(),
            kind: ManagerSelectItemKind::User,
        })
        .collect::<Vec<_>>();

    rsx! {
        div {
            id: "spaces-root",
            tabindex: "0",
            class: "flex h-full min-h-0 flex-col bg-background text-foreground outline-none",
            onmounted: move |e| async move {
                if let Err(error) = e.set_focus(true).await {
                    dioxus::logger::tracing::warn!("focusing the space page failed: {error:?}");
                }
            },
            onkeydown: move |e| keys.on_keydown(&e, |_| false),
            div { class: "flex flex-wrap items-center justify-between gap-3 border-b border-border px-5 py-4",
                div { class: "min-w-0",
                    h1 { class: "text-lg font-semibold", {translate("spaces-title")} }
                    div { class: "mt-1 truncate text-xs text-muted-foreground", "{active_name}" }
                }
                div { class: "w-40 max-w-full shrink-0",
                    if !profile_items.is_empty() {
                        ManagerSelect {
                            items: profile_items,
                            value: active_profile,
                            placeholder: translate("team-profile-name"),
                            onselect: move |profile_id| {
                                let _ = send(&TeamCommandEvent {
                                    command: "switch_profile".to_string(),
                                    member_id: None,
                                    profile_id: Some(profile_id),
                                    profile_name: None,
                                });
                            },
                        }
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-5 py-5",
                div { class: "mx-auto w-full max-w-4xl",
                    if spaces.is_empty() {
                        div { class: "flex min-h-72 items-center justify-center text-sm text-muted-foreground", {translate("spaces-empty")} }
                    } else {
                        div { class: "grid grid-cols-[repeat(auto-fit,minmax(14rem,18rem))] justify-center gap-3",
                            for (index, space) in spaces.iter().enumerate() {
                                SpaceRowView {
                                    key: "{space.id}",
                                    space: space.clone(),
                                    selected: index == sel,
                                    deletable: count > 1,
                                }
                            }
                            NewSpaceCard { count }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SpaceKeys {
    state: Signal<SpacesListEvent>,
    selected: Signal<usize>,
}

impl SpaceKeys {
    fn listen(self) {
        let mut keys = self;
        let _resolved = use_listener::<SpaceKey, _>(SPACE_KEY_EVENT, move |key| keys.apply(key));
    }

    fn apply(&mut self, key: SpaceKey) {
        match key {
            SpaceKey::Next => self.move_selection(MenuDirection::Next),
            SpaceKey::Previous => self.move_selection(MenuDirection::Previous),
            SpaceKey::Attach => self.attach(),
            SpaceKey::Delete => self.delete(),
        }
    }

    fn move_selection(&mut self, direction: MenuDirection) {
        let count = self.state.peek().spaces.len();
        let from = self.row();
        let landed = match direction {
            MenuDirection::Next => (from + 1).min(count.saturating_sub(1)),
            MenuDirection::Previous => from.saturating_sub(1),
        };
        self.selected.set(landed);
    }

    fn attach(&self) {
        let Some(id) = self.selected_id() else {
            return;
        };
        emit_command("attach", Some(id), None);
    }

    fn delete(&self) {
        if self.state.peek().spaces.len() <= 1 {
            return;
        }
        let Some(id) = self.selected_id() else {
            return;
        };
        emit_command("delete", Some(id), None);
    }

    fn row(&self) -> usize {
        let count = self.state.peek().spaces.len();
        (*self.selected.peek()).min(count.saturating_sub(1))
    }

    fn selected_id(&self) -> Option<String> {
        let row = self.row();
        let state = self.state.peek();
        state.spaces.get(row).map(|space| space.id.clone())
    }
}

fn new_space_name(typed: &str, count: usize) -> String {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        translate_with(
            "spaces-default-name",
            &[("number", TranslationValue::Number((count + 1) as i64))],
        )
    } else {
        trimmed.to_string()
    }
}

fn emit_command(command: &str, space_id: Option<String>, name: Option<String>) {
    let _ = send(&SpaceCommandEvent {
        command: command.to_string(),
        space_id,
        name,
    });
}

#[component]
fn SpaceRowView(space: SpaceRow, selected: bool, deletable: bool) -> Element {
    let editing = use_signal(|| false);
    let draft = use_signal(|| space.name.clone());
    let menu_value = use_signal(|| space.id.clone());
    let nav_id = space.id.clone();
    let delete_id = space.id.clone();
    let rename_id = space.id.clone();
    let class = if selected {
        "relative flex min-h-24 cursor-pointer items-center justify-between rounded-xl border border-primary/40 bg-primary/[0.08] px-3 py-3 shadow-[0_0_18px_-6px_color-mix(in_oklab,var(--primary)_50%,transparent)]"
    } else {
        "glass relative flex min-h-24 cursor-pointer items-center justify-between rounded-xl border border-border/70 px-3 py-3 transition-colors hover:border-primary/40 hover:bg-glass-hover"
    };
    let tab_label = translate_with(
        "spaces-tabs",
        &[("count", TranslationValue::Number(space.tab_count as i64))],
    );

    rsx! {
        ContextMenu {
            ContextMenuTrigger { attributes: vec![],
                div {
                    class: "{class}",
                    button {
                        r#type: "button",
                        class: if editing() {
                            "pointer-events-none absolute inset-0 z-0 rounded-xl outline-none"
                        } else {
                            "absolute inset-0 z-0 rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary/60"
                        },
                        title: space.name.clone(),
                        aria_label: space.name.clone(),
                        disabled: editing(),
                        onclick: move |_| {
                        emit_command("attach", Some(nav_id.clone()), None);
                        },
                    }
                    div { class: "pointer-events-none relative z-10 flex min-w-0 flex-1 items-center justify-between",
                        div { class: "min-w-0 flex-1",
                            div { class: "flex min-w-0 items-center gap-2",
                                EditableText {
                                    value: space.name.clone(),
                                    editing,
                                    draft,
                                    display_class: "pointer-events-auto min-w-0 cursor-text truncate rounded px-1 py-0.5 text-left text-sm font-medium text-foreground hover:bg-foreground/[0.06]".to_string(),
                                    input_class: "pointer-events-auto min-w-0 flex-1 rounded-md bg-background/70 px-2 py-1 text-sm font-medium text-foreground outline-none ring-1 ring-inset ring-primary/40".to_string(),
                                    title: translate("common-rename"),
                                    placeholder: translate("spaces-new-placeholder"),
                                    on_commit: move |name| emit_command("rename", Some(rename_id.clone()), Some(name)),
                                }
                            if space.is_active {
                                span { class: "rounded-full bg-primary/15 px-2 py-0.5 text-xs text-primary", {translate("common-active")} }
                            }
                        }
                        div { class: "mt-1 truncate px-1 text-xs text-muted-foreground", "{space.profile}" }
                    }
                    div { class: "ml-3 flex shrink-0 items-center gap-2",
                        div { class: "text-xs text-muted-foreground", "{tab_label}" }
                        if deletable {
                            button {
                                class: "pointer-events-auto flex h-7 w-7 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-foreground/10 hover:text-foreground",
                                title: translate("spaces-delete"),
                                "aria-label": translate("spaces-delete"),
                                onclick: move |e| {
                                    e.stop_propagation();
                                    emit_command("delete", Some(delete_id.clone()), None);
                                },
                                span { class: "text-base leading-none", "\u{00d7}" }
                            }
                        }
                    }
                    }
                }
            }
            ContextMenuContent { attributes: vec![],
                ContextMenuItem {
                    index: 0usize,
                    value: Into::<ReadSignal<String>>::into(menu_value),
                    on_select: {
                        let name = space.name.clone();
                        move |_: String| begin_space_rename(editing, draft, name.clone())
                    },
                    attributes: vec![],
                    {translate("common-rename")}
                }
                ContextMenuItem {
                    index: 1usize,
                    value: Into::<ReadSignal<String>>::into(menu_value),
                    disabled: !deletable,
                    on_select: move |_: String| emit_command("delete", Some(space.id.clone()), None),
                    attributes: vec![],
                    {translate("spaces-delete")}
                }
            }
        }
    }
}

#[component]
fn NewSpaceCard(count: usize) -> Element {
    let mut creating = use_signal(|| false);
    let mut draft = use_signal(String::new);

    if creating() {
        return rsx! {
            div { class: "glass flex min-h-24 items-center rounded-xl border border-border/70 p-2",
                InlineEdit {
                    draft,
                    class: "m-1 min-w-0 flex-1 rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary/50".to_string(),
                    placeholder: translate("spaces-new-placeholder"),
                    aria_label: translate("spaces-new-placeholder"),
                    allow_empty: true,
                    restore_focus_id: "new-space".to_string(),
                    on_commit: move |name: String| {
                        creating.set(false);
                        emit_command("new", None, Some(new_space_name(&name, count)));
                    },
                    on_cancel: move |_| creating.set(false),
                }
            }
        };
    }

    rsx! {
        button {
            id: "new-space",
            r#type: "button",
            class: "flex min-h-24 items-center justify-center gap-2 rounded-xl border border-dashed border-border text-sm font-medium text-muted-foreground transition-colors hover:border-foreground/25 hover:bg-foreground/[0.035] hover:text-foreground",
            onclick: move |_| {
                draft.set(String::new());
                creating.set(true);
            },
            svg { class: "size-4", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8",
                path { d: "M12 5v14M5 12h14" }
            }
            {translate("common-new")}
        }
    }
}

fn begin_space_rename(mut editing: Signal<bool>, mut draft: Signal<String>, name: String) {
    draft.set(name);
    spawn(async move {
        sleep_ms(0).await;
        editing.set(true);
    });
}
