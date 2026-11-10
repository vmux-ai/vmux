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
