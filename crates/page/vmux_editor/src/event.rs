#[vmux_api::ui_event_variants(
    Copy,
    Eq,
    url = "file://",
    shared(line: u32, col: u32)
)]
pub(crate) enum FileEditorOperation {
    GotoDeclaration,
    GotoTypeDefinition,
    GotoImplementation,
    Rename,
    FormatDocument,
    FormatSelection,
    Cut,
    Copy,
    Paste,
    ChangeAllOccurrences,
    CodeAction,
    CommandPalette,
}
