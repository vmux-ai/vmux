use crate::format::ResumeMenuState;
use crate::ui::state::Chat;
use dioxus::prelude::*;
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::i18n::translate;
use vmux_ui::launcher::results::{CommandBarResultItem, ResumeRows};
use vmux_ui::launcher::row::ResultRow;

#[component]
pub(super) fn MediaMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
            PromptMediaOptions {
                items: chat.media_options(),
                selected: menu_sel(),
                loading: (chat.media.loading)(),
                loading_label: translate("agent-loading-media"),
                empty_label: translate("agent-no-matching-media"),
                on_hover: move |index| menu_sel.set(index),
                on_select: move |index| {
                    if let Some(entry) = chat.media.entries.peek().get(index).cloned() {
                        chat.select_media_entry(&entry);
                    }
                },
            }
        }
    }
}

#[component]
pub(super) fn CommandMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
            for (index , command) in chat.filtered_commands().into_iter().enumerate() {
                {
                    let hint = if command.name == "mcp" {
                        translate("mcp-command-description")
                    } else {
                        command.description.clone()
                    };
                    rsx! {
                        ResultRow {
                            key: "sc{index}",
                            index,
                            item: CommandBarResultItem::Slash {
                                name: command.name.clone(),
                                hint,
                            },
                            selected: index == menu_sel(),
                            on_activate: {
                                let name = command.name.clone();
                                move |()| chat.run_slash_command(&name)
                            },
                            on_hover: move |()| menu_sel.set(index),
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn ResumeMenu(chat: Chat) -> Element {
    let mut menu_sel = chat.slash.menu_sel;
    let state = chat.resume_state();
    let note = match state {
        Some(ResumeMenuState::Loading) => Some(translate("agent-loading-sessions")),
        Some(ResumeMenuState::Empty) => Some(translate("agent-no-resumable-sessions")),
        Some(ResumeMenuState::NoMatch) => Some(translate("agent-no-matching-sessions")),
        _ => None,
    };
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
            if let Some(note) = note {
                div { class: "px-3.5 py-2 text-sm text-muted-foreground", "{note}" }
            } else {
                for (index , item) in ResumeRows::all(&chat.filtered_sessions()).into_iter().enumerate() {
                    if let CommandBarResultItem::Resume { entry, .. } = &item {
                        ResultRow {
                            key: "rs{index}",
                            index,
                            item: item.clone(),
                            selected: index == menu_sel(),
                            on_activate: {
                                let session = entry.clone();
                                move |()| chat.select_resume_session(&session)
                            },
                            on_hover: move |()| menu_sel.set(index),
                        }
                    }
                }
            }
        }
    }
}
