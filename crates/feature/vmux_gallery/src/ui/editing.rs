use crate::{GalleryClickRequest, GalleryTextRequest, GalleryUiState};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::inline_edit::{EditableText, InlineEdit, InlineTextarea};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn EditingPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let mut editing = use_signal(|| false);
    let draft = use_signal(|| state.text.clone());
    rsx! {
        fieldset { disabled: state.disabled, class: "mx-auto max-w-xl space-y-8",
            div { class: "space-y-3",
                p { class: "font-mono text-xs text-muted-foreground", "EditableText" }
                EditableText {
                    value: state.text.clone(), editing, draft,
                    display_class: "block min-h-8 cursor-text rounded-md bg-foreground/5 p-2",
                    input_class: "w-full rounded-md bg-background p-2 outline-none",
                    title: "EditableText", empty_display: translate("gallery-sample"),
                    allow_empty: true,
                    on_commit: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                }
            }
            div { class: "space-y-3",
                p { class: "font-mono text-xs text-muted-foreground", "InlineEdit / InlineTextarea" }
                Button { variant: ButtonVariant::Secondary, onclick: move |_| editing.set(!editing()), "editing: {editing}" }
                if editing() {
                    InlineEdit { draft, class: "w-full rounded-md bg-background p-2", aria_label: "InlineEdit", allow_empty: true,
                        on_commit: move |text| { let _ = send(&GalleryTextRequest { revision, text }); editing.set(false); },
                        on_cancel: move |_| { editing.set(false); let _ = send(&GalleryClickRequest { revision, label: "InlineEdit.cancel".into() }); },
                    }
                    InlineTextarea { draft, class: "w-full rounded-md bg-background p-2", aria_label: "InlineTextarea", allow_empty: true,
                        on_commit: move |text| { let _ = send(&GalleryTextRequest { revision, text }); editing.set(false); },
                        on_cancel: move |_| { editing.set(false); let _ = send(&GalleryClickRequest { revision, label: "InlineTextarea.cancel".into() }); },
                    }
                }
            }
        }
    }
}
