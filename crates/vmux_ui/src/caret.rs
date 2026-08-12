//! Placing the caret in a text field, where only the host can do it.

/// The caret in one text field, addressed by element id and measured in UTF-8 bytes.
///
/// Dioxus can control a field's value but has no API for its caret or selection, so every
/// programmatic move — a readline chord, accepting a completion, revealing a URL ready to
/// overtype — has to reach the host. Byte offsets in and out: the DOM's UTF-16 code units stop
/// here, because mixing the two units is how a caret ends up beside the wrong character.
#[derive(Clone, Copy)]
#[cfg_attr(not(web), allow(dead_code))]
pub struct TextCaret {
    element_id: &'static str,
}

impl TextCaret {
    /// The caret in the `<input>` with this id.
    pub fn in_field(element_id: &'static str) -> Self {
        Self { element_id }
    }
}

#[cfg(web)]
mod imp {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    use super::{
        TextCaret, byte_offset_to_utf16, caret_scroll_left, floor_char_boundary,
        utf16_offset_to_byte,
    };

    /// Pixels of text kept visible past the caret when the field has to scroll to reach it.
    const FOLLOW_MARGIN_PX: f64 = 8.0;

    impl TextCaret {
        /// Where the caret is, as a byte offset into the field's value. Zero if the field is gone.
        pub fn position(self) -> usize {
            let Some(input) = self.input() else {
                return 0;
            };
            let utf16 = input.selection_start().unwrap_or(Some(0)).unwrap_or(0);
            utf16_offset_to_byte(&input.value(), utf16)
        }

        /// Put the caret at a byte offset, scrolling the field so it is visible.
        ///
        /// The two are one operation because a programmatic move bypasses Chromium's own
        /// caret-follow: on a long URL, placing without scrolling leaves the caret off-screen,
        /// which reads as the keystroke having done nothing.
        pub fn place(self, byte: usize) {
            let Some(input) = self.input() else {
                return;
            };
            let value = input.value();
            let byte = floor_char_boundary(&value, byte);
            let utf16 = byte_offset_to_utf16(&value, byte);
            let _ = input.set_selection_range(utf16, utf16);
            Self::follow(&input, &value[..byte]);
        }

        /// Highlight the whole value, leaving the view where it is.
        pub fn select_all(self) {
            let Some(input) = self.input() else {
                return;
            };
            let end = input.value().encode_utf16().count() as u32;
            let _ = input.set_selection_range(0, end);
        }

        /// Highlight the whole value one frame from now, taking focus and rewinding the view to
        /// the start of the text.
        ///
        /// Deferred because a caller that has just set the value through a signal cannot select
        /// it yet: the render that puts those characters in the field has not happened, so
        /// selecting now would highlight the previous value — usually an empty one, which looks
        /// like nothing happened. Focus is claimed here as well as by
        /// [`crate::focus::FocusClaim`] because focusing an input may itself move the selection,
        /// so a retry landing after this would otherwise undo it.
        ///
        /// The rewind is what separates this from [`Self::select_all`]: this is the gesture that
        /// offers a value up to be overtyped, and a long one scrolled to its tail does not read
        /// as an offer.
        pub fn select_all_from_start_next_frame(self) {
            let Some(window) = web_sys::window() else {
                return;
            };
            let callback = Closure::once_into_js(move || {
                let Some(input) = self.input() else {
                    return;
                };
                let _ = input.focus();
                self.select_all();
                input.set_scroll_left(0);
            });
            let _ = window.request_animation_frame(callback.unchecked_ref());
        }

        fn input(self) -> Option<web_sys::HtmlInputElement> {
            web_sys::window()?
                .document()?
                .get_element_by_id(self.element_id)?
                .dyn_into()
                .ok()
        }

        fn follow(input: &web_sys::HtmlInputElement, before_caret: &str) {
            let Some((viewport, caret_px)) = Self::metrics(input, before_caret) else {
                return;
            };
            let scroll_left = input.scroll_left() as f64;
            if let Some(scrolled) =
                caret_scroll_left(caret_px, viewport, scroll_left, FOLLOW_MARGIN_PX)
            {
                input.set_scroll_left(scrolled as i32);
            }
        }

        /// The field's usable text width and the pixel offset of `prefix` in its current font,
        /// measured on an offscreen canvas. `None` when the canvas, its context or the computed
        /// font is unavailable, which leaves the field scrolled where it was.
        fn metrics(input: &web_sys::HtmlInputElement, prefix: &str) -> Option<(f64, f64)> {
            let window = web_sys::window()?;
            let document = window.document()?;
            let style = window.get_computed_style(input).ok()??;
            let font_size = style.get_property_value("font-size").unwrap_or_default();
            let font_family = style.get_property_value("font-family").unwrap_or_default();
            if font_size.is_empty() || font_family.is_empty() {
                return None;
            }
            let font_weight = style.get_property_value("font-weight").unwrap_or_default();
            let font_style = style.get_property_value("font-style").unwrap_or_default();
            let canvas: web_sys::HtmlCanvasElement =
                document.create_element("canvas").ok()?.unchecked_into();
            let context: web_sys::CanvasRenderingContext2d =
                canvas.get_context("2d").ok()??.unchecked_into();
            context
                .set_font(format!("{font_style} {font_weight} {font_size} {font_family}").trim());
            let caret_px = context.measure_text(prefix).ok()?.width();
            let pad_left = css_px(&style.get_property_value("padding-left").unwrap_or_default());
            let pad_right = css_px(
                &style
                    .get_property_value("padding-right")
                    .unwrap_or_default(),
            );
            let viewport = (input.client_width() as f64 - pad_left - pad_right).max(1.0);
            caret_px.is_finite().then_some((viewport, caret_px))
        }
    }

    /// Parse a computed `<n>px` length, defaulting to `0.0`.
    fn css_px(value: &str) -> f64 {
        value
            .trim()
            .strip_suffix("px")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    }
}

#[cfg(not(web))]
impl TextCaret {
    /// Inert: a touch host has no programmatic caret to place, and the field scrolls its own.
    pub fn position(self) -> usize {
        0
    }

    /// Inert. See [`Self::position`].
    pub fn place(self, _byte: usize) {}

    /// Inert. See [`Self::position`].
    pub fn select_all(self) {}

    /// Inert. See [`Self::position`].
    pub fn select_all_from_start_next_frame(self) {}
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
