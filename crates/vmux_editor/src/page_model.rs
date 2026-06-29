use vmux_core::event::{DiagSeverity, FileDiagnostic, FileDirEntry, LspPkgStatus, StyledSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgAction {
    Install,
    Update,
    Uninstall,
    None,
}

pub fn pkg_status_label(status: LspPkgStatus) -> &'static str {
    match status {
        LspPkgStatus::Available => "Available",
        LspPkgStatus::OnPath => "On PATH",
        LspPkgStatus::Installing => "Installing…",
        LspPkgStatus::Installed => "Installed",
        LspPkgStatus::Outdated => "Update available",
        LspPkgStatus::Running => "Running",
        LspPkgStatus::Failed => "Failed",
    }
}

pub fn pkg_status_class(status: LspPkgStatus) -> &'static str {
    match status {
        LspPkgStatus::Installed | LspPkgStatus::Running => "text-ansi-2",
        LspPkgStatus::OnPath => "text-ansi-6",
        LspPkgStatus::Installing => "text-ansi-4",
        LspPkgStatus::Outdated => "text-ansi-3",
        LspPkgStatus::Failed => "text-ansi-1",
        LspPkgStatus::Available => "text-muted-foreground",
    }
}

pub fn pkg_action(status: LspPkgStatus, installable: bool) -> PkgAction {
    match status {
        LspPkgStatus::Installed | LspPkgStatus::Running => PkgAction::Uninstall,
        LspPkgStatus::Outdated => PkgAction::Update,
        LspPkgStatus::Installing => PkgAction::None,
        LspPkgStatus::OnPath => PkgAction::None,
        LspPkgStatus::Available | LspPkgStatus::Failed => {
            if installable {
                PkgAction::Install
            } else {
                PkgAction::None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentClass {
    Dir,
    Image { mime: String },
    Text,
    Other,
}

pub fn image_mime(path: &str) -> Option<&'static str> {
    vmux_core::media::image_mime(path)
}

pub fn classify(path: &str, is_dir: bool) -> ContentClass {
    if is_dir {
        return ContentClass::Dir;
    }
    if let Some(mime) = image_mime(path) {
        return ContentClass::Image {
            mime: mime.to_string(),
        };
    }
    if path.rsplit('/').next().is_some_and(|s| s.contains('.')) {
        ContentClass::Text
    } else {
        ContentClass::Other
    }
}

pub fn clamp_selection(idx: usize, len: usize) -> usize {
    if len == 0 { 0 } else { idx.min(len - 1) }
}

pub fn dir_select_index(entries: &[FileDirEntry], came_from: &str) -> usize {
    let name = came_from
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    if name.is_empty() {
        return 0;
    }
    entries.iter().position(|e| e.name == name).unwrap_or(0)
}

pub fn gutter_width(total_lines: u32) -> usize {
    let digits = total_lines.max(1).to_string().len();
    digits.max(3)
}

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

pub fn line_severity(diags: &[FileDiagnostic], line: u32) -> Option<DiagSeverity> {
    diags
        .iter()
        .filter(|d| d.line == line)
        .map(|d| d.severity)
        .min_by_key(|s| match s {
            DiagSeverity::Error => 0,
            DiagSeverity::Warning => 1,
            DiagSeverity::Info => 2,
            DiagSeverity::Hint => 3,
        })
}

pub fn severity_color_class(sev: DiagSeverity) -> &'static str {
    match sev {
        DiagSeverity::Error => "text-ansi-1",
        DiagSeverity::Warning => "text-ansi-3",
        DiagSeverity::Info => "text-ansi-4",
        DiagSeverity::Hint => "text-ansi-6",
    }
}

pub fn squiggle_style(start_col: u32, end_col: u32, color_rgb: &str) -> String {
    let width = end_col.saturating_sub(start_col).max(1);
    format!(
        "position:absolute;left:calc(var(--cw,1ch) * {start});\
         width:calc(var(--cw,1ch) * {width});bottom:0;height:1.1em;\
         border-bottom:2px solid {color};pointer-events:auto;",
        start = start_col,
        width = width,
        color = color_rgb,
    )
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

    #[test]
    fn line_severity_takes_most_severe() {
        let mk = |line, sev| FileDiagnostic {
            line,
            start_col: 0,
            end_col: 1,
            severity: sev,
            message: String::new(),
            source: None,
        };
        let v = vec![mk(3, DiagSeverity::Warning), mk(3, DiagSeverity::Error)];
        assert_eq!(line_severity(&v, 3), Some(DiagSeverity::Error));
        assert_eq!(line_severity(&v, 4), None);
    }

    #[test]
    fn squiggle_style_positions_by_columns() {
        let s = squiggle_style(2, 6, "rgb(255,0,0)");
        assert!(s.contains("left:calc(var(--cw,1ch) * 2)"));
        assert!(s.contains("width:calc(var(--cw,1ch) * 4)"));
    }

    #[test]
    fn pkg_action_by_status() {
        assert_eq!(
            pkg_action(LspPkgStatus::Available, true),
            PkgAction::Install
        );
        assert_eq!(pkg_action(LspPkgStatus::Available, false), PkgAction::None);
        assert_eq!(
            pkg_action(LspPkgStatus::Installed, true),
            PkgAction::Uninstall
        );
        assert_eq!(pkg_action(LspPkgStatus::Outdated, true), PkgAction::Update);
        assert_eq!(pkg_action(LspPkgStatus::Installing, true), PkgAction::None);
        assert_eq!(pkg_action(LspPkgStatus::OnPath, true), PkgAction::None);
    }

    #[test]
    fn pkg_status_label_covers_states() {
        assert_eq!(pkg_status_label(LspPkgStatus::OnPath), "On PATH");
        assert_eq!(pkg_status_label(LspPkgStatus::Installed), "Installed");
        assert_eq!(pkg_status_label(LspPkgStatus::Available), "Available");
    }
}

#[cfg(test)]
mod dir_browser_tests {
    use super::*;

    fn entry(path: &str, is_dir: bool) -> FileDirEntry {
        FileDirEntry {
            name: path.rsplit('/').next().unwrap().to_string(),
            path: path.to_string(),
            is_dir,
        }
    }

    #[test]
    fn classify_dir_and_image_and_text() {
        assert_eq!(classify("/a/b", true), ContentClass::Dir);
        assert_eq!(
            classify("/a/p.PNG", false),
            ContentClass::Image {
                mime: "image/png".into()
            }
        );
        assert_eq!(classify("/a/main.rs", false), ContentClass::Text);
        assert_eq!(classify("/a/blob", false), ContentClass::Other);
    }

    #[test]
    fn clamp_selection_bounds() {
        assert_eq!(clamp_selection(5, 3), 2);
        assert_eq!(clamp_selection(0, 0), 0);
        assert_eq!(clamp_selection(1, 3), 1);
    }

    #[test]
    fn dir_select_index_matches_came_from_by_basename() {
        let parent = vec![
            entry("/a/x", true),
            entry("/a/.worktrees", true),
            entry("/a/y", false),
        ];
        assert_eq!(dir_select_index(&parent, "/a/.worktrees"), 1);
        assert_eq!(dir_select_index(&parent, "a/.worktrees/"), 1);
        assert_eq!(dir_select_index(&parent, "~/proj/a/.worktrees"), 1);
        assert_eq!(dir_select_index(&parent, "/a/zzz"), 0);
        assert_eq!(dir_select_index(&parent, ""), 0);
    }
}
