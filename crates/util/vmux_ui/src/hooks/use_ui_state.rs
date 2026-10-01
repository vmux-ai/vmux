use crate::listener_guard::GuardedListener;
use crate::transport::Host;
use crate::transport::event_listener::{EventListenerError, listen_ui_state, try_emit_page_ready};
use dioxus::core::{Runtime, current_scope_id};
use dioxus::prelude::*;
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use vmux_api::{BatchedUiState, UiState, UiStatePatch};

struct UiStateListener {
    error: Signal<Option<String>>,
}

#[derive(Clone)]
struct PageReadyAnnouncement(Rc<Cell<bool>>);

impl PageReadyAnnouncement {
    fn new() -> Self {
        use_root_context(|| Self(Rc::new(Cell::new(false))))
    }

    fn announce(&self) -> Result<(), EventListenerError> {
        if self.0.get() {
            return Ok(());
        }
        try_emit_page_ready()?;
        self.0.set(true);
        Ok(())
    }
}

pub struct UiStatePatchBatch<S, T> {
    state: Signal<S>,
    handled_sequence: Signal<u64>,
    payload: PhantomData<fn() -> T>,
}

pub struct UiStateBinding<T> {
    pub state: Signal<T>,
    pub error: Signal<Option<String>>,
}

pub struct UiStateValue<T> {
    pub value: Signal<T>,
    pub ready: Signal<bool>,
}

fn use_ui_state_listener<T, F>(on_state: F) -> UiStateListener
where
    T: UiState + rkyv::Archive + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
    F: FnMut(T) + 'static,
{
    let listener = use_hook(|| GuardedListener::new(on_state));
    let listener_guard = listener.guard();
    use_drop(move || listener_guard.deactivate());
    let mut error = use_signal(|| None::<String>);
    let mut listening = use_signal(|| false);
    let retry_tick = use_signal(|| 0u32);
    let announcement = PageReadyAnnouncement::new();

    use_effect(move || {
        let current_retry = retry_tick();
        if listening() {
            return;
        }
        let announcement = announcement.clone();
        let listener = listener.clone();
        let Some(runtime) = Runtime::try_current() else {
            error.set(Some("use_ui_state: no Dioxus runtime".into()));
            return;
        };
        let scope = current_scope_id();
        match listen_ui_state::<T, _>(move |state| {
            let listener = listener.clone();
            runtime.in_scope(scope, || listener.call(state));
        }) {
            Ok(()) => {
                listening.set(true);
                error.set(None);
                if let Err(cause) = announcement.announce() {
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

impl<T> Clone for UiStateValue<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for UiStateValue<T> {}

impl<T: 'static> PartialEq for UiStateValue<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.ready == other.ready
    }
}

impl<S, T> Clone for UiStatePatchBatch<S, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, T> Copy for UiStatePatchBatch<S, T> {}

impl<S, T> UiStatePatchBatch<S, T>
where
    S: BatchedUiState,
    S::Patch: UiStatePatch<T>,
    T: Clone + 'static,
{
    pub fn take(mut self) -> Vec<T> {
        let (sequence, payloads) = {
            let event = self.state.read();
            let sequence = event.sequence();
            if sequence == 0 || sequence == *self.handled_sequence.peek() {
                return Vec::new();
            }
            let payloads = event
                .patches()
                .iter()
                .filter_map(<S::Patch as UiStatePatch<T>>::payload)
                .cloned()
                .collect();
            (sequence, payloads)
        };
        self.handled_sequence.set(sequence);
        payloads
    }

    pub fn for_each(self, mut callback: impl FnMut(T)) {
        for payload in self.take() {
            callback(payload);
        }
    }
}

impl<S> UiStateBinding<S>
where
    S: BatchedUiState,
{
    pub fn use_patch<T>(self) -> UiStatePatchBatch<S, T>
    where
        S::Patch: UiStatePatch<T>,
        T: Clone + 'static,
    {
        UiStatePatchBatch {
            state: self.state,
            handled_sequence: use_signal(|| 0),
            payload: PhantomData,
        }
    }

    pub fn use_value<T>(self) -> UiStateValue<T>
    where
        S::Patch: UiStatePatch<T>,
        T: Clone + Default + PartialEq + 'static,
    {
        let patches = self.use_patch::<T>();
        let mut value = use_signal(T::default);
        let mut ready = use_signal(|| false);
        use_effect(move || {
            for next in patches.take() {
                if value.peek().ne(&next) {
                    value.set(next);
                }
                if !*ready.peek() {
                    ready.set(true);
                }
            }
        });
        UiStateValue { value, ready }
    }

    pub fn use_updates<T>(self, mut apply: impl FnMut(T) + 'static)
    where
        S::Patch: UiStatePatch<T>,
        T: Clone + 'static,
    {
        let patches = self.use_patch::<T>();
        use_effect(move || patches.for_each(&mut apply));
    }

    pub fn use_patches(self, mut apply: impl FnMut(&S::Patch) + 'static) -> Signal<Option<String>> {
        let mut handled_sequence = use_signal(|| 0);
        use_effect(move || {
            let event = self.state.read();
            let sequence = event.sequence();
            if sequence == 0 || sequence == *handled_sequence.peek() {
                return;
            }
            handled_sequence.set(sequence);
            for patch in event.patches() {
                apply(patch);
            }
        });
        self.error
    }

    pub fn use_projection<T>(
        self,
        mut apply: impl FnMut(&mut T, &S::Patch) + 'static,
    ) -> UiStateBinding<T>
    where
        T: Default + 'static,
    {
        let mut state = use_signal(T::default);
        let error = self.use_patches(move |patch| {
            state.with_mut(|state| apply(state, patch));
        });
        UiStateBinding { state, error }
    }
}

pub fn use_ui_state<T>() -> Signal<T>
where
    T: UiState + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(T::default);
    let _listener = use_ui_state_listener::<T, _>(move |event| state.set(event));
    state
}

pub fn use_ui_state_binding<T>() -> UiStateBinding<T>
where
    T: BatchedUiState + rkyv::Archive + Default + 'static,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>
        + for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    let mut state = use_signal(T::default);
    let listener = use_ui_state_listener::<T, _>(move |event| state.set(event));
    let binding = UiStateBinding {
        state,
        error: listener.error,
    };
    use_context_provider(|| binding);
    binding
}
