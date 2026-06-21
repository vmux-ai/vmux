use vmux_core::event::StyledSpan;

/// Gutter character width = digits in the largest line number, min 3.
pub fn gutter_width(total_lines: u32) -> usize {
    let digits = total_lines.max(1).to_string().len();
    digits.max(3)
}

/// Inline CSS for a styled span: `color` + optional bold/italic.
pub fn span_style(span: &StyledSpan) -> String {
    let [r, g, b] = span.fg;
    let mut s = format!("color:rgb({r},{g},{b});");
    if span.bold {
        s.push_str("font-weight:700;");
    }
    if span.italic {
        s.push_str("font-style:italic;");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_width_min_three() {
        assert_eq!(gutter_width(0), 3);
        assert_eq!(gutter_width(9), 3);
        assert_eq!(gutter_width(1000), 4);
        assert_eq!(gutter_width(99999), 5);
    }

    #[test]
    fn span_style_emits_color_and_styles() {
        let s = span_style(&StyledSpan {
            text: "x".into(),
            fg: [10, 20, 30],
            bold: true,
            italic: true,
        });
        assert!(s.contains("color:rgb(10,20,30)"));
        assert!(s.contains("font-weight:700"));
        assert!(s.contains("font-style:italic"));
    }
}
