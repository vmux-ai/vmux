use crate::explorer::{SEARCH_INPUT_ID, SidebarView};
use crate::ui::{ExplorerPane, Mode};
use dioxus::prelude::*;
use vmux_core::event::{FileKey, FilePanelState};
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{KeyClaim, send, use_key_claim};
use vmux_ui::platform::sleep_ms;

pub(crate) fn use_file_keys(page: FilePage) -> FileKeys {
    let handler = FileKeyHandler(page);
    let events = crate::state::use_file_ui::<FileKey>();
    use_effect(move || events.for_each(|key| handler.apply(key)));
    let keys = FileKeys {
        claim: use_key_claim(Unclaimed::Types, move || page.key_context()),
    };
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });
    keys
}

#[derive(Clone, Copy)]
pub struct FileKeys {
    claim: KeyClaim,
}

impl FileKeys {
    pub fn offer(&self, event: &Event<KeyboardData>) -> bool {
        self.claim.on_keydown(event, |_| false);
        !event.default_action_enabled()
    }
}

#[derive(Clone, Copy)]
struct FileKeyHandler(FilePage);

impl FileKeyHandler {
    fn apply(&self, key: FileKey) {
        match key {
            FileKey::ToggleExplorer => self.0.toggle_explorer(),
            FileKey::RevealInExplorer => self.0.reveal_in_explorer(),
            FileKey::Find { forward } => self.0.open_find(forward),
            FileKey::FindClose => self.0.close_find(),
            FileKey::FindInFiles => self.0.open_find_in_files(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FilePage {
    pub mode: Signal<Mode>,
    pub explorer: ExplorerPane,
    pub panel: Signal<FilePanelState>,
    pub find_open: Signal<bool>,
    pub find_forward: Signal<bool>,
    pub sidebar_view: Signal<SidebarView>,
}

impl FilePage {
    fn key_context(&self) -> Vec<String> {
        let mut keys = vec!["files".to_string()];
        if (self.panel)().content.is_some() {
            keys.push("files.panel".to_string());
        }
        keys
    }

    fn toggle_explorer(&self) {
        self.explorer.toggle(self.mode);
    }

    fn reveal_in_explorer(&self) {
        let mut view = self.sidebar_view;
        view.set(SidebarView::Explorer);
        self.explorer.reveal_current(self.mode);
    }

    fn close_find(&self) {
        let mut open = self.find_open;
        open.set(false);
    }

    fn open_find(&self, forward: bool) {
        let mut open = self.find_open;
        let mut direction = self.find_forward;
        direction.set(forward);
        open.set(true);
        spawn(async move {
            sleep_ms(0).await;
            crate::ui::focus_find_input();
        });
    }

    fn open_find_in_files(&self) {
        let mut view = self.sidebar_view;
        view.set(SidebarView::Search);
        self.explorer.show(self.mode);
        spawn(async move {
            sleep_ms(0).await;
            FocusClaim::new(SEARCH_INPUT_ID).request();
        });
    }
}
