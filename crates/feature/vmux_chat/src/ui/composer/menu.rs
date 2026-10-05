use crate::ui::format::ResumeMenuState;
use crate::ui::state::Chat;
use dioxus::prelude::*;
use vmux_api::command_bar::CommandBarResultItem;
use vmux_command::ResultRow;
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn MediaMenu(chat: Chat) -> Element {
    let media = chat.media.current();
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
            PromptMediaOptions {
                items: chat.media_options(),
                selected: chat.list_selection(),
                loading: media.loading,
                loading_label: translate("agent-loading-media"),
                empty_label: translate("agent-no-matching-media"),
                on_hover: move |index| chat.point_at_list(index),
                on_select: move |index| chat.choose_list(index),
            }
        }
    }
}

#[component]
pub(super) fn CommandMenu(chat: Chat) -> Element {
    rsx! {
        PromptPopup { on_dismiss: move |()| chat.dismiss_selector(),
            for (index , command) in chat.filtered_commands().into_iter().enumerate() {
                {
                    rsx! {
                        ResultRow {
                            key: "sc{index}",
                            index,
                            item: CommandBarResultItem {
                                key: command.command.name().to_string(),
                                leading: "/".to_string(),
                                title: command.command.name().to_string(),
                                subtitle: command.description.clone(),
                                trailing: "\u{21b5}".to_string(),
                                ..Default::default()
                            },
                            selected: index == chat.list_selection(),
                            on_activate: move |()| chat.choose_list(index),
                            on_hover: move |()| chat.point_at_list(index),
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn ResumeMenu(chat: Chat) -> Element {
    let state = chat.resume_state();
    let resume = chat.resume.current();
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
                for (index , item) in resume.rows.iter().cloned().enumerate() {
                    ResultRow {
                        key: "rs{index}",
                        index,
                        item,
                        selected: index == chat.list_selection(),
                        on_activate: move |()| chat.choose_list(index),
                        on_hover: move |()| chat.point_at_list(index),
                    }
                }
            }
        }
    }
}
