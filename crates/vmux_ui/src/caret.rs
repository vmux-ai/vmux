#[derive(Clone, Copy)]
pub struct TextCaret {
    element_id: &'static str,
}

impl TextCaret {
    pub fn in_field(element_id: &'static str) -> Self {
        Self { element_id }
    }
}

pub struct EventSelection;

impl EventSelection {
    pub fn caret_in(element_id: &str) -> usize {
        Self::in_field(element_id).0
    }
}

impl EventSelection {
    pub fn in_field(element_id: &str) -> (usize, usize) {
        crate::transport::Host::event_field_selection(element_id)
    }

    pub fn in_document() -> bool {
        crate::transport::Host::event_document_has_selection()
    }
}

impl TextCaret {
    pub fn place(self, byte: usize) {
        crate::transport::Host::place_caret(self.element_id, byte);
    }

    pub fn select_all(self) {
        crate::transport::Host::select_element_text(self.element_id);
    }

    pub fn clear(self) {
        crate::transport::Host::clear_element_text(self.element_id);
    }

    pub fn select_all_from_start_next_frame(self) {
        crate::transport::Host::offer_element_text(self.element_id);
    }

    pub fn to_end(self) {
        crate::transport::Host::caret_to_end(self.element_id);
    }
}

pub fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn utf16_offset_to_byte(s: &str, utf16_offset: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in s.char_indices() {
        if units >= utf16_offset {
            return byte;
        }
        units += ch.len_utf16() as u32;
    }
    s.len()
}

pub fn byte_offset_to_utf16(s: &str, byte_offset: usize) -> u32 {
    let mut units = 0u32;
    for (byte, ch) in s.char_indices() {
        if byte >= byte_offset {
            return units;
        }
        units += ch.len_utf16() as u32;
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offset_maps_to_bytes_for_ascii() {
        assert_eq!(utf16_offset_to_byte("hello", 0), 0);
        assert_eq!(utf16_offset_to_byte("hello", 3), 3);
        assert_eq!(utf16_offset_to_byte("hello", 5), 5);
    }

    #[test]
    fn utf16_offset_maps_to_bytes_across_multibyte_chars() {
        let s = "aé本b";
        assert_eq!(utf16_offset_to_byte(s, 0), 0);
        assert_eq!(utf16_offset_to_byte(s, 1), 1);
        assert_eq!(utf16_offset_to_byte(s, 2), 3);
        assert_eq!(utf16_offset_to_byte(s, 3), 6);
        assert_eq!(utf16_offset_to_byte(s, 4), 7);
    }

    #[test]
    fn utf16_offset_handles_surrogate_pairs_and_overflow() {
        let s = "x😀y";
        assert_eq!(utf16_offset_to_byte(s, 1), 1);
        assert_eq!(utf16_offset_to_byte(s, 3), 5);
        assert_eq!(utf16_offset_to_byte(s, 99), s.len());
    }

    #[test]
    fn byte_and_utf16_offsets_round_trip() {
        for s in ["hello", "aé本b", "x😀y", ""] {
            for (byte, _) in s.char_indices().chain([(s.len(), ' ')]) {
                let units = byte_offset_to_utf16(s, byte);
                assert_eq!(utf16_offset_to_byte(s, units), byte, "{s:?} at byte {byte}");
            }
        }
        assert_eq!(byte_offset_to_utf16("x😀y", 5), 3);
        assert_eq!(byte_offset_to_utf16("x😀y", 99), 4);
    }

    #[test]
    fn a_byte_offset_inside_a_character_falls_back_to_its_start() {
        assert_eq!(floor_char_boundary("aé本b", 4), 3);
        assert_eq!(floor_char_boundary("aé本b", 3), 3);
        assert_eq!(floor_char_boundary("aé本b", 99), 7);
        assert_eq!(floor_char_boundary("", 5), 0);
    }
}
