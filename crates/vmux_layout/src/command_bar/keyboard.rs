#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlEditAction {
    Home,
    End,
    Forward,
    Back,
    Delete,
    Backspace,
    DeleteWord,
    DeleteToBeginning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlKeyCapture {
    Ignore,
    Edit(CtrlEditAction),
    PassToDioxus,
    RerouteToDioxus,
}

pub fn ctrl_key_capture_for_code(code: &str) -> CtrlKeyCapture {
    match code {
        "KeyA" => CtrlKeyCapture::Edit(CtrlEditAction::Home),
        "KeyE" => CtrlKeyCapture::Edit(CtrlEditAction::End),
        "KeyF" => CtrlKeyCapture::Edit(CtrlEditAction::Forward),
        "KeyB" => CtrlKeyCapture::Edit(CtrlEditAction::Back),
        "KeyD" => CtrlKeyCapture::Edit(CtrlEditAction::Delete),
        "KeyH" => CtrlKeyCapture::Edit(CtrlEditAction::Backspace),
        "KeyW" => CtrlKeyCapture::Edit(CtrlEditAction::DeleteWord),
        "KeyU" => CtrlKeyCapture::Edit(CtrlEditAction::DeleteToBeginning),
        "KeyC" | "KeyJ" | "KeyK" | "KeyN" | "KeyP" => CtrlKeyCapture::PassToDioxus,
        _ => CtrlKeyCapture::Ignore,
    }
}

pub fn ignore_physical_rerouted_ctrl_keydown(code: &str, is_synthetic: bool) -> bool {
    !is_synthetic
        && matches!(
            ctrl_key_capture_for_code(code),
            CtrlKeyCapture::RerouteToDioxus
        )
}

/// New horizontal `scroll_left` that keeps a caret at pixel offset `caret_px` visible in an
/// input of width `client_width` currently scrolled to `scroll_left`, preserving `margin` px
/// at whichever edge the caret approaches. Returns `None` when the caret is already visible
/// (no scroll change needed). Programmatic `set_selection_range` does not auto-scroll in
/// CEF/Chromium, so the command-bar input drives its own caret-follow with this.
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

/// Convert a UTF-16 code-unit offset (the unit DOM `selection_start`/`set_selection_range`
/// use) to a UTF-8 byte offset into `s`. Offsets past the end clamp to `s.len()`. Byte
/// offsets are what caret-follow needs to slice the value string for pixel measurement.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_j_and_k_pass_to_dioxus_without_synthetic_reroute() {
        assert_eq!(
            ctrl_key_capture_for_code("KeyJ"),
            CtrlKeyCapture::PassToDioxus
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyK"),
            CtrlKeyCapture::PassToDioxus
        );
    }

    #[test]
    fn ctrl_n_p_c_pass_to_dioxus() {
        assert_eq!(
            ctrl_key_capture_for_code("KeyN"),
            CtrlKeyCapture::PassToDioxus
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyP"),
            CtrlKeyCapture::PassToDioxus
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyC"),
            CtrlKeyCapture::PassToDioxus
        );
    }

    #[test]
    fn ctrl_text_edit_keys_are_handled_by_capture_listener() {
        assert_eq!(
            ctrl_key_capture_for_code("KeyA"),
            CtrlKeyCapture::Edit(CtrlEditAction::Home)
        );
        assert_eq!(
            ctrl_key_capture_for_code("KeyU"),
            CtrlKeyCapture::Edit(CtrlEditAction::DeleteToBeginning)
        );
    }

    #[test]
    fn physical_ctrl_j_k_are_not_suppressed_for_synthetic_reroute() {
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyJ", false));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyK", false));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyJ", true));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyK", true));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyN", false));
        assert!(!ignore_physical_rerouted_ctrl_keydown("KeyP", false));
    }

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
}
