/// A readline edit the command-bar input performs on itself, named for the motion rather than
/// the chord that triggers it.
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

/// What the command bar does with a Ctrl chord it sees before the browser acts on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtrlKeyCapture {
    /// Not ours; let it through untouched.
    Ignore,
    /// Edit the text ourselves, because Chromium's own readline emulation would otherwise
    /// move a caret we are trying to keep authoritative.
    Edit(CtrlEditAction),
    /// Ours, but handled by the page's own key handler; only suppress the browser default.
    PassToDioxus,
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

/// The text and caret a readline edit produces. `caret` is a UTF-8 byte offset into `value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub value: String,
    pub caret: usize,
}

impl CtrlEditAction {
    /// Apply this edit to `value`, with the caret at UTF-8 byte offset `caret`.
    ///
    /// `ghost` is the inline completion offered past the end of the text, which [`Self::End`]
    /// accepts and every other action ignores.
    ///
    /// The deletions read the caret alone and ignore any selection, which is how the command bar
    /// has always behaved: Ctrl+W with text selected removes the word before the selection rather
    /// than the selection itself.
    pub fn apply(self, value: &str, caret: usize, ghost: &str) -> Edited {
        let caret = floor_char_boundary(value, caret);
        let kept = |caret| Edited {
            value: value.to_string(),
            caret,
        };
        let next = || {
            value[caret..]
                .chars()
                .next()
                .map_or(caret, |c| caret + c.len_utf8())
        };
        let prev = || {
            value[..caret]
                .chars()
                .next_back()
                .map_or(0, |c| caret - c.len_utf8())
        };
        let cut = |start: usize, end: usize| Edited {
            value: format!("{}{}", &value[..start], &value[end..]),
            caret: start,
        };

        match self {
            Self::Home => kept(0),
            Self::End if ghost.is_empty() => kept(value.len()),
            Self::End => {
                let value = format!("{value}{ghost}");
                Edited {
                    caret: value.len(),
                    value,
                }
            }
            Self::Forward => kept(next()),
            Self::Back => kept(prev()),
            Self::Delete => cut(caret, next()),
            Self::Backspace => cut(prev(), caret),
            Self::DeleteWord => cut(word_start_before(value, caret), caret),
            Self::DeleteToBeginning => Edited {
                value: value[caret..].to_string(),
                caret: 0,
            },
        }
    }
}

/// Where Ctrl+W stops: back over the spaces immediately behind the caret, then back over the
/// run of non-spaces before those. The character right at the caret is always consumed, so the
/// chord never does nothing when there is text to its left.
fn word_start_before(value: &str, caret: usize) -> usize {
    let bytes = value.as_bytes();
    let mut i = caret.saturating_sub(1);
    while i > 0 && bytes[i - 1] == b' ' {
        i -= 1;
    }
    while i > 0 && bytes[i - 1] != b' ' {
        i -= 1;
    }
    floor_char_boundary(value, i)
}

/// Largest char boundary of `s` at or before `i`, so a DOM text offset never slices a UTF-8
/// string mid-character — which panics the wasm UI rather than merely misplacing the caret.
pub fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
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

/// Convert a UTF-8 byte offset into `s` to the UTF-16 code-unit offset DOM
/// `set_selection_range` expects. Offsets past the end clamp to the string's UTF-16 length.
/// The inverse of [`utf16_offset_to_byte`], needed because the edits compute in bytes but the
/// caret has to be written back in the DOM's units.
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

    /// The whole Ctrl map in one place: which chords the command bar edits itself, which it
    /// merely suppresses so its own key handler sees them, and that everything else is left alone.
    #[test]
    fn every_ctrl_chord_is_edited_passed_on_or_ignored() {
        let edits = [
            ("KeyA", CtrlEditAction::Home),
            ("KeyE", CtrlEditAction::End),
            ("KeyF", CtrlEditAction::Forward),
            ("KeyB", CtrlEditAction::Back),
            ("KeyD", CtrlEditAction::Delete),
            ("KeyH", CtrlEditAction::Backspace),
            ("KeyW", CtrlEditAction::DeleteWord),
            ("KeyU", CtrlEditAction::DeleteToBeginning),
        ];
        for (code, action) in edits {
            assert_eq!(
                ctrl_key_capture_for_code(code),
                CtrlKeyCapture::Edit(action),
                "{code}"
            );
        }

        for code in ["KeyC", "KeyJ", "KeyK", "KeyN", "KeyP"] {
            assert_eq!(
                ctrl_key_capture_for_code(code),
                CtrlKeyCapture::PassToDioxus,
                "{code}"
            );
        }

        for code in ["KeyG", "KeyZ", "Enter", "Tab", ""] {
            assert_eq!(
                ctrl_key_capture_for_code(code),
                CtrlKeyCapture::Ignore,
                "{code}"
            );
        }
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

    /// `(action, value, caret, expected value, expected caret)` over one ASCII string, so the
    /// whole readline set is pinned in one place rather than one test per chord.
    #[test]
    fn ctrl_edits_move_and_cut_as_readline_does() {
        let cases = [
            (CtrlEditAction::Home, "foo bar", 7, "foo bar", 0),
            (CtrlEditAction::End, "foo bar", 0, "foo bar", 7),
            (CtrlEditAction::Forward, "foo bar", 3, "foo bar", 4),
            (CtrlEditAction::Back, "foo bar", 3, "foo bar", 2),
            (CtrlEditAction::Delete, "foo bar", 3, "foobar", 3),
            (CtrlEditAction::Backspace, "foo bar", 4, "foobar", 3),
            (CtrlEditAction::DeleteWord, "foo bar", 7, "foo ", 4),
            (CtrlEditAction::DeleteWord, "foo bar ", 8, "foo ", 4),
            (CtrlEditAction::DeleteToBeginning, "foo bar", 4, "bar", 0),
            (CtrlEditAction::DeleteToBeginning, "foo bar", 7, "", 0),
        ];
        for (action, value, caret, want_value, want_caret) in cases {
            let got = action.apply(value, caret, "");
            assert_eq!(
                got,
                Edited {
                    value: want_value.to_string(),
                    caret: want_caret
                },
                "{action:?} on {value:?} at {caret}"
            );
        }
    }

    #[test]
    fn edits_at_the_ends_of_the_text_change_nothing() {
        let at_start = CtrlEditAction::Backspace.apply("foo", 0, "");
        assert_eq!(at_start.value, "foo");
        assert_eq!(at_start.caret, 0);

        let at_end = CtrlEditAction::Delete.apply("foo", 3, "");
        assert_eq!(at_end.value, "foo");
        assert_eq!(at_end.caret, 3);

        assert_eq!(CtrlEditAction::Back.apply("foo", 0, "").caret, 0);
        assert_eq!(CtrlEditAction::Forward.apply("foo", 3, "").caret, 3);
    }

    /// The caret arrives as a byte offset, so an edit next to a multi-byte character must act on
    /// the character the caret is actually beside. Passing a UTF-16 offset through unconverted
    /// deleted the wrong character here.
    #[test]
    fn edits_next_to_multibyte_characters_act_on_the_right_one() {
        // "aé本b": 'a'@0, 'é'@1..3, '本'@3..6, 'b'@6..7.
        let s = "aé本b";
        assert_eq!(CtrlEditAction::Delete.apply(s, 6, "").value, "aé本");
        assert_eq!(CtrlEditAction::Delete.apply(s, 3, "").value, "aéb");
        assert_eq!(CtrlEditAction::Backspace.apply(s, 6, "").value, "aéb");

        let back = CtrlEditAction::Back.apply(s, 6, "");
        assert_eq!(back.caret, 3, "one character back, not one byte");
        assert_eq!(CtrlEditAction::Forward.apply(s, 3, "").caret, 6);
    }

    /// A caret landing inside a character is pulled back to its start rather than panicking the
    /// slice, so a stale offset degrades to a misplaced caret.
    #[test]
    fn a_caret_inside_a_character_is_pulled_to_its_start() {
        let inside = CtrlEditAction::Delete.apply("aé本b", 4, "");
        assert_eq!(inside.value, "aéb");
        assert_eq!(inside.caret, 3);
    }

    #[test]
    fn ctrl_e_accepts_the_inline_completion_and_lands_past_it() {
        let accepted = CtrlEditAction::End.apply("git.co", 6, "m/vmux");
        assert_eq!(accepted.value, "git.com/vmux");
        assert_eq!(accepted.caret, 12);

        // Only End reads the ghost.
        assert_eq!(
            CtrlEditAction::Home.apply("git.co", 6, "m/vmux").value,
            "git.co"
        );
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
