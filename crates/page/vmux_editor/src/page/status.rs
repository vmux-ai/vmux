use dioxus::prelude::*;
use vmux_core::event::{CommandBarPicker, FileStatusPickerOpen, FileViewMode};
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

use super::Mode;

#[component]
pub(super) fn FileStatusInfo(
    scope: FileStatusScope,
    line: u32,
    col: u32,
    indent: vmux_core::event::FileIndent,
    line_ending: vmux_core::event::FileLineEnding,
    encoding: vmux_core::event::FileEncoding,
    language: String,
) -> Element {
    let position = translate_with(
        "editor-status-position",
        &[
            ("line", TranslationValue::Number(i64::from(line))),
            ("col", TranslationValue::Number(i64::from(col))),
        ],
    );
    let eol = match line_ending {
        vmux_core::event::FileLineEnding::Crlf => "CRLF",
        vmux_core::event::FileLineEnding::Lf => "LF",
    };
    rsx! {
        if scope.shows_caret() {
            StatusItemButton {
                label: position,
                title: translate("editor-status-goto-title"),
                extra: "tabular-nums",
                picker: CommandBarPicker::GotoLine,
            }
        }
        if scope.shows_indent() {
            StatusItemButton {
                label: IndentChoice::from(indent).label(),
                title: translate("editor-status-indent-title"),
                extra: "",
                picker: CommandBarPicker::Indent,
            }
        }
        StatusItemButton {
            label: encoding.label().to_string(),
            title: translate("editor-status-encoding-title"),
            extra: "",
            picker: CommandBarPicker::Encoding,
        }
        StatusItemButton {
            label: eol.to_string(),
            title: translate("editor-status-eol-title"),
            extra: "",
            picker: CommandBarPicker::LineEnding,
        }
        if !language.is_empty() {
            span { class: "shrink-0", "{language}" }
        }
    }
}

#[component]
fn StatusItemButton(
    label: String,
    title: String,
    extra: String,
    picker: CommandBarPicker,
) -> Element {
    rsx! {
        button {
            class: "shrink-0 rounded px-1 py-0.5 transition-colors hover:bg-foreground/[0.10] hover:text-foreground {extra}",
            title,
            onclick: move |_| {
                let _ = send(&FileStatusPickerOpen::from(picker));
            },
            "{label}"
        }
    }
}

#[component]
pub(super) fn EncodingRecovery() -> Element {
    rsx! {
        button {
            class: "shrink-0 rounded-md bg-foreground/10 px-3 py-1 font-sans text-xs font-medium text-foreground transition-colors hover:bg-foreground/20",
            onclick: move |_| {
                let _ = send(&FileStatusPickerOpen::from(CommandBarPicker::EncodingReopen));
            },
            {translate("editor-status-encoding-reopen")}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStatusScope {
    mode: Mode,
    view: FileViewMode,
    markdown: bool,
    has_diff: bool,
}

impl FileStatusScope {
    pub(super) fn new(mode: Mode, view: FileViewMode, markdown: bool, has_diff: bool) -> Self {
        Self {
            mode,
            view,
            markdown,
            has_diff,
        }
    }

    pub(super) fn reading_note(self) -> bool {
        self.mode == Mode::Text && self.view == FileViewMode::Note && self.markdown
    }

    fn reading_diff(self) -> bool {
        self.mode == Mode::Text && self.view == FileViewMode::Diff && self.has_diff
    }

    pub(super) fn shows_anything(self) -> bool {
        self.mode == Mode::Text
    }

    fn shows_caret(self) -> bool {
        self.shows_anything() && !self.reading_diff()
    }

    fn shows_indent(self) -> bool {
        self.shows_caret() && !self.reading_note()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct IndentChoice {
    spaces: bool,
    width: u16,
}

impl From<vmux_core::event::FileIndent> for IndentChoice {
    fn from(indent: vmux_core::event::FileIndent) -> Self {
        Self {
            spaces: indent.spaces,
            width: indent.width,
        }
    }
}

impl IndentChoice {
    fn label(self) -> String {
        let id = match self.spaces {
            true => "editor-status-spaces",
            false => "editor-status-tabs",
        };
        translate_with(
            id,
            &[("width", TranslationValue::Number(i64::from(self.width)))],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_core::media::MediaKind;

    fn text(view: FileViewMode, markdown: bool, has_diff: bool) -> FileStatusScope {
        FileStatusScope {
            mode: Mode::Text,
            view,
            markdown,
            has_diff,
        }
    }

    #[test]
    fn a_view_with_no_text_buffer_offers_none_of_the_group() {
        for mode in [Mode::Dir, Mode::Media(MediaKind::Image)] {
            let scope = FileStatusScope {
                mode,
                view: FileViewMode::Editor,
                markdown: false,
                has_diff: false,
            };

            assert!(!scope.shows_anything());
            assert!(!scope.shows_caret());
            assert!(!scope.shows_indent());
        }
    }

    #[test]
    fn the_source_editor_answers_every_item() {
        let scope = text(FileViewMode::Editor, false, false);

        assert!(scope.shows_anything());
        assert!(scope.shows_caret());
        assert!(scope.shows_indent());
    }

    #[test]
    fn a_rendered_note_keeps_the_caret_and_drops_the_indent() {
        let scope = text(FileViewMode::Note, true, false);

        assert!(scope.shows_caret());
        assert!(!scope.shows_indent());
        assert!(
            text(FileViewMode::Note, false, false).shows_indent(),
            "a non-markdown file shows the source editor whatever the shared view mode says"
        );
    }

    #[test]
    fn a_diff_drops_the_caret_and_the_indent_but_still_describes_the_file() {
        let scope = text(FileViewMode::Diff, false, true);

        assert!(scope.shows_anything());
        assert!(!scope.shows_caret());
        assert!(!scope.shows_indent());
        assert!(
            text(FileViewMode::Diff, false, false).shows_caret(),
            "a file with nothing to diff falls back to the editor, caret and all"
        );
    }
}
