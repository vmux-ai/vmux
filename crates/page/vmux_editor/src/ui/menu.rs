use dioxus::prelude::*;
use vmux_core::event::{
    EditorCapability, FileCodeActionPick, FileDefinitionRequest, FileGotoRequest,
    FileReferencesRequest, FileRenameRequest, RefItem,
};
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::ime::use_ime_guard;

use super::focus_file_input;
use crate::event::FileEditorOperation;
use crate::page_key::FileKeys;

const RENAME_ID: &str = "file-rename";
const CODE_ACTION_ID: &str = "file-code-action";

#[component]
pub(super) fn CodeActionMenu(
    titles: Signal<Vec<String>>,
    selected: Signal<usize>,
    top: f64,
    left: f64,
) -> Element {
    let mut titles = titles;
    let mut selected = selected;
    let entries = titles();
    if entries.is_empty() {
        return rsx! {};
    }
    let chosen = selected().min(entries.len() - 1);
    rsx! {
        div {
            id: CODE_ACTION_ID,
            tabindex: 0,
            autofocus: true,
            class: "absolute z-50 max-h-56 min-w-64 overflow-auto rounded-lg bg-background/95 py-1 text-xs text-foreground outline-none ring-1 ring-inset ring-primary/30 backdrop-blur-2xl shadow-lg",
            style: "left:{left}px;top:{top}px;",
            onkeydown: move |event| {
                event.stop_propagation();
                let len = titles().len();
                match event.key() {
                    Key::ArrowDown => {
                        event.prevent_default();
                        selected.set((chosen + 1) % len);
                    }
                    Key::ArrowUp => {
                        event.prevent_default();
                        selected.set((chosen + len - 1) % len);
                    }
                    Key::Enter => {
                        event.prevent_default();
                        let _ = send(&FileCodeActionPick { index: chosen as u32 });
                        titles.set(Vec::new());
                        focus_file_input();
                    }
                    Key::Escape => {
                        event.prevent_default();
                        titles.set(Vec::new());
                        focus_file_input();
                    }
                    _ => {}
                }
            },
            onblur: move |_| titles.set(Vec::new()),
            for (index, title) in entries.iter().enumerate() {
                div {
                    key: "{index}",
                    class: if index == chosen { "cursor-default px-3 py-1 bg-primary/15" } else { "cursor-default px-3 py-1" },
                    onmousedown: move |event: Event<MouseData>| {
                        event.prevent_default();
                        let _ = send(&FileCodeActionPick { index: index as u32 });
                        titles.set(Vec::new());
                        focus_file_input();
                    },
                    "{title}"
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct RenameBox {
    line: u32,
    col: u32,
    original: String,
    draft: String,
}

impl RenameBox {
    pub(super) fn new(line: u32, col: u32, current: String) -> Self {
        Self {
            line,
            col,
            original: current.clone(),
            draft: current,
        }
    }

    pub(super) fn line(&self) -> u32 {
        self.line
    }

    pub(super) fn col(&self) -> u32 {
        self.col
    }

    fn submit(&self) {
        let name = self.draft.trim();
        if name.is_empty() || name == self.original {
            return;
        }
        let _ = send(&FileRenameRequest {
            line: self.line,
            col: self.col,
            new_name: name.to_string(),
        });
    }
}

#[component]
pub(super) fn RenameInput(state: Signal<Option<RenameBox>>, top: f64, left: f64) -> Element {
    let mut state = state;
    let Some(rename) = state() else {
        return rsx! {};
    };
    let ime = use_ime_guard();
    rsx! {
        input {
            id: RENAME_ID,
            autofocus: true,
            spellcheck: false,
            autocomplete: "off",
            class: "absolute z-50 min-w-32 rounded-md bg-background/95 px-2 py-1 text-xs text-foreground ring-1 ring-inset ring-primary/40 outline-none backdrop-blur-2xl shadow-lg",
            style: "left:{left}px;top:{top}px;",
            value: "{rename.draft}",
            oninput: move |event| {
                if let Some(open) = state.write().as_mut() {
                    open.draft = event.value();
                }
            },
            oncompositionstart: move |_| ime.start(),
            oncompositionend: move |_| ime.commit(),
            onkeydown: move |event: Event<KeyboardData>| {
                event.stop_propagation();
                if ime.swallows(&event) {
                    return;
                }
                match event.key() {
                    Key::Enter => {
                        event.prevent_default();
                        if let Some(open) = state() {
                            open.submit();
                        }
                        state.set(None);
                        focus_file_input();
                    }
                    Key::Escape => {
                        event.prevent_default();
                        state.set(None);
                        focus_file_input();
                    }
                    _ => {}
                }
            },
            onblur: move |_| state.set(None),
        }
    }
}

#[component]
pub(super) fn EditorContextMenu(
    position: Signal<Option<(f64, f64, u32, u32)>>,
    offered: Signal<Vec<EditorCapability>>,
) -> Element {
    let mut position = position;
    let Some((x, y, line, col)) = position() else {
        return rsx! {};
    };
    rsx! {
        div {
            class: "fixed inset-0 z-40",
            onmousedown: move |_| position.set(None),
            oncontextmenu: move |event| {
                event.prevent_default();
                position.set(None);
            },
        }
        div {
            class: "fixed z-50 min-w-56 overflow-hidden rounded-lg bg-foreground/[0.06] py-1 text-xs text-foreground/90 ring-1 ring-inset ring-foreground/10 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
            style: "left:{x}px;top:{y}px;",
            for (index, row) in EditorMenu::offering(&offered()).rows().into_iter().enumerate() {
                div {
                    key: "{index}",
                    class: if row.opens_group && index > 0 {
                        "mt-1 flex cursor-default items-center gap-6 border-t border-foreground/10 px-3 pt-2 pb-1.5 hover:bg-primary/15"
                    } else {
                        "flex cursor-default items-center gap-6 px-3 py-1.5 hover:bg-primary/15"
                    },
                    onmousedown: move |event: Event<MouseData>| {
                        event.prevent_default();
                        row.invoke(line, col);
                        position.set(None);
                    },
                    span { class: "grow whitespace-nowrap", {translate(row.label)} }
                    span { class: "shrink-0 text-[10px] text-foreground/40", "{row.shortcut}" }
                }
            }
        }
    }
}

#[component]
pub(super) fn ReferencesPanel(
    open: Signal<bool>,
    items: Signal<Vec<RefItem>>,
    selected: Signal<usize>,
) -> Element {
    let keys = use_context::<FileKeys>();
    let mut open = open;
    let mut selected = selected;
    if !open() {
        return rsx! {};
    }
    let rows = items();
    rsx! {
        div {
            id: "refs-panel",
            tabindex: "0",
            class: "absolute bottom-1 left-4 right-4 z-40 max-h-64 overflow-auto rounded-xl bg-foreground/[0.05] p-1 text-xs text-foreground/90 outline-none ring-1 ring-inset ring-primary/20 backdrop-blur-2xl shadow-lg dark:shadow-[0_8px_40px_-12px_rgba(0,0,0,0.7)]",
            onkeydown: move |event: Event<KeyboardData>| {
                event.stop_propagation();
                if keys.offer(&event) {
                    return;
                }
                let key = event.key().to_string();
                let len = items.read().len();
                match key.as_str() {
                    "j" => {
                        event.prevent_default();
                        if len > 0 {
                            selected.set((selected() + 1).min(len - 1));
                        }
                    }
                    "k" => {
                        event.prevent_default();
                        selected.set(selected().saturating_sub(1));
                    }
                    _ => {}
                }
            },
            div {
                class: "px-2 py-1 text-[10px] uppercase tracking-wide text-foreground/50",
                {translate_with(
                    "editor-references",
                    &[("count", TranslationValue::Number(rows.len() as i64))],
                )}
            }
            for (index, item) in rows.iter().enumerate() {
                {
                    let target = (item.path.clone(), item.line, item.col);
                    rsx! {
                        div {
                            key: "{index}",
                            class: if index == selected() { "flex gap-2 rounded bg-primary/15 px-2 py-1" } else { "flex gap-2 rounded px-2 py-1 hover:bg-foreground/[0.05]" },
                            onmousedown: move |event: Event<MouseData>| {
                                event.prevent_default();
                                let _ = send(&FileGotoRequest {
                                    path: target.0.clone(),
                                    line: target.1,
                                    col: target.2,
                                });
                                open.set(false);
                                focus_file_input();
                            },
                            span { class: "shrink-0 text-primary/80", "{item.display}" }
                            span { class: "truncate text-foreground/60", "{item.preview}" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct MenuRow {
    label: &'static str,
    shortcut: &'static str,
    operation: Option<FileEditorOperation>,
    opens_group: bool,
}

impl MenuRow {
    fn invoke(self, line: u32, col: u32) {
        match self.operation {
            Some(operation) => operation.send(line, col),
            None if self.label == "editor-go-to-definition" => {
                let _ = send(&FileDefinitionRequest { line, col });
            }
            None => {
                let _ = send(&FileReferencesRequest { line, col });
            }
        }
    }
}

struct EditorMenu {
    offered: Vec<EditorCapability>,
}

impl EditorMenu {
    fn offering(offered: &[EditorCapability]) -> Self {
        Self {
            offered: offered.to_vec(),
        }
    }

    fn rows(&self) -> Vec<MenuRow> {
        let lsp = |label, shortcut, operation, opens_group| MenuRow {
            label,
            shortcut,
            operation: Some(operation),
            opens_group,
        };
        let mut rows = vec![
            MenuRow {
                label: "editor-go-to-definition",
                shortcut: "F12",
                operation: None,
                opens_group: false,
            },
            MenuRow {
                label: "editor-find-references",
                shortcut: "⇧F12",
                operation: None,
                opens_group: false,
            },
        ];
        for (capability, operation, label, shortcut) in [
            (
                EditorCapability::GotoDeclaration,
                FileEditorOperation::GotoDeclaration,
                "editor-go-to-declaration",
                "",
            ),
            (
                EditorCapability::GotoTypeDefinition,
                FileEditorOperation::GotoTypeDefinition,
                "editor-go-to-type-definition",
                "",
            ),
            (
                EditorCapability::GotoImplementation,
                FileEditorOperation::GotoImplementation,
                "editor-go-to-implementation",
                "⌘F12",
            ),
        ] {
            if self.offered.contains(&capability) {
                rows.push(lsp(label, shortcut, operation, false));
            }
        }

        let mut modifying = Vec::new();
        if self.offered.contains(&EditorCapability::Rename) {
            modifying.push(lsp(
                "editor-rename-symbol",
                "F2",
                FileEditorOperation::Rename,
                false,
            ));
        }
        modifying.push(lsp(
            "editor-change-all-occurrences",
            "⌘F2",
            FileEditorOperation::ChangeAllOccurrences,
            false,
        ));
        if self.offered.contains(&EditorCapability::FormatDocument) {
            modifying.push(lsp(
                "editor-format-document",
                "⇧⌥F",
                FileEditorOperation::FormatDocument,
                false,
            ));
        }
        if self.offered.contains(&EditorCapability::FormatSelection) {
            modifying.push(lsp(
                "editor-format-selection",
                "",
                FileEditorOperation::FormatSelection,
                false,
            ));
        }
        if self.offered.contains(&EditorCapability::CodeAction) {
            modifying.push(lsp(
                "editor-code-action",
                "⌃⇧R",
                FileEditorOperation::CodeAction,
                false,
            ));
        }
        if let Some(first) = modifying.first_mut() {
            first.opens_group = true;
        }
        rows.append(&mut modifying);

        rows.push(lsp("editor-cut", "⌘X", FileEditorOperation::Cut, true));
        rows.push(lsp("editor-copy", "⌘C", FileEditorOperation::Copy, false));
        rows.push(lsp("editor-paste", "⌘V", FileEditorOperation::Paste, false));
        rows.push(lsp(
            "editor-command-palette",
            "⇧⌘P",
            FileEditorOperation::CommandPalette,
            true,
        ));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl EditorMenu {
        fn labels(offered: &[EditorCapability]) -> Vec<&'static str> {
            Self::offering(offered)
                .rows()
                .into_iter()
                .map(|row| row.label)
                .collect()
        }
    }

    #[test]
    fn a_file_without_a_server_keeps_only_the_rows_needing_none() {
        let rows = EditorMenu::labels(&[]);
        assert!(rows.contains(&"editor-cut"));
        assert!(rows.contains(&"editor-change-all-occurrences"));
        assert!(!rows.contains(&"editor-rename-symbol"));
        assert!(!rows.contains(&"editor-format-document"));
    }

    #[test]
    fn a_row_appears_exactly_when_its_server_offers_it() {
        let rows = EditorMenu::labels(&[
            EditorCapability::Rename,
            EditorCapability::GotoImplementation,
        ]);
        assert!(rows.contains(&"editor-rename-symbol"));
        assert!(rows.contains(&"editor-go-to-implementation"));
        assert!(!rows.contains(&"editor-go-to-declaration"));
        assert!(!rows.contains(&"editor-format-selection"));
    }

    #[test]
    fn each_group_opens_exactly_once() {
        let all = [
            EditorCapability::GotoDeclaration,
            EditorCapability::GotoTypeDefinition,
            EditorCapability::GotoImplementation,
            EditorCapability::Rename,
            EditorCapability::FormatDocument,
            EditorCapability::FormatSelection,
        ];
        let opens = EditorMenu::offering(&all)
            .rows()
            .into_iter()
            .filter(|row| row.opens_group)
            .count();
        assert_eq!(opens, 3);
    }
}
