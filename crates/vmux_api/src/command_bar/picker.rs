#[vmux_api::contract(Copy, Eq)]
pub enum CommandBarPicker {
    Space,
    GotoLine,
    Indent,
    LineEnding,
    Encoding,
    EncodingReopen,
    EncodingSave,
}

impl CommandBarPicker {
    pub const fn is_space(self) -> bool {
        matches!(self, Self::Space)
    }

    pub const fn takes_typed_value(self) -> bool {
        matches!(self, Self::GotoLine)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Space => "",
            Self::GotoLine => "editor-status-goto-title",
            Self::Indent => "editor-status-indent-title",
            Self::LineEnding => "editor-status-eol-title",
            Self::Encoding => "editor-status-encoding-title",
            Self::EncodingReopen => "editor-status-encoding-reopen",
            Self::EncodingSave => "editor-status-encoding-save",
        }
    }

    pub const fn placeholder(self) -> &'static str {
        match self {
            Self::Space => "command-switch-space",
            Self::GotoLine => "editor-status-goto-placeholder",
            Self::Indent
            | Self::LineEnding
            | Self::Encoding
            | Self::EncodingReopen
            | Self::EncodingSave => "editor-status-pick-placeholder",
        }
    }
}

#[vmux_api::contract(Eq)]
pub enum CommandBarPick {
    Picker(CommandBarPicker),
    GotoLine { line: u32 },
    Indent { spaces: bool, width: u16 },
    LineEnding { crlf: bool },
    Encoding { label: String, save: bool },
}

impl CommandBarPick {
    pub fn labelled(self, label: impl Into<String>) -> CommandBarPickRow {
        CommandBarPickRow {
            label: label.into(),
            pick: self,
        }
    }

    pub fn goto_line(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        let digits = match trimmed.split_once(':') {
            Some((line, _)) => line.trim(),
            None => trimmed,
        };
        let line = digits.parse::<u32>().ok()?;
        Some(Self::GotoLine {
            line: line.saturating_sub(1),
        })
    }
}

#[vmux_api::contract(Eq)]
pub struct CommandBarPickRow {
    pub label: String,
    pub pick: CommandBarPick,
}

impl CommandBarPickRow {
    pub fn matches(&self, query: &str) -> bool {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }
        self.label.to_lowercase().contains(&needle)
    }
}
