use vmux_ecs::event::StyledSpan;

pub(super) struct StyledSpanStyle;

impl StyledSpanStyle {
    pub(super) fn of(span: &StyledSpan) -> String {
        let [red, green, blue] = span.fg;
        let mut style = format!("color:rgb({red},{green},{blue});");
        if span.bold {
            style.push_str("font-weight:700;");
        }
        if span.italic {
            style.push_str("font-style:italic;");
        }
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_style_emits_color_and_emphasis() {
        let style = StyledSpanStyle::of(&StyledSpan {
            text: "x".into(),
            fg: [10, 20, 30],
            bold: true,
            italic: true,
        });
        assert!(style.contains("color:rgb(10,20,30)"));
        assert!(style.contains("font-weight:700"));
        assert!(style.contains("font-style:italic"));
    }
}
