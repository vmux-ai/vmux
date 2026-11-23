use dioxus::prelude::*;
use vmux_api::browser::{OpenExternalBrowserRequest, Url};

use crate::components::context_menu::ContextMenuItem;
use crate::hooks::send;
use crate::i18n::translate;

#[component]
pub fn ExternalBrowserMenuItem(url: String, index: usize) -> Element {
    let available = Url::parse(&url).is_ok();
    let value = use_memo(use_reactive!(|url| url.clone()));
    if !available {
        return rsx! {};
    }
    rsx! {
        ContextMenuItem {
            index,
            value: Into::<ReadSignal<String>>::into(value),
            attributes: vec![],
            on_select: move |url: String| { let _ = send(&OpenExternalBrowserRequest { url }); },
            {translate("browser-open-external")}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::context_menu::{ContextMenu, ContextMenuContent, ContextMenuTrigger};
    use crate::hooks::EventListenerError;
    use crate::transport::{BytesListener, HostBinding, PageHost};
    use dioxus::core::{Mutation, VirtualDom};
    use dioxus_html::{
        Code, EventData, Location, Modifiers, SerializedHtmlEventConverter, SerializedKeyboardData,
        SerializedMouseData,
    };
    use std::{cell::RefCell, rc::Rc};
    use vmux_api::BinEvent;

    #[derive(Default)]
    struct Host {
        sent: RefCell<Vec<String>>,
        url: RefCell<Option<Signal<String>>>,
    }

    impl PageHost for Host {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == OpenExternalBrowserRequest::id() {
                let request =
                    rkyv::from_bytes::<OpenExternalBrowserRequest, rkyv::rancor::Error>(bytes)
                        .unwrap();
                self.sent.borrow_mut().push(request.url);
            }
            Ok(())
        }

        fn listen(&self, _: &str, _: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[component]
    fn BrowserMenu() -> Element {
        let host = use_context::<Rc<Host>>();
        let url = use_signal(|| "https://example.test/first".to_string());
        use_hook(move || {
            *host.url.borrow_mut() = Some(url);
        });
        rsx! {
            ContextMenu { attributes: vec![],
                ContextMenuTrigger { attributes: vec![], "Provider page" }
                ContextMenuContent { attributes: vec![],
                    ExternalBrowserMenuItem { url: url(), index: 0usize }
                }
            }
        }
    }

    #[test]
    fn right_click_can_open_the_current_provider_url_in_the_external_browser() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let host = Rc::new(Host::default());
        let mut dom = VirtualDom::new(BrowserMenu);
        dom.provide_root_context(HostBinding(host.clone()));
        dom.provide_root_context(host.clone());
        let initial = dom.rebuild_to_vec();
        let trigger = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "contextmenu" => Some(*id),
                _ => None,
            })
            .unwrap();
        dom.runtime().handle_event(
            "contextmenu",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            trigger,
        );
        let opened = dom.render_immediate_to_vec();
        assert!(dioxus_ssr::render(&dom).contains(&translate("browser-open-external")));
        let item = opened
            .edits
            .iter()
            .rev()
            .find_map(|edit| match edit {
                Mutation::NewEventListener { name, id } if name == "keydown" => Some(*id),
                _ => None,
            })
            .unwrap();
        let updated = "https://example.test/auth?state=%2B%2F&challenge=a+b";
        host.url.borrow().unwrap().set(updated.to_string());
        dom.render_immediate_to_vec();
        let keyboard = SerializedKeyboardData::new(
            Key::Enter,
            Code::Enter,
            Location::Standard,
            false,
            Modifiers::empty(),
            false,
        );
        dom.runtime().handle_event(
            "keydown",
            Event::new(EventData::Keyboard(keyboard).into_any(), true),
            item,
        );
        dom.render_immediate_to_vec();
        assert_eq!(*host.sent.borrow(), [updated]);
    }
}
