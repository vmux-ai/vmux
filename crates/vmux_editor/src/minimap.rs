use vmux_core::event::{FileLine, MinimapLine, MinimapRun};

use crate::viewport::clamp_top_line;

pub const MINIMAP_MAX_LINES: u32 = 100_000;
pub const MINIMAP_MAX_RUNS_PER_LINE: usize = 256;

pub fn build_overview(lines: &[FileLine]) -> Vec<MinimapLine> {
    lines.iter().map(build_line).collect()
}

fn build_line(line: &FileLine) -> MinimapLine {
    let mut runs: Vec<MinimapRun> = Vec::new();
    let mut cur: Option<MinimapRun> = None;
    let mut col: u32 = 0;
    'outer: for span in &line.spans {
        for ch in span.text.chars() {
            let c = col.min(u16::MAX as u32) as u16;
            if ch.is_whitespace() {
                if let Some(run) = cur.take() {
                    runs.push(run);
                    if runs.len() >= MINIMAP_MAX_RUNS_PER_LINE {
                        break 'outer;
                    }
                }
            } else {
                match cur.as_mut() {
                    Some(run) if run.fg == span.fg => {
                        run.len = run.len.saturating_add(1);
                    }
                    _ => {
                        if let Some(run) = cur.take() {
                            runs.push(run);
                            if runs.len() >= MINIMAP_MAX_RUNS_PER_LINE {
                                break 'outer;
                            }
                        }
                        cur = Some(MinimapRun {
                            fg: span.fg,
                            start: c,
                            len: 1,
                        });
                    }
                }
            }
            col += 1;
        }
    }
    if runs.len() < MINIMAP_MAX_RUNS_PER_LINE
        && let Some(run) = cur.take()
    {
        runs.push(run);
    }
    MinimapLine { runs }
}

pub fn line_to_y(line: u32, total_lines: u32, height_px: f32) -> f32 {
    if total_lines == 0 {
        return 0.0;
    }
    let line = line.min(total_lines);
    height_px * line as f32 / total_lines as f32
}

pub fn viewport_box(first_line: u32, rows: u16, total_lines: u32, height_px: f32) -> (f32, f32) {
    if total_lines == 0 {
        return (0.0, height_px);
    }
    let end = first_line.saturating_add(rows as u32).min(total_lines);
    let y = line_to_y(first_line, total_lines, height_px);
    let y_end = line_to_y(end, total_lines, height_px);
    (y, y_end - y)
}

pub fn y_to_top_line(y_px: f32, height_px: f32, total_lines: u32, rows: u16) -> u32 {
    if height_px <= 0.0 || total_lines == 0 {
        return 0;
    }
    let frac = (y_px / height_px).clamp(0.0, 1.0);
    let top = (frac * total_lines as f32).round().max(0.0) as u32;
    clamp_top_line(top, total_lines, rows)
}

pub fn sample_step(total_lines: u32, height_px: f32) -> usize {
    if height_px <= 0.0 || total_lines == 0 {
        return 1;
    }
    ((total_lines as f32 / height_px).ceil() as usize).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::event::{FileLine, StyledSpan};

    const A: [u8; 3] = [1, 2, 3];
    const B: [u8; 3] = [9, 8, 7];

    fn line(spans: &[(&str, [u8; 3])]) -> FileLine {
        FileLine {
            line_no: 0,
            spans: spans
                .iter()
                .map(|(t, fg)| StyledSpan {
                    text: (*t).into(),
                    fg: *fg,
                    bold: false,
                    italic: false,
                })
                .collect(),
        }
    }

    #[test]
    fn build_overview_whitespace_makes_gaps() {
        let out = build_overview(&[line(&[("ab cd", A)])]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].runs.len(), 2);
        assert_eq!((out[0].runs[0].start, out[0].runs[0].len), (0, 2));
        assert_eq!((out[0].runs[1].start, out[0].runs[1].len), (3, 2));
        assert_eq!(out[0].runs[0].fg, A);
    }

    #[test]
    fn build_overview_merges_same_color_across_spans() {
        let out = build_overview(&[line(&[("fn", A), ("x", A)])]);
        assert_eq!(out[0].runs.len(), 1);
        assert_eq!((out[0].runs[0].start, out[0].runs[0].len), (0, 3));
    }

    #[test]
    fn build_overview_splits_on_color_change() {
        let out = build_overview(&[line(&[("fn", A), ("x", B)])]);
        assert_eq!(out[0].runs.len(), 2);
        assert_eq!(out[0].runs[0].fg, A);
        assert_eq!((out[0].runs[0].start, out[0].runs[0].len), (0, 2));
        assert_eq!(out[0].runs[1].fg, B);
        assert_eq!((out[0].runs[1].start, out[0].runs[1].len), (2, 1));
    }

    #[test]
    fn build_overview_preserves_leading_indent() {
        let out = build_overview(&[line(&[("    x", A)])]);
        assert_eq!(out[0].runs.len(), 1);
        assert_eq!((out[0].runs[0].start, out[0].runs[0].len), (4, 1));
    }

    #[test]
    fn build_overview_empty_line_has_no_runs() {
        let out = build_overview(&[line(&[]), line(&[("   ", A)])]);
        assert_eq!(out.len(), 2);
        assert!(out[0].runs.is_empty());
        assert!(out[1].runs.is_empty());
    }

    #[test]
    fn build_overview_caps_runs_per_line() {
        let mut spans = Vec::new();
        for i in 0..(MINIMAP_MAX_RUNS_PER_LINE + 50) {
            let fg = if i % 2 == 0 { A } else { B };
            spans.push(("x", fg));
        }
        let owned: Vec<(&str, [u8; 3])> = spans;
        let out = build_overview(&[line(&owned)]);
        assert_eq!(out[0].runs.len(), MINIMAP_MAX_RUNS_PER_LINE);
    }

    #[test]
    fn line_to_y_fits_height() {
        assert_eq!(line_to_y(0, 100, 200.0), 0.0);
        assert_eq!(line_to_y(50, 100, 200.0), 100.0);
        assert_eq!(line_to_y(100, 100, 200.0), 200.0);
        assert_eq!(line_to_y(50, 0, 200.0), 0.0);
    }

    #[test]
    fn viewport_box_top_middle_end() {
        assert_eq!(viewport_box(0, 10, 100, 200.0), (0.0, 20.0));
        assert_eq!(viewport_box(50, 10, 100, 200.0), (100.0, 20.0));
        assert_eq!(viewport_box(95, 10, 100, 200.0), (190.0, 10.0));
    }

    #[test]
    fn viewport_box_file_shorter_than_viewport_fills() {
        assert_eq!(viewport_box(0, 10, 5, 200.0), (0.0, 200.0));
    }

    #[test]
    fn viewport_box_empty_file_fills() {
        assert_eq!(viewport_box(0, 10, 0, 200.0), (0.0, 200.0));
    }

    #[test]
    fn y_to_top_line_round_trips_with_box() {
        let (y, _) = viewport_box(50, 10, 100, 200.0);
        assert_eq!(y_to_top_line(y, 200.0, 100, 10), 50);
    }

    #[test]
    fn y_to_top_line_clamps_both_ends() {
        assert_eq!(y_to_top_line(-5.0, 200.0, 100, 10), 0);
        assert_eq!(y_to_top_line(500.0, 200.0, 100, 10), 90);
    }

    #[test]
    fn sample_step_at_least_one_and_collapses() {
        assert_eq!(sample_step(10, 500.0), 1);
        assert_eq!(sample_step(100_000, 500.0), 200);
        assert_eq!(sample_step(0, 500.0), 1);
        assert_eq!(sample_step(100, 0.0), 1);
    }
}
