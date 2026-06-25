#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

pub fn editable_tag_forces_insert(tag: &str, content_editable: bool) -> bool {
    if content_editable {
        return true;
    }
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "input" | "textarea" | "select"
    )
}

pub fn resolve_mode(
    focused_tag: Option<&str>,
    content_editable: bool,
    force_insert: bool,
    force_normal: bool,
) -> Mode {
    if force_normal {
        return Mode::Normal;
    }
    if force_insert {
        return Mode::Insert;
    }
    match focused_tag {
        Some(tag) if editable_tag_forces_insert(tag, content_editable) => Mode::Insert,
        _ => Mode::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_force_insert() {
        assert!(editable_tag_forces_insert("INPUT", false));
        assert!(editable_tag_forces_insert("textarea", false));
        assert!(editable_tag_forces_insert("select", false));
    }

    #[test]
    fn contenteditable_forces_insert() {
        assert!(editable_tag_forces_insert("div", true));
    }

    #[test]
    fn plain_elements_do_not_force_insert() {
        assert!(!editable_tag_forces_insert("div", false));
        assert!(!editable_tag_forces_insert("a", false));
        assert!(!editable_tag_forces_insert("body", false));
    }

    #[test]
    fn focus_on_input_is_insert() {
        assert_eq!(
            resolve_mode(Some("input"), false, false, false),
            Mode::Insert
        );
    }

    #[test]
    fn escape_forces_normal_even_in_input() {
        assert_eq!(
            resolve_mode(Some("input"), false, false, true),
            Mode::Normal
        );
    }

    #[test]
    fn i_forces_insert_on_plain_element() {
        assert_eq!(resolve_mode(Some("div"), false, true, false), Mode::Insert);
    }
}
