use vmux_ecs::event::{FileUiState, FileUiStatePatch};
use vmux_ui::hooks::{UiStateBinding, UiStatePatch, UiStatePatchBatch};

pub(crate) fn use_file_ui<T>() -> UiStatePatchBatch<FileUiState, T>
where
    FileUiStatePatch: UiStatePatch<T>,
    T: Clone + 'static,
{
    dioxus::prelude::use_context::<UiStateBinding<FileUiState>>().use_patch::<T>()
}
