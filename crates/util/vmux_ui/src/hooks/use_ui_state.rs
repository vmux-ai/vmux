use crate::listener_guard::GuardedListener;
use crate::transport::Host;
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use vmux_api::{UiState, UiStateFrame};

#[derive(Clone, Default)]
pub struct UiStateSnapshots(Rc<RefCell<SnapshotValues>>);

#[derive(Default)]
struct SnapshotValues {
    values: HashMap<TypeId, Box<dyn Any>>,
    readers: HashMap<TypeId, SnapshotReader>,
}

type SnapshotReader = Box<dyn Fn() -> Option<Box<dyn Any>>>;

impl UiStateSnapshots {
    pub fn capture(&self) {
        let readers = std::mem::take(&mut self.0.borrow_mut().readers);
        let mut values = HashMap::new();
        for (id, read) in readers {
            if let Some(value) = read() {
                values.insert(id, value);
            }
        }
        self.0.borrow_mut().values = values;
    }

    fn initial<T: UiState + Default>(&self) -> T {
        let snapshots = self.0.borrow();
        if let Some(read) = snapshots.readers.get(&TypeId::of::<T>())
            && let Some(value) = read()
            && let Ok(value) = value.downcast::<T>()
        {
            return *value;
        }
        match snapshots
            .values
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
        {
            Some(value) => value.clone(),
            None => T::default(),
        }
    }

    pub fn finish_restore(&self) {
        self.0.borrow_mut().values.clear();
    }

    fn track<T: UiState>(&self, state: Signal<T>) {
        self.0.borrow_mut().readers.insert(
            TypeId::of::<T>(),
            Box::new(move || {
                let value = state.try_peek().ok()?.retained()?;
                Some(Box::new(value) as Box<dyn Any>)
            }),
        );
    }
}

struct UiStateListener {
    error: Signal<Option<String>>,
}

pub struct UiStateBinding<T> {
    pub state: Signal<T>,
    pub error: Signal<Option<String>>,
}

impl<T> UiStateBinding<T>
where
    T: 'static,
{
    pub fn use_context() -> Self {
        use_context::<Self>()
    }

    pub fn use_selector<S>(self, select: impl Fn(&T) -> S + 'static) -> Memo<S>
    where
        S: Clone + PartialEq + 'static,
    {
        use_memo(move || select(&self.state.read()))
    }
}

fn use_ui_state_listener<T, F>(on_state: F) -> UiStateListener
where
    T: UiState + rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    UiStateFrame<T, T::Update>: rkyv::Archive,
    <UiStateFrame<T, T::Update> as rkyv::Archive>::Archived: rkyv::Deserialize<
            UiStateFrame<T, T::Update>,
            rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
        > + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    F: FnMut(UiStateFrame<T, T::Update>) + 'static,
{
    let listener = use_hook(|| GuardedListener::new(on_state));
    let listener_guard = listener.guard();
    use_drop(move || listener_guard.deactivate());
    let mut error = use_signal(|| None::<String>);
    let mut listening = use_signal(|| false);
    let retry_tick = use_signal(|| 0u32);

    use_effect(move || {
        let current_retry = retry_tick();
        if listening() {
            return;
        }
        let listener = listener.clone();
        let Some(runtime) = Runtime::try_current() else {
            error.set(Some("use_ui_state: no Dioxus runtime".into()));
            return;
        };
        let scope = current_scope_id();
        match Host::listen_state::<T, _>(move |state| {
            let listener = listener.clone();
            runtime.in_scope(scope, || listener.call(state))
        }) {
            Ok(()) => {
                listening.set(true);
                error.set(None);
                if let Err(cause) = Host::announce_ready() {
                    error.set(Some(format!("page ready emit failed: {cause}")));
                }
            }
            Err(cause) => {
                error.set(Some(format!("host listen failed: {cause}")));
                Host::schedule_listener_retry(retry_tick, current_retry);
            }
        }
    });

    UiStateListener { error }
}

impl<T> Clone for UiStateBinding<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for UiStateBinding<T> {}

pub fn use_ui_state<T>() -> UiStateBinding<T>
where
    T: UiState + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    UiStateFrame<T, T::Update>: rkyv::Archive,
    <UiStateFrame<T, T::Update> as rkyv::Archive>::Archived: rkyv::Deserialize<
            UiStateFrame<T, T::Update>,
            rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
        > + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let inherited = use_hook(try_consume_context::<UiStateBinding<T>>);
    if let Some(binding) = inherited {
        return binding;
    }
    let snapshots = try_consume_context::<UiStateSnapshots>();
    let initial = snapshots.clone();
    let mut state = use_signal(move || match initial {
        Some(snapshots) => snapshots.initial::<T>(),
        None => T::default(),
    });
    use_hook(move || {
        if let Some(snapshots) = snapshots {
            snapshots.track(state);
        }
    });
    let listener = use_ui_state_listener::<T, _>(move |frame| match frame {
        UiStateFrame::Snapshot(snapshot) => state.set(snapshot),
        UiStateFrame::Updates(updates) => {
            state.write().apply_updates(updates);
        }
    });
    let binding = UiStateBinding {
        state,
        error: listener.error,
    };
    use_context_provider(|| binding);
    binding
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::task::{Context, Waker};

    use super::*;
    use crate::hooks::EventListenerError;
    use crate::transport::{BytesListener, HostBinding, HostScope, PageHost};
    use vmux_api::BinEvent;

    #[vmux_api::ui_state(Default, Eq)]
    struct SharedState {
        value: u32,
    }

    #[derive(Default)]
    struct PreviewHost {
        listeners: RefCell<Vec<BytesListener>>,
    }

    impl PageHost for PreviewHost {
        fn send(&self, _: &str, _: &[u8]) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            if id == SharedState::id() {
                self.listeners.borrow_mut().push(listener);
            }
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct Visibility(Rc<RefCell<Option<Signal<bool>>>>);

    struct Preview {
        host: Rc<PreviewHost>,
        dom: VirtualDom,
        visible: Visibility,
        _scope: HostScope,
    }

    impl Preview {
        fn new() -> Self {
            let host = Rc::new(PreviewHost::default());
            let scope = HostScope::enter(host.clone());
            let visible = Visibility::default();
            let mut dom = VirtualDom::new(Root);
            dom.provide_root_context(HostBinding(host.clone()));
            dom.provide_root_context(visible.clone());
            dom.rebuild_to_vec();
            let mut preview = Self {
                host,
                dom,
                visible,
                _scope: scope,
            };
            preview.render();
            preview
        }

        fn publish(&mut self, value: u32) -> String {
            let frame = UiStateFrame::<SharedState, <SharedState as UiState>::Update>::Snapshot(
                SharedState { value },
            );
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            for listener in self.host.listeners.borrow_mut().iter_mut() {
                listener(&bytes);
            }
            self.render()
        }

        fn show(&mut self, visible: bool) -> String {
            self.visible.0.borrow().unwrap().set(visible);
            self.render()
        }

        fn render(&mut self) -> String {
            for _ in 0..4 {
                let mut context = Context::from_waker(Waker::noop());
                let _ = std::pin::pin!(self.dom.wait_for_work()).poll(&mut context);
                self.dom.render_immediate_to_vec();
            }
            dioxus_ssr::render(&self.dom)
        }
    }

    #[component]
    fn Root() -> Element {
        use_ui_state::<SharedState>();
        let visible = use_signal(|| true);
        let control = use_context::<Visibility>();
        use_hook(move || *control.0.borrow_mut() = Some(visible));
        rsx! { div { if visible() { Child {} } } }
    }

    #[component]
    fn Child() -> Element {
        let binding = use_ui_state::<SharedState>();
        let value = binding.use_selector(|state| state.value);
        rsx! { span { "value={value}" } }
    }

    #[test]
    fn child_replacement_reuses_one_listener_and_receives_the_latest_root_snapshot() {
        let mut preview = Preview::new();
        assert_eq!(preview.host.listeners.borrow().len(), 1);
        assert!(preview.publish(7).contains("value=7"));
        assert!(!preview.show(false).contains("value="));
        preview.publish(9);
        assert!(preview.show(true).contains("value=9"));
        assert_eq!(preview.host.listeners.borrow().len(), 1);
    }
}
