use crate::{
    GalleryCheckedRequest, GalleryClickRequest, GalleryCursorRequest, GalleryTextRequest,
    GalleryUiState, Story,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use std::collections::HashMap;
use vmux_ui::back::{BackButton, PageBack};
use vmux_ui::directory::{DirectoryEntryVisual, DirectoryNavigator};
use vmux_ui::favicon::{Favicon, GlobeIcon};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::matrix_rain::{MatrixLoader, MatrixRain};
use vmux_ui::prompt_ghost::PromptGhost;

#[component]
pub(super) fn AuxiliaryPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    use_context_provider(move || {
        PageBack::new(EventHandler::new(move |_| {
            let _ = send(&GalleryClickRequest {
                revision,
                label: "BackButton".into(),
            });
        }))
    });
    let count = state.examples.files.len().max(1);
    rsx! {
        fieldset { disabled: state.disabled, class: "space-y-6",
            match state.story {
                Story::Favicon => rsx! {
                    div { class: "flex items-center gap-6",
                        Favicon { favicon_url: "assets/agent-logos/claude.png", url: "", class: "size-8" }
                        Favicon { favicon_url: "", url: "about:blank", class: "size-8", globe_class: "size-8" }
                        GlobeIcon { class: "size-8" }
                    }
                },
                Story::TypeIcon => rsx! {
                    div { class: "flex items-center gap-6",
                        for path in ["example.rs", "Cargo.toml", "src"] {
                            TypeIcon { path, is_dir: path == "src", class: "size-8" }
                        }
                    }
                },
                Story::PromptGhost => rsx! {
                    div { class: "max-w-xl space-y-8 overflow-hidden",
                        PromptGhost { accent_bg: "bg-primary/10", terminal: false }
                        PromptGhost { accent_bg: "bg-primary/10", terminal: true }
                    }
                },
                Story::MatrixRain => rsx! {
                    div { class: "relative h-56 overflow-hidden rounded-lg",
                        MatrixRain { accent_rgb: "var(--primary)", words: vec!["VMUX".into(), "UI".into()], columns: 24 }
                    }
                    MatrixLoader { label: translate("gallery-sample"), class: "h-56 w-full", columns: 24 }
                },
                Story::BackButton => rsx! { BackButton {} },
                Story::DirectoryNavigator => rsx! {
                    div { class: "flex h-80 overflow-auto rounded-lg bg-foreground/[0.025]",
                        DirectoryNavigator {
                            path: if state.text.is_empty() { "/example".into() } else { state.text.clone() },
                            parent_entries: state.examples.files.clone(),
                            entries: state.examples.files.clone(),
                            children: Some(state.examples.files.clone()),
                            selected: state.cursor,
                            thumbs: HashMap::new(),
                            preview: rsx! { {translate("gallery-sample")} },
                            on_select: move |(cursor, entry): (usize, vmux_ecs::event::FileDirEntry)| {
                                let _ = send(&GalleryCursorRequest { revision, cursor });
                                let _ = send(&GalleryClickRequest { revision, label: format!("DirectoryNavigator.select:{}", entry.name) });
                            },
                            on_ascend: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                            on_descend: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                            on_open: move |entry: vmux_ecs::event::FileDirEntry| { let _ = send(&GalleryClickRequest { revision, label: format!("DirectoryNavigator.open:{}", entry.name) }); },
                            on_next: move |_| { let _ = send(&GalleryCursorRequest { revision, cursor: (state.cursor + 1) % count }); },
                            on_previous: move |_| { let _ = send(&GalleryCursorRequest { revision, cursor: (state.cursor + count - 1) % count }); },
                            on_activate: move |_| { let _ = send(&GalleryClickRequest { revision, label: "DirectoryNavigator.activate".into() }); },
                            on_parent: move |_| { let _ = send(&GalleryTextRequest { revision, text: "/example".into() }); },
                            on_toggle_hidden: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                        }
                    }
                    div { class: "flex gap-4",
                        for entry in state.examples.files.clone() {
                            DirectoryEntryVisual { entry, thumb: None }
                        }
                    }
                },
                _ => rsx! {},
            }
        }
    }
}
