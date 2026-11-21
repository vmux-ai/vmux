#[vmux_api::ui_event_variants(
    Copy,
    Eq,
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

#[vmux_api::ui_event(Eq)]
pub(crate) struct DocumentTextInput {
    pub revision: u64,
    pub text: String,
}

#[vmux_api::ui_event(Copy, Eq)]
pub(crate) struct DocumentPointerInput {
    pub revision: u64,
    pub line: u32,
    pub col: u32,
    pub extend: bool,
    pub add: bool,
}

#[vmux_api::ui_event(Default, Eq)]
pub(crate) struct DocumentFindInput {
    pub revision: u64,
    pub query: String,
    pub step: bool,
    pub reverse: bool,
    pub done: bool,
    pub regex: bool,
    pub forward: bool,
}

#[vmux_api::ui_event(Eq)]
pub(crate) struct DocumentKeyInput {
    pub revision: u64,
    pub stroke: vmux_api::input::KeyStroke,
}

#[vmux_api::ui_event(Eq)]
pub(crate) struct DocumentPropertyInput {
    pub revision: u64,
    pub edit: vmux_ecs::event::FilePropertyEdit,
}
