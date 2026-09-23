use dioxus::prelude::{ReadSignal, Signal};
use vmux_core::event::{FileUiStateEvent, FileUiStatePatch};
use vmux_ui::hooks::{UiStatePatch, use_ui_state_patch, use_ui_state_root};

pub(crate) fn use_file_ui_root() -> Signal<FileUiStateEvent> {
    use_ui_state_root::<FileUiStateEvent>()
}

pub(crate) fn use_file_ui<T>() -> ReadSignal<Option<T>>
where
    FileUiStatePatch: UiStatePatch<T>,
    T: Clone + 'static,
{
    use_ui_state_patch::<FileUiStateEvent, T>()
}
