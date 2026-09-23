use vmux_core::event::{FileUiStateEvent, FileUiStatePatch};
use vmux_ui::hooks::{UiStatePatch, UiStatePatchBatch, use_ui_state_patch};

pub(crate) fn use_file_ui<T>() -> UiStatePatchBatch<FileUiStateEvent, T>
where
    FileUiStatePatch: UiStatePatch<T>,
    T: Clone + 'static,
{
    use_ui_state_patch::<FileUiStateEvent, T>()
}
