#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileArrival {
    OpensFile,
    RefreshesOpenFile,
}

impl FileArrival {
    pub(super) fn new(abs_path: &str, showing: &str) -> Self {
        if !showing.is_empty() && showing == abs_path {
            return Self::RefreshesOpenFile;
        }
        Self::OpensFile
    }

    pub(super) fn resets_view(self) -> bool {
        self == Self::OpensFile
    }
}

pub(super) fn is_markdown_file(path: &str) -> bool {
    path.rsplit_once('.')
        .map(|(_, extension)| {
            extension.eq_ignore_ascii_case("md")
                || extension.eq_ignore_ascii_case("markdown")
                || extension.eq_ignore_ascii_case("mdx")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_for_the_file_already_on_screen_leaves_the_view_alone() {
        assert!(!FileArrival::new("/w/src/main.rs", "/w/src/main.rs").resets_view());
    }

    #[test]
    fn metadata_for_another_file_resets_the_view() {
        assert!(FileArrival::new("/w/src/other.rs", "/w/src/main.rs").resets_view());
        assert!(FileArrival::new("/w/src/main.rs", "").resets_view());
        assert!(FileArrival::new("", "").resets_view());
    }
}
