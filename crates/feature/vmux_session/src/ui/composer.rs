use self::menu::{CommandMenu, MediaMenu, ResumeMenu};
use self::options::{ChatComposerMenus, ChatModelMenu};
use super::approval::ChoiceList;
use super::error::ChatFeedback;
use super::keys::ChatKeys;
use super::state::Chat;
use super::transcript::QueuedPrompts;
use crate::event::{ChatPasteMedia, ChatPickFiles};
use dioxus::prelude::*;
use vmux_ui::agent_accent::AgentAccent;
use vmux_ui::components::composer::PromptComposer;
use vmux_ui::components::composer_bar::{ComposerBar, ComposerChip};
use vmux_ui::components::mcp_menu::McpMenu;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn ChatDock(chat: Chat) -> Element {
    rsx! {
        div { class: "relative z-20 px-4 pb-[calc(0.75rem+env(safe-area-inset-bottom))] pt-2",
            div { class: "session-chat-prompt-shell relative mx-auto flex max-w-3xl flex-col gap-2",
                if !chat.in_use() {
                    if chat.media_menu_open() {
                        MediaMenu { chat }
                    }
                    if chat.command_menu_open() {
                        CommandMenu { chat }
                    }
                    if chat.mcp_menu_open() {
                        McpMenu {
                            state: chat.mcp.value.read().clone(),
                            entries: chat.filtered_mcp_servers(),
                            selected: chat.list_selection(),
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
                }
                ChatComposerMenus { chat }
                ChoiceList { chat }
                QueuedPrompts { chat }
                ChatFeedback { chat,
                    ChatComposer { chat }
                }
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
            disabled: chat.in_use(),
            show_send_button: false,
            value: drafted,
            preview: String::new(),
            attachments: chat.composer_attachments(),
            placeholder: if chat.in_use() { translate("sessions-busy-title") } else if chat.choice_pending() { translate("agent-choose-option") } else { translate("command-composer-placeholder") },
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
                    chat.choose_list(chat.list_selection());
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
    let mut status = chat.status();
    let mut model = chat.model_chip();
    let mut effort = chat.effort_chip();
    let mut permission = chat.permission_chip();
    let project = if !chat.projects.context_ready() {
        Some(ComposerChip::loading())
    } else if context.cwd.is_empty() {
        None
    } else {
        Some(ComposerChip::ready(
            context.workspace_name.clone(),
            context.cwd.clone(),
        ))
    };
    let branch = (!context.branch.is_empty())
        .then(|| ComposerChip::ready(context.branch.clone(), context.branch.clone()));
    if chat.in_use() {
        status.clear();
        model = model.map(|chip| chip.readonly());
        effort = effort.map(|chip| chip.readonly());
        permission = permission.map(|chip| chip.readonly());
    }
    rsx! {
        ComposerBar {
            opened: chat.menu.opened(),
            agent: Some(chat.agent_chip()),
            model,
            effort,
            permission,
            project,
            branch,
            is_git_repo: context.is_git_repo,
            workspace_known: context.workspace_selected,
            uncommitted: 0,
            ahead: 0,
            status,
            active_subagents: transcript.active_subagents as usize,
            active_tasks: transcript.active_tasks as usize,
            queued_count: queued.len(),
        }
    }
}

mod menu;
pub(crate) mod options;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        ChatComposerMenuKind, ChatComposerMenuRequest, ChatComposerMenuState, ChatSnapshot,
        ComposerContext,
    };
    use crate::state::{ChatUiState, ChatUiStatePatch};
    use crate::ui::keys::use_chat_keys;
    use crate::ui::state::use_chat;
    use dioxus::core::VirtualDom;
    use dioxus_html::SerializedHtmlEventConverter;
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::{BinEvent, UiStateFrame};
    use vmux_ecs::event::ProjectRow;
    use vmux_ui::transport::{
        BytesListener, HostBinding, PageHost, event_listener::EventListenerError,
    };

    #[component]
    fn LockedComposer() -> Element {
        vmux_ui::hooks::use_ui_state::<ChatUiState>();
        let chat = use_chat("locked-session".into());
        let keys = use_chat_keys(chat);
        use_context_provider(|| keys);
        rsx! { ChatDock { chat } }
    }

    #[derive(Default)]
    struct ComposerHost {
        listeners: RefCell<Vec<BytesListener>>,
        menus: RefCell<Vec<Option<ChatComposerMenuKind>>>,
    }

    impl PageHost for ComposerHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == ChatComposerMenuRequest::id() {
                let request =
                    rkyv::from_bytes::<ChatComposerMenuRequest, rkyv::rancor::Error>(bytes)
                        .unwrap();
                self.menus.borrow_mut().push(request.menu);
            }
            Ok(())
        }
        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            if id == ChatUiState::id() {
                self.listeners.borrow_mut().push(listener);
            }
            Ok(())
        }
    }

    impl ComposerHost {
        fn show(&self, menu: Option<ChatComposerMenuKind>) {
            let frame = UiStateFrame::<ChatUiState, ChatUiStatePatch>::Snapshot(ChatUiState {
                snapshot: ChatSnapshot {
                    status: "errored".into(),
                    error: "thread_active_writer".into(),
                    header_name: "Codex".into(),
                    agent_id: "codex".into(),
                    ..Default::default()
                },
                snapshot_ready: true,
                composer_ready: true,
                composer_draft: "Unsent draft".into(),
                composer: ComposerContext {
                    cwd: "/tmp/current".into(),
                    workspace_name: "Current project".into(),
                    branch: "feature/current".into(),
                    workspace_selected: true,
                    can_manage_workspace: true,
                    projects: vec![
                        ProjectRow {
                            path: "/tmp/current".into(),
                            label: "Current project".into(),
                            is_active: true,
                            ..Default::default()
                        },
                        ProjectRow {
                            path: "/tmp/other".into(),
                            label: "Another project".into(),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
                composer_menu: ChatComposerMenuState { menu, index: 0 },
                ..Default::default()
            });
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            for listener in &mut *self.listeners.borrow_mut() {
                listener(&bytes);
            }
        }
    }

    #[test]
    fn busy_sessions_keep_drafts_and_show_their_own_workspace_without_selectors() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let _runtime = runtime.enter();
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(ComposerHost::default());
        let mut dom = VirtualDom::new(LockedComposer);
        dom.provide_root_context(HostBinding(host.clone()));
        dom.rebuild_in_place();
        dom.render_immediate_to_vec();
        host.show(None);
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        let title = format!("{} · /tmp/current", translate("agent-project-choose"));
        let html = dioxus_ssr::render(&dom);
        assert!(!html.contains(&title), "{html}");
        assert!(!html.contains("Another project"), "{html}");
        assert!(html.contains("Current project"), "{html}");
        assert!(html.contains("feature/current"), "{html}");
        assert!(host.menus.borrow().is_empty());
        assert!(html.contains("Unsent draft"));
        let textarea = html
            .split("<textarea")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(textarea.contains("disabled"));
        let label = translate("sessions-busy-title");
        assert!(
            !html.contains(&format!(">{label}<")),
            "the composer footer repeats the busy notice"
        );
    }
}
