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

pub fn caret_scroll_left(
    caret_px: f64,
    client_width: f64,
    scroll_left: f64,
    margin: f64,
) -> Option<f64> {
    if !caret_px.is_finite() || client_width <= 0.0 {
        return None;
    }
    let margin = margin.clamp(0.0, client_width / 2.0);
    let new_scroll = if caret_px < scroll_left + margin {
        caret_px - margin
    } else if caret_px > scroll_left + client_width - margin {
        caret_px - client_width + margin
    } else {
        return None;
    }
    .max(0.0);
    ((new_scroll - scroll_left).abs() >= 0.5).then_some(new_scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_within_view_needs_no_scroll() {
        assert_eq!(caret_scroll_left(50.0, 200.0, 0.0, 12.0), None);
    }

    #[test]
    fn caret_past_right_edge_scrolls_right_to_reveal_it() {
        let s = caret_scroll_left(500.0, 200.0, 0.0, 12.0).expect("should scroll");
        assert!((s - (500.0 - 200.0 + 12.0)).abs() < 0.001, "got {s}");
        assert!(s < 500.0 && 500.0 <= s + 200.0);
    }

    #[test]
    fn caret_before_left_edge_scrolls_left() {
        let s = caret_scroll_left(40.0, 200.0, 300.0, 12.0).expect("should scroll");
        assert!((s - (40.0 - 12.0)).abs() < 0.001, "got {s}");
    }

    #[test]
    fn caret_at_home_clamps_scroll_to_zero() {
        assert_eq!(caret_scroll_left(0.0, 200.0, 300.0, 12.0), Some(0.0));
    }

    #[test]
    fn degenerate_geometry_is_ignored() {
        assert_eq!(caret_scroll_left(100.0, 0.0, 0.0, 12.0), None);
        assert_eq!(caret_scroll_left(f64::NAN, 200.0, 0.0, 12.0), None);
    }

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
