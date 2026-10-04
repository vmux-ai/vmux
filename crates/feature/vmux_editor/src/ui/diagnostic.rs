use vmux_ecs::event::{DiagSeverity, FileDiagnostic};

pub(super) struct DiagnosticPresentation;

impl DiagnosticPresentation {
    pub(super) fn severity(diags: &[FileDiagnostic], line: u32) -> Option<DiagSeverity> {
        diags
            .iter()
            .filter(|diagnostic| diagnostic.line == line)
            .map(|diagnostic| diagnostic.severity)
            .min_by_key(|severity| match severity {
                DiagSeverity::Error => 0,
                DiagSeverity::Warning => 1,
                DiagSeverity::Info => 2,
                DiagSeverity::Hint => 3,
            })
    }

    pub(super) fn color_class(severity: DiagSeverity) -> &'static str {
        match severity {
            DiagSeverity::Error => "text-ansi-1",
            DiagSeverity::Warning => "text-ansi-3",
            DiagSeverity::Info => "text-ansi-4",
            DiagSeverity::Hint => "text-ansi-6",
        }
    }

    pub(super) fn squiggle(left: f64, width: f64, color_rgb: &str) -> String {
        format!(
            "position:absolute;left:{left}px;width:{width}px;bottom:0;height:1.1em;\
             border-bottom:2px solid {color_rgb};pointer-events:auto;",
            width = width.max(1.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_selects_the_strongest_diagnostic() {
        let diagnostic = |line, severity| FileDiagnostic {
            line,
            start_col: 0,
            end_col: 1,
            severity,
            message: String::new(),
            source: None,
        };
        let diagnostics = vec![
            diagnostic(3, DiagSeverity::Warning),
            diagnostic(3, DiagSeverity::Error),
        ];
        assert_eq!(
            DiagnosticPresentation::severity(&diagnostics, 3),
            Some(DiagSeverity::Error)
        );
        assert_eq!(DiagnosticPresentation::severity(&diagnostics, 4), None);
    }

    #[test]
    fn empty_ranges_keep_a_pointer_target() {
        let style = DiagnosticPresentation::squiggle(16.0, 0.0, "rgb(255,0,0)");
        assert!(style.contains("left:16px"));
        assert!(style.contains("width:1px"));
    }
}
