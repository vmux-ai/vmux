#![allow(non_snake_case)]

use dioxus::core::Task;
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize};
use vmux_ui::components::checkbox::{Checkbox, CheckboxTone};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::hooks::send;
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::{LineIcon, LineIconView};
use vmux_ui::platform::Platform;

use crate::event::{
    GitReviewNoteDeleteRequest, GitReviewNoteEditRequest, GitReviewNoteSyncRequest,
    GitReviewSubmitRequest,
};
use crate::state::{GitReviewLocation, GitReviewNote, GitReviewTarget};

use super::state::GitPageState;

const NOTE_EDIT_DEBOUNCE_MS: u32 = 150;

#[component]
pub(super) fn ReviewPanel(target: GitReviewTarget, generation: u64) -> Element {
    let state = use_context::<GitPageState>();
    let selected_target = target.clone();
    let review = (state.review)();
    let notes = review
        .target
        .as_ref()
        .filter(|current| current.same_selection(&target))
        .map(|_| review.notes)
        .unwrap_or_default();
    let github = review.providers.iter().any(|provider| provider == "github");
    if notes.is_empty() {
        return rsx! {};
    }
    let can_submit =
        !(state.diff.loading)() && notes.iter().any(|note| !note.body.trim().is_empty());
    rsx! {
        aside { class: "absolute bottom-4 right-4 z-40 flex max-h-[min(34rem,calc(100%-2rem))] w-[min(24rem,calc(100%-2rem))] flex-col overflow-hidden rounded-xl border border-foreground/15 bg-background/90 shadow-2xl backdrop-blur-2xl",
            div { class: "flex h-10 shrink-0 items-center gap-2 border-b border-foreground/10 px-3",
                LineIconView { icon: LineIcon::MessagesRound, class: "size-3.5 text-muted-foreground" }
                span { class: "min-w-0 flex-1 truncate text-xs font-semibold", {translate("git-review-title")} }
                span { class: "text-[10px] tabular-nums text-muted-foreground", "{notes.len()}" }
            }
            div { class: "min-h-0 flex-1 space-y-2 overflow-y-auto p-2",
                for note in notes {
                    ReviewNoteCard {
                        key: "{note.id}",
                        note,
                        target: selected_target.clone(),
                        generation,
                        github,
                    }
                }
            }
            div { class: "flex shrink-0 justify-end border-t border-foreground/10 p-2",
                Button {
                    size: ButtonSize::Sm,
                    disabled: !can_submit,
                    onclick: move |_| {
                        let _ = send(&GitReviewSubmitRequest);
                    },
                    LineIconView { icon: LineIcon::ArrowUp, class: "size-3.5" }
                    {translate("git-review-submit")}
                }
            }
        }
    }
}

#[component]
fn ReviewNoteCard(
    note: GitReviewNote,
    target: GitReviewTarget,
    generation: u64,
    github: bool,
) -> Element {
    let mut draft = use_signal(|| note.body.clone());
    let mut pending = use_signal(|| None::<Task>);
    let drop_id = note.id.clone();
    use_drop(move || {
        if let Some(task) = pending.write().take() {
            task.cancel();
            let _ = send(&GitReviewNoteEditRequest {
                id: drop_id,
                body: draft(),
            });
        }
    });
    let stale = note.anchor.is_stale(&target, generation);
    let can_sync = github
        && target.supports_external_review()
        && matches!(note.anchor.location, GitReviewLocation::Line { .. })
        && !stale;
    let input_id = note.id.clone();
    let blur_id = note.id.clone();
    let sync_id = note.id.clone();
    let delete_id = note.id.clone();
    let location = match note.anchor.location {
        GitReviewLocation::Line { side, line } => translate_with(
            "git-review-location-line",
            &[
                ("side", TranslationValue::String(side.label())),
                ("line", TranslationValue::Number(i64::from(line))),
            ],
        ),
        GitReviewLocation::File => translate("git-review-location-file"),
    };
    rsx! {
        article { class: "rounded-lg bg-foreground/[0.045] p-2 ring-1 ring-inset ring-foreground/10",
            div { class: "mb-1.5 flex items-center gap-2",
                div { class: "min-w-0 flex-1",
                    div { class: "truncate font-mono text-[10px] font-semibold", "{note.anchor.path}" }
                    div { class: if stale { "text-[9px] text-destructive" } else { "text-[9px] text-muted-foreground" },
                        "{location}"
                        if stale { " · " {translate("git-review-stale")} }
                    }
                }
                button {
                    r#type: "button",
                    class: "flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive",
                    title: translate("common-delete"),
                    onclick: move |_| {
                        let _ = send(&GitReviewNoteDeleteRequest { id: delete_id.clone() });
                    },
                    LineIconView { icon: LineIcon::Trash, class: "size-3.5" }
                }
            }
            Textarea {
                variant: TextareaVariant::Ghost,
                class: "min-h-20 resize-y rounded-md bg-background/60 px-2 py-1.5 text-xs text-foreground",
                value: draft(),
                placeholder: translate("git-review-placeholder"),
                oninput: move |event: FormEvent| {
                    let body = event.value();
                    draft.set(body.clone());
                    if let Some(task) = pending.write().take() {
                        task.cancel();
                    }
                    let id = input_id.clone();
                    pending.set(Some(spawn(async move {
                        Platform::sleep(NOTE_EDIT_DEBOUNCE_MS).await;
                        let _ = send(&GitReviewNoteEditRequest { id, body });
                    })));
                },
                onblur: move |_| {
                    if let Some(task) = pending.write().take() {
                        task.cancel();
                    }
                    let _ = send(&GitReviewNoteEditRequest {
                        id: blur_id.clone(),
                        body: draft(),
                    });
                },
            }
            label { class: if can_sync { "mt-2 flex cursor-pointer items-center gap-2 text-[10px] text-muted-foreground" } else { "mt-2 flex cursor-not-allowed items-center gap-2 text-[10px] text-muted-foreground/50" },
                Checkbox {
                    checked: note.publish_to_provider,
                    disabled: !can_sync,
                    tone: CheckboxTone::Neutral,
                    aria_label: translate("git-review-sync-github"),
                    on_checked_change: move |enabled| {
                        let _ = send(&GitReviewNoteSyncRequest {
                            id: sync_id.clone(),
                            enabled,
                        });
                    },
                }
                {translate("git-review-sync-github")}
            }
        }
    }
}
