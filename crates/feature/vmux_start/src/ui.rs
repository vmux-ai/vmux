#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::button::Button;
use vmux_ui::components::manager::ManagerPage;
use vmux_ui::navigation::PageNavigation;

use vmux_ui::hooks::use_theme;
use vmux_ui::i18n::translate;

use vmux_command::{CommandPalette, CommandPaletteSurface, use_command_bar_ui};

#[vmux_page::page(
    component = Page,
    states = ["PageAddressUiState", "ThemeUiState", "CommandBarUiState", "KeyClaimsUiState"]
)]
pub(crate) struct StartPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_command_bar_ui();
    let setup = state
        .read()
        .palette
        .projection
        .composer
        .agent_url
        .is_empty();

    rsx! {
        document::Title { "Any prompt. Anything, done." }
        document::Meta { name: "description", content: "Any prompt. Anything, done." }
        ManagerPage {
            div {
                class: "flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background px-5 py-8 text-foreground sm:px-8",
                div { class: "m-auto flex w-full max-w-2xl flex-col gap-3",
                    if setup {
                        SetupWelcome {}
                    } else {
                    div { class: "flex flex-col items-start gap-1 px-1 text-left",
                        h1 { class: "text-2xl font-semibold tracking-tight text-foreground", "vmux" }
                        p { class: "text-ui text-muted-foreground", {translate("start-tagline")} }
                    }
                    div { class: "relative w-full",
                        CommandPalette {
                            state,
                            surface: CommandPaletteSurface::Start,
                            on_close: move |_| {},
                            on_activity: move |_| {},
                        }
                    }
                    vmux_ui::components::starter_prompts::StarterPrompts {
                        on_choose: move |text| {
                            let _ = vmux_ui::hooks::send(&vmux_api::command_bar::PromptRequest {
                                text,
                                target_url: None,
                                attachments: Vec::new(),
                            });
                        },
                    }
                    }
                }
            }
        }
    }
}

#[component]
fn SetupWelcome() -> Element {
    rsx! {
        div { class: "flex flex-col items-start gap-4 px-1",
            h1 { class: "text-2xl font-semibold", {translate("onboarding-start-title")} }
            p { class: "text-sm text-muted-foreground", {translate("onboarding-start-description")} }
            Button {
                aria_label: translate("start-title"),
                onclick: move |_| PageNavigation::replace("vmux://sessions/inbox?setup=1"),
                {translate("start-title")}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::{BinEvent, command_bar::OpenRequest};
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostBinding, PageHost};

    #[derive(Default)]
    struct Host(RefCell<Vec<OpenRequest>>);

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            assert_eq!(id, OpenRequest::id());
            self.0
                .borrow_mut()
                .push(rkyv::from_bytes::<OpenRequest, rkyv::rancor::Error>(bytes).unwrap());
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[test]
    fn start_opens_the_setup_conversation_without_submitting_a_work_prompt() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new(SetupWelcome);
        dom.provide_root_context(HostBinding(host.clone()));
        let initial = dom.rebuild_to_vec();
        assert!(host.0.borrow().is_empty());
        let html = dioxus_ssr::render(&dom);
        for prompt in vmux_ui::components::starter_prompts::StarterPrompt::all() {
            assert!(!html.contains(&prompt.text()));
        }
        let label = translate("start-title");
        let start = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == &label => Some(*id),
                _ => None,
            })
            .expect("Start must be available on the welcome screen");
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            start,
        );
        dom.render_immediate_to_vec();
        let requests = host.0.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].value, "vmux://sessions/inbox?setup=1");
        assert_eq!(
            requests[0].open,
            Some(vmux_api::open_target::OpenTarget::InPlace)
        );
    }
}
