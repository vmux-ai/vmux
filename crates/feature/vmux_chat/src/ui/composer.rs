use self::menu::{CommandMenu, MediaMenu, ResumeMenu};
use self::options::{ChatComposerMenus, ChatModelMenu};
use super::approval::ChoiceList;
use super::keys::ChatKeys;
use super::state::Chat;
use super::transcript::QueuedPrompts;
use crate::event::{ChatPasteMedia, ChatPickFiles};
use dioxus::prelude::*;
use vmux_ui::agent_accent::AgentAccent;
use vmux_ui::components::composer::PromptComposer;
use vmux_ui::components::composer_bar::ComposerBar;
use vmux_ui::components::mcp_menu::McpMenu;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn ChatDock(chat: Chat) -> Element {
    rsx! {
        div { class: "relative z-20 px-4 pb-[calc(0.75rem+env(safe-area-inset-bottom))] pt-2",
            div { class: "session-chat-prompt-shell relative mx-auto flex max-w-3xl flex-col gap-2 drop-shadow-[0_20px_32px_rgba(0,0,0,0.28)]",
                if chat.media_menu_open() {
                    MediaMenu { chat }
                }
                if chat.command_menu_open() {
                    CommandMenu { chat }
                }
                if chat.mcp_menu_open() {
                    McpMenu {
                        connections: chat.mcp,
                        entries: chat.filtered_mcp_servers(),
                        selected: (chat.slash.menu_sel)(),
                        on_select: move |index| chat.choose_list(index),
                        on_hover: move |index| chat.point_at_list(index),
                        on_dismiss: move |()| chat.dismiss_selector(),
                    }
                }
                if chat.resume_menu_open() {
                    ResumeMenu { chat }
                }
                if chat.model_menu_open() {
                    ChatModelMenu { chat }
                }
                ChatComposerMenus { chat }
                ChoiceList { chat }
                QueuedPrompts { chat }
                ChatComposer { chat }
            }
        }
    }
}

#[component]
fn ChatComposer(chat: Chat) -> Element {
    let accent = AgentAccent::for_agent(&chat.agent());
    let keys = use_context::<ChatKeys>();
    let drafted = chat.draft();
    rsx! {
        PromptComposer {
            shared_transition: true,
            show_send_button: false,
            value: drafted,
            preview: String::new(),
            attachments: chat.composer_attachments(),
            placeholder: if chat.choice_pending() { translate("agent-choose-option") } else { translate("command-composer-placeholder") },
            accent_color: chat.accent().css,
            accent_gradient: accent.grad.to_string(),
            footer: Some(rsx! {
                ComposerFooter { chat }
            }),
            mode: chat.prompt_mode(),
            action_title: chat.prompt_action_title(),
            action_enabled: chat.prompt_action_enabled(),
            on_input: move |value| chat.edit_draft(value),
            on_keydown: move |event| keys.on_prompt_keydown(event),
            on_paste: move |_| {
                let _ = send(&ChatPasteMedia);
            },
            on_attach: move |_| {
                let _ = send(&ChatPickFiles);
            },
            on_remove_attachment: move |index| chat.remove_attachment(index),
            on_action: move |_| {
                if chat.streaming() {
                    chat.stop_or_flush();
                } else if chat.mcp_menu_open() {
                    chat.choose_list((chat.slash.menu_sel)());
                } else {
                    chat.submit();
                }
            },
        }
    }
}

#[component]
fn ComposerFooter(chat: Chat) -> Element {
    let context = chat.slash.context();
    let transcript = chat.transcript.current();
    let queued = chat.queue.queued();
    rsx! {
        ComposerBar {
            opened: chat.menu.opened(),
            model: chat.model_chip(),
            effort: chat.effort_chip(),
            permission: chat.permission_chip(),
            project: chat.project_chip(),
            branch: chat.branch_chip(),
            is_git_repo: context.is_git_repo,
            workspace_known: context.workspace_selected,
            uncommitted: context.uncommitted,
            ahead: context.ahead,
            status: chat.status(),
            active_subagents: transcript.active_subagents as usize,
            active_tasks: transcript.active_tasks as usize,
            queued_count: queued.len(),
        }
    }
}

mod menu;
pub(crate) mod options;
