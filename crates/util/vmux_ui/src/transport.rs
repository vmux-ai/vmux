use std::cell::RefCell;
use std::rc::Rc;

use crate::transport::event_listener::EventListenerError;

pub trait PageHost {
    fn publish_address(&self, _slot: u64) {}

    fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError>;

    fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError>;

    fn focus_element(&self, _element_id: &str) {}

    fn show_element_popover(&self, _element_id: &str) {}

    fn show_dropdown_popover(&self, element_id: &str) {
        self.show_element_popover(element_id);
    }

    fn scroll_element_into_view(&self, _element_id: &str) {}

    fn scroll_element_into_view_vertical(&self, _element_id: &str) {}

    fn reveal_first_rendered(&self, _element_ids: &[&str], _centered: bool) {}

    fn scroll_element_to(&self, _element_id: &str, _top: f64) {}

    fn scroll_element_to_smooth(&self, element_id: &str, top: f64) {
        self.scroll_element_to(element_id, top);
    }

    fn section_at_top(&self, _container: &str) -> TextOffsetAnswer {
        Box::pin(std::future::ready(None))
    }

    fn scroll_section(
        &self,
        _container: &str,
        _element: &str,
        _behavior: dioxus::html::ScrollBehavior,
    ) {
    }

    fn animate_height(&self, _element_id: &str, _duration_ms: u32) {}
    fn animate_layout(&self, _element_id: &str, _duration_ms: u32) {}

    fn preserve_scroll(&self) {}

    fn text_offset_at(&self, _element_id: &str, _x: f64, _y: f64) -> TextOffsetAnswer {
        Box::pin(std::future::ready(None))
    }

    fn select_element_text(&self, _element_id: &str) {}

    fn toggle_media(&self, _element_id: &str) {}

    fn offer_element_text(&self, _element_id: &str) {}

    fn place_caret(&self, _element_id: &str, _byte: usize) {}

    fn caret_to_end(&self, _element_id: &str) {}

    fn run_script(&self, _script: String) {}

    fn event_field_selection(&self, _element_id: &str) -> (usize, usize) {
        (0, 0)
    }

    fn event_document_has_selection(&self) -> bool {
        false
    }

    fn write_to_clipboard(&self, _text: &str) {}

    fn resolves_keys(&self) -> bool {
        false
    }
}

pub type BytesListener = Box<dyn FnMut(&[u8]) -> bool>;

pub type TextOffsetAnswer = std::pin::Pin<Box<dyn std::future::Future<Output = Option<u32>>>>;

pub fn install_host(host: Rc<dyn PageHost>) {
    HOST.with(|slot| *slot.borrow_mut() = Some(host));
}

pub struct HostScope(Option<Rc<dyn PageHost>>);

#[derive(Clone)]
pub struct HostBinding(pub Rc<dyn PageHost>);

impl PartialEq for HostBinding {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl HostBinding {
    pub fn current() -> Option<Self> {
        if dioxus::core::Runtime::try_current().is_some()
            && let Some(binding) = dioxus::prelude::try_consume_context::<Self>()
        {
            return Some(binding);
        }
        HOST.with(|slot| slot.borrow().clone().map(Self))
    }
}

impl HostScope {
    pub fn binding() -> Option<HostBinding> {
        HOST.with(|slot| slot.borrow().clone().map(HostBinding))
    }

    pub fn enter(host: Rc<dyn PageHost>) -> Self {
        Self(HOST.with(|slot| slot.borrow_mut().replace(host)))
    }
}

impl Drop for HostScope {
    fn drop(&mut self) {
        HOST.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

pub(crate) struct Host;

impl Host {
    pub(crate) fn section_at_top(container: &str) -> TextOffsetAnswer {
        Self::with_installed(|host| host.section_at_top(container))
            .unwrap_or_else(|_| Box::pin(std::future::ready(None)))
    }

    pub(crate) fn scroll_section(
        container: &str,
        element: &str,
        behavior: dioxus::html::ScrollBehavior,
    ) {
        let _ = Self::with_installed(|host| host.scroll_section(container, element, behavior));
    }

    pub(crate) fn emit(id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
        Self::with_installed(|host| host.send(id, bytes))?
    }

    pub(crate) fn listen(id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
        Self::with_installed(|host| host.listen(id, on_bytes))?
    }

    pub(crate) fn focus_element(id: &str) {
        let _ = Self::with_installed(|host| host.focus_element(id));
    }

    pub(crate) fn show_element_popover(id: &str) {
        let _ = Self::with_installed(|host| host.show_element_popover(id));
    }

    pub(crate) fn show_dropdown_popover(id: &str) {
        let _ = Self::with_installed(|host| host.show_dropdown_popover(id));
    }

    pub(crate) fn select_element_text(id: &str) {
        let _ = Self::with_installed(|host| host.select_element_text(id));
    }

    pub(crate) fn offer_element_text(id: &str) {
        let _ = Self::with_installed(|host| host.offer_element_text(id));
    }

    pub(crate) fn write_to_clipboard(text: &str) {
        let _ = Self::with_installed(|host| host.write_to_clipboard(text));
    }

    pub(crate) fn event_field_selection(id: &str) -> (usize, usize) {
        Self::with_installed(|host| host.event_field_selection(id)).unwrap_or((0, 0))
    }

    pub(crate) fn event_document_has_selection() -> bool {
        Self::with_installed(|host| host.event_document_has_selection()).unwrap_or(false)
    }

    #[cfg(ui)]
    pub(crate) fn resolves_keys() -> bool {
        Self::with_installed(|host| host.resolves_keys()).unwrap_or(false)
    }

    pub(crate) fn place_caret(id: &str, byte: usize) {
        let _ = Self::with_installed(|host| host.place_caret(id, byte));
    }

    pub(crate) fn caret_to_end(id: &str) {
        let _ = Self::with_installed(|host| host.caret_to_end(id));
    }

    pub(crate) fn run_script(script: String) {
        let _ = Self::with_installed(|host| host.run_script(script));
    }

    #[cfg(ui)]
    pub(crate) fn toggle_media(id: &str) {
        let _ = Self::with_installed(|host| host.toggle_media(id));
    }

    pub(crate) fn scroll_element_to(element_id: &str, top: f64) {
        let _ = Self::with_installed(|host| host.scroll_element_to(element_id, top));
    }

    pub(crate) fn scroll_element_to_smooth(element_id: &str, top: f64) {
        let _ = Self::with_installed(|host| host.scroll_element_to_smooth(element_id, top));
    }

    pub(crate) fn animate_height(element_id: &str, duration_ms: u32) {
        let _ = Self::with_installed(|host| host.animate_height(element_id, duration_ms));
    }

    pub(crate) fn animate_layout(element_id: &str, duration_ms: u32) {
        let _ = Self::with_installed(|host| host.animate_layout(element_id, duration_ms));
    }

    pub(crate) fn preserve_scroll() {
        let _ = Self::with_installed(|host| host.preserve_scroll());
    }

    pub(crate) fn reveal_first_rendered(element_ids: &[&str], centered: bool) {
        let _ = Self::with_installed(|host| host.reveal_first_rendered(element_ids, centered));
    }

    #[cfg(ui)]
    pub(crate) fn text_offset_at(id: &str, x: f64, y: f64) -> TextOffsetAnswer {
        match Self::with_installed(|host| host.text_offset_at(id, x, y)) {
            Ok(answer) => answer,
            Err(_) => Box::pin(std::future::ready(None)),
        }
    }

    fn with_installed<R>(f: impl FnOnce(&dyn PageHost) -> R) -> Result<R, EventListenerError> {
        if let Some(binding) = HostBinding::current() {
            return Ok(f(binding.0.as_ref()));
        }
        let Some(fallback) = Self::fallback() else {
            return Err(EventListenerError::NoHost);
        };
        Ok(f(fallback))
    }
}

pub struct HostPayload<'a>(&'a [u8]);

impl<'a> HostPayload<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    pub fn decode<T>(&self) -> Option<T>
    where
        T: rkyv::Archive,
        T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
            + for<'b> rkyv::bytecheck::CheckBytes<
                rkyv::api::high::HighValidator<'b, rkyv::rancor::Error>,
            >,
    {
        rkyv::from_bytes::<T, rkyv::rancor::Error>(self.0).ok()
    }
}

thread_local! {
    static HOST: RefCell<Option<Rc<dyn PageHost>>> = const { RefCell::new(None) };
}

pub mod bin_ipc_envelope;
pub mod event_listener;
mod native;
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
    struct Ping {
        value: u32,
    }

    #[vmux_api::ui_state(Default, Eq)]
    struct SnapshotState {
        value: u32,
    }

    #[derive(Default)]
    struct LoopbackHost {
        listeners: RefCell<Vec<(String, BytesListener)>>,
    }

    impl PageHost for LoopbackHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            self.listeners
                .borrow_mut()
                .retain_mut(|(registered, on_bytes)| registered != id || on_bytes(bytes));
            Ok(())
        }

        fn listen(&self, id: &str, on_bytes: BytesListener) -> Result<(), EventListenerError> {
            self.listeners.borrow_mut().push((id.to_string(), on_bytes));
            Ok(())
        }
    }

    #[test]
    fn a_payload_reaches_the_listener_registered_for_its_id() {
        install_host(Rc::new(LoopbackHost::default()));

        let seen = Rc::new(RefCell::new(Vec::<Ping>::new()));
        let sink = seen.clone();
        Host::listen(
            "ping",
            Box::new(move |bytes| {
                if let Some(ping) = HostPayload::new(bytes).decode::<Ping>() {
                    sink.borrow_mut().push(ping);
                }
                true
            }),
        )
        .unwrap();

        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&Ping { value: 7 }).unwrap();
        Host::emit("ping", &bytes).unwrap();
        Host::emit("other", &bytes).unwrap();

        assert_eq!(*seen.borrow(), vec![Ping { value: 7 }]);
    }

    #[test]
    fn an_inactive_listener_is_removed_after_delivery() {
        let host = Rc::new(LoopbackHost::default());
        install_host(host.clone());
        Host::listen("once", Box::new(|_| false)).unwrap();

        Host::emit("once", &[]).unwrap();
        Host::emit("once", &[]).unwrap();

        assert!(host.listeners.borrow().is_empty());
    }

    #[test]
    fn mobile_state_emit_reaches_the_shared_state_listener() {
        let host = Rc::new(LoopbackHost::default());
        install_host(host.clone());
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        Host::listen_state::<SnapshotState, _>(move |frame| {
            sink.borrow_mut().push(frame);
            true
        })
        .unwrap();
        let emit = vmux_api::page::UiStateEmit::from_state(&SnapshotState { value: 7 }).unwrap();

        host.send(&emit.id, &emit.bytes).unwrap();

        assert_eq!(
            *seen.borrow(),
            vec![vmux_api::UiStateFrame::Snapshot(SnapshotState { value: 7 })]
        );
    }

    #[derive(Default)]
    struct CountingHost {
        sent: RefCell<usize>,
    }

    impl PageHost for CountingHost {
        fn send(&self, _id: &str, _bytes: &[u8]) -> Result<(), EventListenerError> {
            *self.sent.borrow_mut() += 1;
            Ok(())
        }

        fn listen(&self, _id: &str, _on_bytes: BytesListener) -> Result<(), EventListenerError> {
            Ok(())
        }
    }

    #[test]
    fn a_page_addresses_its_own_host_even_while_another_is_mounted() {
        let first = Rc::new(CountingHost::default());
        let second = Rc::new(CountingHost::default());

        let outer = HostScope::enter(first.clone());
        Host::emit("a", &[]).unwrap();
        {
            let _inner = HostScope::enter(second.clone());
            Host::emit("b", &[]).unwrap();
            Host::emit("c", &[]).unwrap();
        }
        Host::emit("d", &[]).unwrap();
        drop(outer);

        assert_eq!(*first.sent.borrow(), 2, "the outer page sent 'a' and 'd'");
        assert_eq!(*second.sent.borrow(), 2, "the inner page sent 'b' and 'c'");
        assert!(
            matches!(Host::emit("e", &[]), Err(EventListenerError::NoHost)),
            "leaving the last scope leaves no host installed, rather than the first one"
        );
    }
}
