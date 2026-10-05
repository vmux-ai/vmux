#[derive(Clone, Copy)]
pub struct DomCaret {
    element_id: &'static str,
}

impl DomCaret {
    pub fn in_field(element_id: &'static str) -> Self {
        Self { element_id }
    }
}

pub struct DomSelection;

impl DomSelection {
    pub fn caret_in(element_id: &str) -> usize {
        Self::in_field(element_id).0
    }
}

impl DomSelection {
    pub fn in_field(element_id: &str) -> (usize, usize) {
        crate::transport::Host::event_field_selection(element_id)
    }

    pub fn in_document() -> bool {
        crate::transport::Host::event_document_has_selection()
    }
}

impl DomCaret {
    pub fn place(self, byte: usize) {
        crate::transport::Host::place_caret(self.element_id, byte);
    }

    pub fn select_all(self) {
        crate::transport::Host::select_element_text(self.element_id);
    }

    pub fn select_all_from_start_next_frame(self) {
        crate::transport::Host::offer_element_text(self.element_id);
    }

    pub fn to_end(self) {
        crate::transport::Host::caret_to_end(self.element_id);
    }
}
