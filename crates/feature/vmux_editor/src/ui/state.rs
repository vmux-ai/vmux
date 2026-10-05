use dioxus::prelude::*;
use vmux_ecs::event::FileUiState;
use vmux_ui::hooks::use_ui_state;

#[derive(Clone, Copy)]
pub(crate) struct FileUi {
    state: Signal<FileUiState>,
}

impl FileUi {
    pub(crate) fn use_root() -> Self {
        let state = use_ui_state::<FileUiState>().state;
        let ui = Self { state };
        use_context_provider(|| ui);
        ui
    }

    pub(crate) fn current() -> Self {
        use_context::<Self>()
    }

    pub(crate) fn use_field<T>(self, select: fn(&FileUiState) -> Option<&T>) -> FileUiField<T>
    where
        T: Clone + PartialEq + 'static,
    {
        FileUiField {
            value: self.use_value(select),
            handled: use_signal(|| None),
        }
    }

    pub(crate) fn use_value<T>(self, select: fn(&FileUiState) -> Option<&T>) -> Memo<Option<T>>
    where
        T: Clone + PartialEq + 'static,
    {
        use_memo(move || select(&self.state.read()).cloned())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FileUiField<T: 'static> {
    value: Memo<Option<T>>,
    handled: Signal<Option<T>>,
}

impl<T> FileUiField<T>
where
    T: Clone + PartialEq + 'static,
{
    pub(crate) fn for_each(&self, mut apply: impl FnMut(T)) {
        let value = self.value.read().clone();
        if value == *self.handled.peek() {
            return;
        }
        let mut handled = self.handled;
        handled.set(value.clone());
        if let Some(value) = value {
            apply(value);
        }
    }
}
