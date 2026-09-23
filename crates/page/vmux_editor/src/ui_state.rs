use dioxus::prelude::*;
use vmux_core::event::{FileUiStateEvent, FileUiStatePayload};
use vmux_ui::hooks::use_ui_state;

pub(crate) fn use_file_ui_state_root() -> Signal<FileUiStateEvent> {
    let state = use_ui_state::<FileUiStateEvent>();
    use_context_provider(|| state);
    state
}

pub(crate) fn use_file_ui_state<T, F>(mut on_event: F)
where
    T: FileUiStatePayload,
    F: FnMut(T) + 'static,
{
    let state = use_context::<Signal<FileUiStateEvent>>();
    let mut handled_sequence = use_signal(|| 0u64);
    use_effect(move || {
        let event = state();
        if event.sequence == 0 || event.sequence == *handled_sequence.peek() {
            return;
        }
        handled_sequence.set(event.sequence);
        for patch in &event.patches {
            if let Some(payload) = T::from_patch(patch) {
                on_event(payload.clone());
            }
        }
    });
}
