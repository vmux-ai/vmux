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
#[path = "keyboard.test.rs"]
mod tests;
