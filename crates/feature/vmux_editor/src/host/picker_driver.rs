use vmux_api::command_bar::{CommandBarPick, CommandBarPickRow, CommandBarPicker};
use vmux_command::CommandBarOpenRequest;
use vmux_ui::i18n::{Locale, TranslationValue};

use crate::picker::EditorPicker;

pub(crate) struct EditorPicks;

impl EditorPicks {
    pub(crate) fn request(picker: CommandBarPicker, locale: &Locale) -> CommandBarOpenRequest {
        CommandBarOpenRequest::picker_with(
            picker.clone(),
            Self::for_picker(&picker, locale),
            Self::label(&picker, locale),
            Self::placeholder(&picker, locale),
            picker.is(EditorPicker::GOTO_LINE),
            false,
        )
    }

    fn for_picker(picker: &CommandBarPicker, locale: &Locale) -> Vec<CommandBarPickRow> {
        if picker.is(EditorPicker::GOTO_LINE) {
            Vec::new()
        } else if picker.is(EditorPicker::INDENT) {
            Self::indents(locale)
        } else if picker.is(EditorPicker::LINE_ENDING) {
            vec![
                Self::value("LF", picker.clone(), "lf"),
                Self::value("CRLF", picker.clone(), "crlf"),
            ]
        } else if picker.is(EditorPicker::ENCODING) {
            vec![
                Self::row(
                    locale.translate("editor-status-encoding-reopen"),
                    CommandBarPick::Picker(EditorPicker::encoding_reopen()),
                ),
                Self::row(
                    locale.translate("editor-status-encoding-save"),
                    CommandBarPick::Picker(EditorPicker::encoding_save()),
                ),
            ]
        } else if picker.is(EditorPicker::ENCODING_REOPEN) || picker.is(EditorPicker::ENCODING_SAVE)
        {
            Self::encodings(picker)
        } else {
            Vec::new()
        }
    }

    fn label(picker: &CommandBarPicker, locale: &Locale) -> String {
        let id = if picker.is(EditorPicker::GOTO_LINE) {
            "editor-status-goto-title"
        } else if picker.is(EditorPicker::INDENT) {
            "editor-status-indent-title"
        } else if picker.is(EditorPicker::LINE_ENDING) {
            "editor-status-eol-title"
        } else if picker.is(EditorPicker::ENCODING) {
            "editor-status-encoding-title"
        } else if picker.is(EditorPicker::ENCODING_REOPEN) {
            "editor-status-encoding-reopen"
        } else if picker.is(EditorPicker::ENCODING_SAVE) {
            "editor-status-encoding-save"
        } else {
            return String::new();
        };
        locale.translate(id)
    }

    fn placeholder(picker: &CommandBarPicker, locale: &Locale) -> String {
        let id = if picker.is(EditorPicker::GOTO_LINE) {
            "editor-status-goto-placeholder"
        } else if EditorPicker::owns(&picker.0) {
            "editor-status-pick-placeholder"
        } else {
            return String::new();
        };
        locale.translate(id)
    }

    fn indents(locale: &Locale) -> Vec<CommandBarPickRow> {
        let mut rows = Vec::with_capacity(6);
        for spaces in [true, false] {
            for width in [2u16, 4, 8] {
                let id = match spaces {
                    true => "editor-status-spaces",
                    false => "editor-status-tabs",
                };
                let label = locale
                    .translate_with(id, &[("width", TranslationValue::Number(i64::from(width)))]);
                let kind = if spaces { "spaces" } else { "tabs" };
                rows.push(Self::value(
                    label,
                    EditorPicker::indent(),
                    format!("{kind}:{width}"),
                ));
            }
        }
        rows
    }

    fn encodings(picker: &CommandBarPicker) -> Vec<CommandBarPickRow> {
        let mut rows = Vec::with_capacity(vmux_ecs::event::FileEncoding::ALL.len());
        for encoding in vmux_ecs::event::FileEncoding::ALL {
            rows.push(Self::value(
                encoding.label(),
                picker.clone(),
                encoding.label(),
            ));
        }
        rows
    }

    fn row(label: impl Into<String>, pick: CommandBarPick) -> CommandBarPickRow {
        CommandBarPickRow {
            label: label.into(),
            pick,
        }
    }

    fn value(
        label: impl Into<String>,
        picker: CommandBarPicker,
        value: impl Into<String>,
    ) -> CommandBarPickRow {
        Self::row(
            label,
            CommandBarPick::Typed {
                picker,
                value: value.into(),
            },
        )
    }
}
