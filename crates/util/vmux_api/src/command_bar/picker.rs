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

#[vmux_api::contract(Eq)]
pub enum CommandBarPick {
    Picker(CommandBarPicker),
    GotoLine { line: u32 },
    Indent { spaces: bool, width: u16 },
    LineEnding { crlf: bool },
    Encoding { label: String, save: bool },
}

#[vmux_api::contract(Eq)]
pub struct CommandBarPickRow {
    pub label: String,
    pub pick: CommandBarPick,
}
