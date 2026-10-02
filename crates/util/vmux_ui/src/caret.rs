#[derive(Clone, Copy)]
pub struct TextCaret {
    element_id: &'static str,
}

impl TextCaret {
    pub fn in_field(element_id: &'static str) -> Self {
        Self { element_id }
    }

    pub fn floor_boundary(value: &str, mut byte: usize) -> usize {
        if byte >= value.len() {
            return value.len();
        }
        while byte > 0 && !value.is_char_boundary(byte) {
            byte -= 1;
        }
        byte
    }

    pub fn byte_from_utf16(value: &str, utf16_offset: u32) -> usize {
        let mut units = 0u32;
        for (byte, character) in value.char_indices() {
            if units >= utf16_offset {
                return byte;
            }
            units += character.len_utf16() as u32;
        }
        value.len()
    }

    pub fn utf16_from_byte(value: &str, byte_offset: usize) -> u32 {
        let mut units = 0u32;
        for (byte, character) in value.char_indices() {
            if byte >= byte_offset {
                return units;
            }
            units += character.len_utf16() as u32;
        }
        units
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

    pub fn select_all_from_start_next_frame(self) {
        crate::transport::Host::offer_element_text(self.element_id);
    }

    pub fn to_end(self) {
        crate::transport::Host::caret_to_end(self.element_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offset_maps_to_bytes_for_ascii() {
        assert_eq!(TextCaret::byte_from_utf16("hello", 0), 0);
        assert_eq!(TextCaret::byte_from_utf16("hello", 3), 3);
        assert_eq!(TextCaret::byte_from_utf16("hello", 5), 5);
    }

    #[test]
    fn utf16_offset_maps_to_bytes_across_multibyte_chars() {
        let s = "aé本b";
        assert_eq!(TextCaret::byte_from_utf16(s, 0), 0);
        assert_eq!(TextCaret::byte_from_utf16(s, 1), 1);
        assert_eq!(TextCaret::byte_from_utf16(s, 2), 3);
        assert_eq!(TextCaret::byte_from_utf16(s, 3), 6);
        assert_eq!(TextCaret::byte_from_utf16(s, 4), 7);
    }

    #[test]
    fn utf16_offset_handles_surrogate_pairs_and_overflow() {
        let s = "x😀y";
        assert_eq!(TextCaret::byte_from_utf16(s, 1), 1);
        assert_eq!(TextCaret::byte_from_utf16(s, 3), 5);
        assert_eq!(TextCaret::byte_from_utf16(s, 99), s.len());
    }

    #[test]
    fn byte_and_utf16_offsets_round_trip() {
        for s in ["hello", "aé本b", "x😀y", ""] {
            for (byte, _) in s.char_indices().chain([(s.len(), ' ')]) {
                let units = TextCaret::utf16_from_byte(s, byte);
                assert_eq!(
                    TextCaret::byte_from_utf16(s, units),
                    byte,
                    "{s:?} at byte {byte}"
                );
            }
        }
        assert_eq!(TextCaret::utf16_from_byte("x😀y", 5), 3);
        assert_eq!(TextCaret::utf16_from_byte("x😀y", 99), 4);
    }

    #[test]
    fn a_byte_offset_inside_a_character_falls_back_to_its_start() {
        assert_eq!(TextCaret::floor_boundary("aé本b", 4), 3);
        assert_eq!(TextCaret::floor_boundary("aé本b", 3), 3);
        assert_eq!(TextCaret::floor_boundary("aé本b", 99), 7);
        assert_eq!(TextCaret::floor_boundary("", 5), 0);
    }
}
