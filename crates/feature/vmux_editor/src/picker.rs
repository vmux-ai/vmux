use vmux_api::command_bar::CommandBarPicker;

pub(crate) struct EditorPicker;

impl EditorPicker {
    pub(crate) const ENCODING: &str = "browser_open_encoding";
    pub(crate) const ENCODING_REOPEN: &str = "browser_open_reopen_with_encoding";
    pub(crate) const ENCODING_SAVE: &str = "browser_open_save_with_encoding";
    pub(crate) const GOTO_LINE: &str = "browser_open_goto_line";
    pub(crate) const INDENT: &str = "browser_open_indentation";
    pub(crate) const LINE_ENDING: &str = "browser_open_line_ending";

    #[cfg(ui)]
    pub(crate) fn encoding() -> CommandBarPicker {
        CommandBarPicker::new(Self::ENCODING)
    }

    pub(crate) fn encoding_reopen() -> CommandBarPicker {
        CommandBarPicker::new(Self::ENCODING_REOPEN)
    }

    pub(crate) fn encoding_save() -> CommandBarPicker {
        CommandBarPicker::new(Self::ENCODING_SAVE)
    }

    #[cfg(ui)]
    pub(crate) fn goto_line() -> CommandBarPicker {
        CommandBarPicker::new(Self::GOTO_LINE)
    }

    pub(crate) fn indent() -> CommandBarPicker {
        CommandBarPicker::new(Self::INDENT)
    }

    #[cfg(ui)]
    pub(crate) fn line_ending() -> CommandBarPicker {
        CommandBarPicker::new(Self::LINE_ENDING)
    }

    pub(crate) fn from_command(id: &str) -> Option<CommandBarPicker> {
        Self::owns(id).then(|| CommandBarPicker::new(id))
    }

    pub(crate) fn owns(id: &str) -> bool {
        matches!(
            id,
            Self::ENCODING
                | Self::ENCODING_REOPEN
                | Self::ENCODING_SAVE
                | Self::GOTO_LINE
                | Self::INDENT
                | Self::LINE_ENDING
        )
    }
}
