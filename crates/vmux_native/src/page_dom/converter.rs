use dioxus_html::*;

/// Builds the backing for one mounted element, as many times as the converter is asked for it.
///
/// `HtmlEventConverter::convert_mounted_data` returns a `MountedData` by value and is handed only
/// a shared reference, so the backing cannot simply be moved out of the event — it has to be
/// makeable on demand.
pub(crate) struct MountedBacking(Box<dyn Fn() -> MountedData>);

impl MountedBacking {
    pub(crate) fn of(backing: impl RenderedElementBacking + Clone + 'static) -> Self {
        Self(Box::new(move || MountedData::new(backing.clone())))
    }
}

/// Carries a live element through to `onmounted`, and hands everything else to the serialized path.
///
/// `SerializedHtmlEventConverter::convert_mounted_data` answers `MountedData::from(())`, whose
/// every method is `NotSupported` — it assumes an event that crossed a wire cannot name an element
/// still in the document. Here the event and the element arrive together, so the backing built for
/// it survives, and a page can focus, measure and scroll itself.
pub(crate) struct LiveElements(SerializedHtmlEventConverter);

impl LiveElements {
    pub(crate) fn new() -> Self {
        Self(SerializedHtmlEventConverter)
    }
}

impl HtmlEventConverter for LiveElements {
    fn convert_mounted_data(&self, event: &PlatformEventData) -> MountedData {
        match event.downcast::<MountedBacking>() {
            Some(backing) => (backing.0)(),
            None => self.0.convert_mounted_data(event),
        }
    }

    fn convert_animation_data(&self, event: &PlatformEventData) -> AnimationData {
        self.0.convert_animation_data(event)
    }

    fn convert_cancel_data(&self, event: &PlatformEventData) -> CancelData {
        self.0.convert_cancel_data(event)
    }

    fn convert_clipboard_data(&self, event: &PlatformEventData) -> ClipboardData {
        self.0.convert_clipboard_data(event)
    }

    fn convert_composition_data(&self, event: &PlatformEventData) -> CompositionData {
        self.0.convert_composition_data(event)
    }

    fn convert_drag_data(&self, event: &PlatformEventData) -> DragData {
        self.0.convert_drag_data(event)
    }

    fn convert_focus_data(&self, event: &PlatformEventData) -> FocusData {
        self.0.convert_focus_data(event)
    }

    fn convert_form_data(&self, event: &PlatformEventData) -> FormData {
        self.0.convert_form_data(event)
    }

    fn convert_image_data(&self, event: &PlatformEventData) -> ImageData {
        self.0.convert_image_data(event)
    }

    fn convert_keyboard_data(&self, event: &PlatformEventData) -> KeyboardData {
        self.0.convert_keyboard_data(event)
    }

    fn convert_media_data(&self, event: &PlatformEventData) -> MediaData {
        self.0.convert_media_data(event)
    }

    fn convert_mouse_data(&self, event: &PlatformEventData) -> MouseData {
        self.0.convert_mouse_data(event)
    }

    fn convert_pointer_data(&self, event: &PlatformEventData) -> PointerData {
        self.0.convert_pointer_data(event)
    }

    fn convert_resize_data(&self, event: &PlatformEventData) -> ResizeData {
        self.0.convert_resize_data(event)
    }

    fn convert_scroll_data(&self, event: &PlatformEventData) -> ScrollData {
        self.0.convert_scroll_data(event)
    }

    fn convert_selection_data(&self, event: &PlatformEventData) -> SelectionData {
        self.0.convert_selection_data(event)
    }

    fn convert_toggle_data(&self, event: &PlatformEventData) -> ToggleData {
        self.0.convert_toggle_data(event)
    }

    fn convert_touch_data(&self, event: &PlatformEventData) -> TouchData {
        self.0.convert_touch_data(event)
    }

    fn convert_transition_data(&self, event: &PlatformEventData) -> TransitionData {
        self.0.convert_transition_data(event)
    }

    fn convert_visible_data(&self, event: &PlatformEventData) -> VisibleData {
        self.0.convert_visible_data(event)
    }

    fn convert_wheel_data(&self, event: &PlatformEventData) -> WheelData {
        self.0.convert_wheel_data(event)
    }
}
