use dioxus::prelude::Signal;
use vmux_core::event::{FileUiStateEvent, FileUiStatePatch};
use vmux_ui::hooks::{UiStatePatch, use_ui_state_patch, use_ui_state_root};

pub(crate) fn use_file_ui_state_root() -> Signal<FileUiStateEvent> {
    use_ui_state_root::<FileUiStateEvent>()
}

pub(crate) fn use_file_ui_state<T, F>(on_event: F)
where
    FileUiStatePatch: UiStatePatch<T>,
    T: Clone + 'static,
    F: FnMut(T) + 'static,
{
    use_ui_state_patch::<FileUiStateEvent, T, F>(on_event);
}
