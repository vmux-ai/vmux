//! Pure scroll-windowing math shared by the file editor and the terminal.
//! Rows are `u32`/`u16` counts; no Bevy, no DOM — compiles for wasm.

/// Multiple of the visible row count to buffer beyond the viewport, per side.
pub const EDITOR_OVERSCAN_K: f32 = 1.5;
/// Terminal buffers more: its window refill crosses an extra process hop.
pub const TERMINAL_OVERSCAN_K: f32 = 2.0;
/// Minimum overscan (small panes still get a usable runway).
pub const OVERSCAN_FLOOR: u32 = 48;
/// Maximum overscan (bound DOM node count on very tall panes).
pub const OVERSCAN_CAP: u32 = 512;
/// Refetch trigger margin as a multiple of the visible row count.
pub const EDGE_TRIGGER_K: f32 = 1.0;

/// Clamp a requested top line so the viewport never scrolls past the last page.
pub fn clamp_top_line(top_line: u32, total_lines: u32, rows: u16) -> u32 {
    let max_top = total_lines.saturating_sub(rows as u32);
    top_line.min(max_top)
}

/// `[first, end)` line range for a viewport of `rows` starting at `top_line`.
pub fn window_range(total_lines: u32, top_line: u32, rows: u16) -> (u32, u32) {
    let first = clamp_top_line(top_line, total_lines, rows);
    let end = first.saturating_add(rows as u32).min(total_lines);
    (first, end)
}

/// Number of whole rows that fit in `viewport_height` at `char_height`.
pub fn rows_from_viewport(char_height: f32, viewport_height: f32) -> u16 {
    if char_height <= 0.0 || viewport_height <= 0.0 {
        return 0;
    }
    (viewport_height / char_height).floor() as u16
}

/// `[first, end)` as a `usize` range, for slicing an in-memory line buffer.
pub fn visible_slice(total: u32, top_line: u32, rows: u16) -> std::ops::Range<usize> {
    let (first, end) = window_range(total, top_line, rows);
    (first as usize)..(end as usize)
}

/// Rows to hold beyond the visible region on EACH side, scaled to the viewport
/// and clamped to `[floor, cap]`.
pub fn overscan_for(visible: u16, k: f32, floor: u32, cap: u32) -> u32 {
    let scaled = (visible as f32 * k).ceil() as u32;
    scaled.clamp(floor, cap)
}

/// True when the visible region is within `trigger` rows of the loaded window
/// edge, i.e. a refill should be requested now.
pub fn needs_refetch(
    vis_first: u32,
    vis_rows: u32,
    loaded_first: u32,
    loaded_len: u32,
    trigger: u32,
) -> bool {
    let loaded_end = loaded_first.saturating_add(loaded_len);
    let near_top = vis_first < loaded_first.saturating_add(trigger);
    let near_bot = vis_first + vis_rows + trigger > loaded_end;
    near_top || near_bot
}

/// alacritty grid `Line` for a document row (row 0 = oldest scrollback line):
/// `Line(doc_row - history_size)`. Returned as `i32` (may be negative = history).
pub fn doc_row_to_line(doc_row: u32, history_size: u32) -> i32 {
    doc_row as i32 - history_size as i32
}

/// Bottom alignment pad (px) for a terminal that follows by pinning to the
/// scroll maximum.
///
/// A follow-to-bottom terminal scrolls to `scrollHeight`, so the fractional
/// sub-row left over when `client_h` is not a whole multiple of the cell
/// height `ch` lands as a clipped partial row at the *top* of the viewport.
/// Adding this many pixels below the last row shifts that remainder to the
/// bottom instead, so the pinned top edge falls on a row boundary and the top
/// line renders whole. `pad` is the container's top inner padding (the row
/// grid's y-origin). The result is in `[0, ch)`.
pub fn follow_bottom_pad(client_h: f32, pad: f32, ch: f32) -> f32 {
    if ch <= 0.0 {
        return 0.0;
    }
    (client_h - pad).rem_euclid(ch)
}

#[cfg(test)]
#[path = "scroll.test.rs"]
mod tests;
