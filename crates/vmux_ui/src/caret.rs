//! Moving the caret in a text field, and reading what an event found selected.

/// The caret in one text field, addressed by element id and measured in UTF-8 bytes.
///
/// Dioxus can control a field's value but has no API for its caret or selection, so every
/// programmatic move — a readline chord, accepting a completion, revealing a URL ready to
/// overtype — has to reach the host. Byte offsets in and out: the DOM's UTF-16 code units stop
/// here, because mixing the two units is how a caret ends up beside the wrong character.
///
/// Instructions only. Reading is [`EventSelection`] — a different question with a different
/// lifetime, since an instruction is good whenever it is issued and an answer only for the event
/// it arrived with.
#[derive(Clone, Copy)]
pub struct TextCaret {
    element_id: &'static str,
}

impl TextCaret {
    /// The caret in the field with this id.
    pub fn in_field(element_id: &'static str) -> Self {
        Self { element_id }
    }
}

/// What was selected when the event now being dispatched was raised.
///
/// Meaningful only inside a handler, and the only reading of a selection a handler can do. A key
/// handler settles `prevent_default` before it returns, so it cannot await an answer; off the web
/// the values therefore travel on the event's own request rather than being asked for.
pub struct EventSelection;

impl EventSelection {
    /// Where the caret was in this field, ignoring anything selected past it.
    pub fn caret_in(element_id: &str) -> usize {
        Self::in_field(element_id).0
    }
}

impl EventSelection {
    /// What the event's own request carried, which is `(0, 0)` when the event came from anywhere
    /// but this field — where the web path also lands for a field that is gone.
    pub fn in_field(element_id: &str) -> (usize, usize) {
        crate::transport::Host::event_field_selection(element_id)
    }

    pub fn in_document() -> bool {
        crate::transport::Host::event_document_has_selection()
    }
}

impl TextCaret {
    /// Asks the host, which queues it for the page to apply behind the next frame.
    pub fn place(self, byte: usize) {
        crate::transport::Host::place_caret(self.element_id, byte);
    }

    /// Asks the host, which may have a document even though this target has no `web_sys`.
    pub fn select_all(self) {
        crate::transport::Host::select_element_text(self.element_id);
    }

    /// Asks the host. See [`Self::select_all`].
    pub fn clear(self) {
        crate::transport::Host::clear_element_text(self.element_id);
    }

    /// Asks the host. See [`Self::select_all`].
    pub fn select_all_from_start_next_frame(self) {
        crate::transport::Host::offer_element_text(self.element_id);
    }
}

/// Largest char boundary of `s` at or before `i`, so a DOM text offset never slices a UTF-8
/// string mid-character — which panics the wasm UI rather than merely misplacing the caret.
/// `str::floor_char_boundary` is still unstable.
pub fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Convert a UTF-16 code-unit offset — the unit DOM `selection_start` and `set_selection_range`
/// speak — to a UTF-8 byte offset into `s`. Offsets past the end clamp to `s.len()`.
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

/// The inverse of [`utf16_offset_to_byte`]. Offsets past the end clamp to the UTF-16 length.
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

/// New horizontal `scroll_left` that keeps a caret at pixel offset `caret_px` visible in a field
/// of width `client_width` currently scrolled to `scroll_left`, preserving `margin` px at
/// whichever edge the caret approaches. `None` when the caret is already visible.
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
        // Long URL, caret at end (500px) in a 200px box scrolled to 0.
        let s = caret_scroll_left(500.0, 200.0, 0.0, 12.0).expect("should scroll");
        assert!((s - (500.0 - 200.0 + 12.0)).abs() < 0.001, "got {s}");
        // Caret now sits inside the revealed range.
        assert!(s < 500.0 && 500.0 <= s + 200.0);
    }

    #[test]
    fn caret_before_left_edge_scrolls_left() {
        // Caret at 40px while scrolled to 300px must pull the view back.
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
        // "é" is 1 UTF-16 unit but 2 UTF-8 bytes; "本" is 1 unit, 3 bytes.
        let s = "aé本b";
        assert_eq!(utf16_offset_to_byte(s, 0), 0);
        assert_eq!(utf16_offset_to_byte(s, 1), 1); // after 'a'
        assert_eq!(utf16_offset_to_byte(s, 2), 3); // after 'é'
        assert_eq!(utf16_offset_to_byte(s, 3), 6); // after '本'
        assert_eq!(utf16_offset_to_byte(s, 4), 7); // after 'b'
    }

    #[test]
    fn utf16_offset_handles_surrogate_pairs_and_overflow() {
        // "😀" is a surrogate pair: 2 UTF-16 units, 4 UTF-8 bytes.
        let s = "x😀y";
        assert_eq!(utf16_offset_to_byte(s, 1), 1); // after 'x'
        assert_eq!(utf16_offset_to_byte(s, 3), 5); // after full emoji (1 + 4)
        assert_eq!(utf16_offset_to_byte(s, 99), s.len()); // past end clamps
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
