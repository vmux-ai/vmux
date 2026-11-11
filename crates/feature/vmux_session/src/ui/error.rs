use super::state::Chat;
use crate::event::ChatRetrySession;
use crate::{Failure, FailureKind};
use dioxus::core::Task;
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::composer::ComposerFrame;
use vmux_ui::components::notice::{Notice, NoticeKind};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::navigation::PageNavigation;

#[component]
pub(super) fn ChatFeedback(chat: Chat, #[props(default = rsx! {})] children: Element) -> Element {
    let message = chat.run.error();
    let status = chat.status();
    let kind = Failure(&message).kind();
    #[cfg(not(target_os = "ios"))]
    let setup_url = Some(format!("{}acp", vmux_tool::ToolPlugin::URL));
    #[cfg(target_os = "ios")]
    let setup_url: Option<String> = None;
    let mut visible = use_signal(|| true);
    let mut seen = use_signal(|| None::<(String, String)>);
    let mut dismiss_task = use_signal(|| None::<Task>);
    use_drop(move || {
        if let Some(task) = dismiss_task.write().take() {
            task.cancel();
        }
    });
    let observed_message = message.clone();
    let observed_status = status.clone();
    use_effect(use_reactive!(|(observed_message, observed_status)| {
        let identity = (observed_status.clone(), observed_message.clone());
        if seen.peek().as_ref() == Some(&identity) {
            return;
        }
        seen.set(Some(identity));
        visible.set(true);
        if let Some(task) = dismiss_task.write().take() {
            task.cancel();
        }
        if observed_status == "errored" && kind == FailureKind::Other {
            dismiss_task.set(Some(spawn(async move {
                vmux_ui::platform::Platform::sleep(5000).await;
                visible.set(false);
                dismiss_task.set(None);
            })));
        }
    }));
    let shown = status == "errored" && !message.is_empty() && visible();
    if shown && kind == FailureKind::Busy {
        return rsx! {
            details { class: "disclosure rounded-xl bg-background/85 text-xs text-muted-foreground",
                summary { class: "flex min-h-9 cursor-pointer items-center gap-2 px-3 py-1.5",
                    span { class: "min-w-0 flex-1 truncate", role: "status", title: translate("sessions-busy-detail"), {translate("sessions-busy-detail")} }
                    Button { size: ButtonSize::Xs, variant: ButtonVariant::Ghost,
                        aria_label: translate("common-retry"),
                        onclick: move |event: MouseEvent| { event.prevent_default(); event.stop_propagation(); let _ = send(&ChatRetrySession); },
                        {translate("common-retry")}
                    }
                }
                div { class: "px-1 pb-1", {children} }
            }
        };
    }
    let feedback = match kind {
        FailureKind::Busy => rsx! {},
        FailureKind::MissingHistory => rsx! {
            Notice {
                attached: true,
                message: translate("sessions-history-unavailable"),
                kind: NoticeKind::Warning,
                actions: rsx! {
                    Button { size: ButtonSize::Xs, variant: ButtonVariant::Ghost,
                        aria_label: translate("sessions-start"),
                        onclick: move |_| PageNavigation::replace("vmux://start/"),
                        {translate("sessions-start")}
                    }
                },
                on_dismiss: move |_| visible.set(false),
            }
        },
        FailureKind::Connection => rsx! {
            Notice {
                attached: true,
                message: translate("sessions-connection-interrupted"),
                kind: NoticeKind::Error,
                actions: rsx! {
                    Button { size: ButtonSize::Xs, variant: ButtonVariant::Ghost,
                        aria_label: translate("common-retry"),
                        onclick: move |_| { let _ = send(&ChatRetrySession); },
                        {translate("common-retry")}
                    }
                },
                on_dismiss: move |_| visible.set(false),
            }
        },
        FailureKind::Setup => rsx! {
            Notice {
                attached: true,
                message: translate("agent-error-version-suggestion"),
                kind: NoticeKind::Warning,
                actions: rsx! {
                    if let Some(url) = setup_url {
                        Button { size: ButtonSize::Xs, variant: ButtonVariant::Ghost,
                            aria_label: translate("agent-error-open-agents"),
                            onclick: move |_| PageNavigation::replace(url.clone()),
                            {translate("agent-error-open-agents")}
                        }
                    }
                },
                on_dismiss: move |_| visible.set(false),
            }
        },
        FailureKind::Other => rsx! {
            Notice {
                message: translate("sessions-operation-failed"),
                kind: NoticeKind::Error, floating: true,
                on_dismiss: move |_| visible.set(false),
            }
        },
    };
    let notice = (shown && kind != FailureKind::Other).then(|| feedback.clone());
    rsx! {
        ComposerFrame { notice, {children} }
        if shown && kind == FailureKind::Other {
            {feedback}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ChatSnapshot;
    use crate::state::ChatUiState;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::{BinEvent, UiStateFrame, command_bar::OpenRequest};
    use vmux_ui::transport::{
        BytesListener, HostBinding, PageHost, event_listener::EventListenerError,
    };

    #[component]
    fn FeedbackPage() -> Element {
        vmux_ui::hooks::use_ui_state::<ChatUiState>();
        let chat = super::super::state::use_chat("session".into());
        rsx! { ChatFeedback { chat } }
    }

    #[derive(Default)]
    struct FeedbackHost {
        listeners: RefCell<Vec<BytesListener>>,
        requests: RefCell<Vec<String>>,
    }

    impl PageHost for FeedbackHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == ChatRetrySession::id() {
                self.requests.borrow_mut().push("retry".into());
            } else if id == OpenRequest::id() {
                let request = rkyv::from_bytes::<OpenRequest, rkyv::rancor::Error>(bytes).unwrap();
                self.requests.borrow_mut().push(request.value);
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

    impl FeedbackHost {
        fn fail(&self, message: &str) {
            let frame = UiStateFrame::<ChatUiState, crate::state::ChatUiStatePatch>::Snapshot(
                ChatUiState {
                    snapshot: ChatSnapshot {
                        status: "errored".into(),
                        error: message.into(),
                        ..Default::default()
                    },
                    snapshot_ready: true,
                    ..Default::default()
                },
            );
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            for listener in &mut *self.listeners.borrow_mut() {
                listener(&bytes);
            }
        }
    }

    #[test]
    fn failures_hide_protocol_details_and_offer_only_relevant_recovery() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let _runtime = runtime.enter();
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        for (message, role, label, request) in [
            (
                "Invalid request: {\"reason\":\"thread_active_writer\"}",
                "status",
                "common-retry",
                Some("retry"),
            ),
            (
                "session/load failed: no rollout found for thread id abc",
                "status",
                "sessions-start",
                Some("vmux://start/"),
            ),
            (
                "session/load timed out",
                "alert",
                "common-retry",
                Some("retry"),
            ),
            (
                "agent command not found",
                "status",
                "agent-error-open-agents",
                Some("vmux://tools/acp"),
            ),
            (
                "operation failed unexpectedly",
                "alert",
                "common-close",
                None,
            ),
        ] {
            let host = Rc::new(FeedbackHost::default());
            let mut dom = VirtualDom::new(FeedbackPage);
            dom.provide_root_context(HostBinding(host.clone()));
            dom.rebuild_in_place();
            dom.render_immediate_to_vec();
            host.fail(message);
            let update = dom.render_immediate_to_vec();
            dom.render_immediate_to_vec();
            let html = dioxus_ssr::render(&dom);
            assert!(!html.contains(message), "raw failure leaked: {message}");
            assert!(!html.contains("thread_active_writer"));
            assert!(html.contains(&format!("role=\"{role}\"")), "{html}");
            assert!(html.contains(&translate(label)), "{html}");
            if request != Some("retry") {
                assert!(!html.contains(&translate("common-retry")));
            }
            let button = update
                .edits
                .iter()
                .find_map(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-label",
                        value: AttributeValue::Text(value),
                        id,
                        ..
                    } if value == &translate(label) => Some(*id),
                    _ => None,
                })
                .expect("feedback must offer recovery or dismissal");
            dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                button,
            );
            dom.render_immediate_to_vec();
            assert_eq!(
                *host.requests.borrow(),
                request.into_iter().collect::<Vec<_>>()
            );
            if request.is_none() {
                assert!(!dioxus_ssr::render(&dom).contains("role=\"alert\""));
            }
        }
    }
}
