use vmux_core::event::{FileDirEntry, StyledSpan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentClass {
    Dir,
    Image { mime: String },
    Text,
    Other,
}

pub fn image_mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
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

pub fn parent_highlight_index(parent: &[FileDirEntry], current_path: &str) -> Option<usize> {
    parent.iter().position(|e| e.path == current_path)
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
    fn parent_highlight_matches_current_dir() {
        let parent = vec![
            entry("/a/x", true),
            entry("/a/b", true),
            entry("/a/y", false),
        ];
        assert_eq!(parent_highlight_index(&parent, "/a/b"), Some(1));
        assert_eq!(parent_highlight_index(&parent, "/a/zzz"), None);
    }
}
