#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::{JsCast, closure::Closure};

use crate::event::{
    NOTE_CREATED_EVENT, NOTE_ERROR_EVENT, NOTE_READ_RESPONSE_EVENT, NOTE_WRITTEN_EVENT,
    NOTES_QUERY_RESPONSE_EVENT, NoteCreateRequest, NoteCreatedEvent, NoteErrorEvent,
    NoteOpenRequest, NoteOperation, NoteReadRequest, NoteReadResponse, NoteSummary,
    NoteWriteRequest, NoteWrittenEvent, NotesQueryRequest, NotesQueryResponse,
};

const QUERY_PAGE_SIZE: u32 = 100;
const KNOWLEDGE_USE_CASES: [(&str, &str); 8] = [
    ("Skills", "New Skill"),
    ("Decisions", "Decision Record"),
    ("Runbooks", "Runbook"),
    ("Projects", "Project Brief"),
    ("Meetings", "Meeting Notes"),
    ("Handbook", "Handbook Page"),
    ("Research", "Research Note"),
    ("Templates", "Template"),
];

fn emit_query(
    query: &str,
    request_id: u64,
    offset: u32,
) -> Result<(), vmux_ui::hooks::EventListenerError> {
    try_cef_bin_emit_rkyv(&NotesQueryRequest {
        query: query.to_string(),
        request_id,
        offset,
        limit: QUERY_PAGE_SIZE,
    })
}

fn request_read(
    path: &str,
    mut latest_read: Signal<u64>,
    mut preview_loading: Signal<bool>,
    mut preview_error: Signal<String>,
) {
    let request_id = latest_read().wrapping_add(1);
    latest_read.set(request_id);
    preview_loading.set(true);
    preview_error.set(String::new());
    if let Err(error) = try_cef_bin_emit_rkyv(&NoteReadRequest {
        path: path.to_string(),
        request_id,
    }) {
        preview_loading.set(false);
        preview_error.set(error.to_string());
    }
}

fn request_open(path: &str, mut latest_open: Signal<u64>, mut toast_error: Signal<String>) {
    let request_id = latest_open().wrapping_add(1);
    latest_open.set(request_id);
    toast_error.set(String::new());
    if let Err(error) = try_cef_bin_emit_rkyv(&NoteOpenRequest {
        path: path.to_string(),
        request_id,
    }) {
        toast_error.set(error.to_string());
    }
}

fn submit_create(
    title: String,
    mut create_pending: Signal<bool>,
    mut create_error: Signal<String>,
    mut latest_create: Signal<u64>,
) {
    if create_pending() {
        return;
    }
    let request_id = latest_create().wrapping_add(1);
    latest_create.set(request_id);
    create_pending.set(true);
    create_error.set(String::new());
    if let Err(error) = try_cef_bin_emit_rkyv(&NoteCreateRequest { title, request_id }) {
        create_pending.set(false);
        create_error.set(error.to_string());
    }
}

fn emit_write(
    path: String,
    source: String,
    request_id: u64,
    mut write_pending: Signal<bool>,
    mut write_error: Signal<String>,
) {
    write_pending.set(true);
    write_error.set(String::new());
    if let Err(error) = try_cef_bin_emit_rkyv(&NoteWriteRequest {
        path,
        source,
        request_id,
    }) {
        write_pending.set(false);
        write_error.set(error.to_string());
    }
}

fn submit_write(
    path: String,
    source: String,
    mut latest_write: Signal<u64>,
    write_pending: Signal<bool>,
    write_error: Signal<String>,
) {
    let request_id = latest_write().wrapping_add(1);
    latest_write.set(request_id);
    emit_write(path, source, request_id, write_pending, write_error);
}

fn schedule_write(
    path: String,
    source: String,
    mut generation: Signal<u32>,
    mut latest_write: Signal<u64>,
    mut write_pending: Signal<bool>,
    write_error: Signal<String>,
) {
    let current = generation.peek().wrapping_add(1);
    generation.set(current);
    let request_id = latest_write().wrapping_add(1);
    latest_write.set(request_id);
    write_pending.set(true);
    let Some(window) = web_sys::window() else {
        emit_write(path, source, request_id, write_pending, write_error);
        return;
    };
    let closure = Closure::once(move || {
        if *generation.peek() == current {
            emit_write(path, source, request_id, write_pending, write_error);
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        450,
    );
    closure.forget();
}

fn schedule_query(
    value: String,
    mut generation: Signal<u32>,
    mut latest_request: Signal<u64>,
    mut query_error: Signal<String>,
) {
    let current = generation.peek().wrapping_add(1);
    generation.set(current);
    let Some(window) = web_sys::window() else {
        let next = *latest_request.peek() + 1;
        latest_request.set(next);
        if let Err(error) = emit_query(&value, next, 0) {
            query_error.set(error.to_string());
        }
        return;
    };
    let closure = Closure::once(move || {
        if *generation.peek() != current {
            return;
        }
        let next = *latest_request.peek() + 1;
        latest_request.set(next);
        if let Err(error) = emit_query(&value, next, 0) {
            query_error.set(error.to_string());
        }
    });
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        180,
    );
    closure.forget();
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut notes = use_signal(Vec::<NoteSummary>::new);
    let mut query = use_signal(String::new);
    let mut latest_request = use_signal(|| 1_u64);
    let latest_read = use_signal(|| 0_u64);
    let latest_create = use_signal(|| 0_u64);
    let latest_open = use_signal(|| 0_u64);
    let latest_write = use_signal(|| 0_u64);
    let search_generation = use_signal(|| 0_u32);
    let mut write_generation = use_signal(|| 0_u32);
    let mut vault_path = use_signal(String::new);
    let mut selected_path = use_signal(String::new);
    let mut preview = use_signal(|| None::<NoteReadResponse>);
    let mut preview_loading = use_signal(|| false);
    let mut preview_error = use_signal(String::new);
    let mut draft = use_signal(String::new);
    let mut editing = use_signal(|| false);
    let mut write_pending = use_signal(|| false);
    let mut write_error = use_signal(String::new);
    let mut edit_after_read = use_signal(|| false);
    let mut total = use_signal(|| 0_u32);
    let mut has_more = use_signal(|| false);
    let mut create_open = use_signal(|| false);
    let mut create_title = use_signal(String::new);
    let mut create_pending = use_signal(|| false);
    let mut create_error = use_signal(String::new);
    let mut query_error = use_signal(String::new);
    let mut toast_error = use_signal(String::new);

    let _query_listener = use_bin_event_listener::<NotesQueryResponse, _>(
        NOTES_QUERY_RESPONSE_EVENT,
        move |response: NotesQueryResponse| {
            if response.request_id != *latest_request.peek() {
                return;
            }
            query_error.set(String::new());
            vault_path.set(response.vault_path);
            total.set(response.total);
            has_more.set(response.has_more);
            if response.offset > 0 {
                notes.write().extend(response.notes);
                return;
            }
            let current = selected_path.peek().clone();
            let next = response
                .notes
                .iter()
                .find(|note| note.path == current)
                .or_else(|| response.notes.first())
                .map(|note| note.path.clone())
                .unwrap_or_default();
            notes.set(response.notes);
            if next.is_empty() {
                selected_path.set(String::new());
                preview.set(None);
                preview_loading.set(false);
                preview_error.set(String::new());
            } else if next != current || preview.peek().is_none() {
                selected_path.set(next.clone());
                preview.set(None);
                request_read(&next, latest_read, preview_loading, preview_error);
            }
        },
    );

    let _read_listener = use_bin_event_listener::<NoteReadResponse, _>(
        NOTE_READ_RESPONSE_EVENT,
        move |response: NoteReadResponse| {
            if response.request_id == *latest_read.peek() && response.path == *selected_path.peek()
            {
                preview_loading.set(false);
                preview_error.set(String::new());
                draft.set(response.source.clone());
                editing.set(edit_after_read());
                edit_after_read.set(false);
                write_pending.set(false);
                write_error.set(String::new());
                preview.set(Some(response));
            }
        },
    );

    let _written_listener = use_bin_event_listener::<NoteWrittenEvent, _>(
        NOTE_WRITTEN_EVENT,
        move |event: NoteWrittenEvent| {
            if event.request_id == *latest_write.peek() && event.note.path == *selected_path.peek()
            {
                write_pending.set(false);
                write_error.set(String::new());
                preview.set(Some(event.note));
                let next = *latest_request.peek() + 1;
                latest_request.set(next);
                if let Err(error) = emit_query(&query.peek(), next, 0) {
                    query_error.set(error.to_string());
                }
            }
        },
    );

    let _created_listener = use_bin_event_listener::<NoteCreatedEvent, _>(
        NOTE_CREATED_EVENT,
        move |created: NoteCreatedEvent| {
            if created.request_id != *latest_create.peek() {
                return;
            }
            create_pending.set(false);
            create_open.set(false);
            create_error.set(String::new());
            create_title.set(String::new());
            selected_path.set(created.note.path.clone());
            edit_after_read.set(true);
            preview.set(None);
            request_read(
                &created.note.path,
                latest_read,
                preview_loading,
                preview_error,
            );
            let next = *latest_request.peek() + 1;
            latest_request.set(next);
            if let Err(error) = emit_query(&query.peek(), next, 0) {
                query_error.set(error.to_string());
            }
        },
    );

    let _error_listener =
        use_bin_event_listener::<NoteErrorEvent, _>(NOTE_ERROR_EVENT, move |event| {
            match event.operation {
                NoteOperation::Query if event.request_id == *latest_request.peek() => {
                    query_error.set(event.message);
                }
                NoteOperation::Read
                    if event.request_id == *latest_read.peek()
                        && event.path == *selected_path.peek() =>
                {
                    preview_loading.set(false);
                    preview.set(None);
                    preview_error.set(event.message);
                }
                NoteOperation::Create if event.request_id == *latest_create.peek() => {
                    create_pending.set(false);
                    create_error.set(event.message);
                }
                NoteOperation::Write if event.request_id == *latest_write.peek() => {
                    write_pending.set(false);
                    write_error.set(event.message);
                }
                NoteOperation::Open if event.request_id == *latest_open.peek() => {
                    toast_error.set(event.message);
                }
                _ => {}
            }
        });

    use_effect(move || {
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.set_title("Knowledge");
        }
        let _ = emit_query("", 1, 0);
    });

    rsx! {
        style { dangerous_inner_html: KNOWLEDGE_CSS }
        div { class: "flex h-screen min-h-0 bg-background text-foreground",
            aside { class: "flex w-[220px] shrink-0 flex-col border-r border-sidebar-border bg-sidebar/80",
                div { class: "flex items-center gap-2.5 px-4 pb-4 pt-5",
                    div { class: "grid h-8 w-8 place-items-center rounded-xl bg-primary/15 text-primary ring-1 ring-inset ring-primary/25",
                        Icon { class: "h-4 w-4",
                            path { d: "M2 4a2 2 0 0 1 2-2h6a4 4 0 0 1 4 4v16a4 4 0 0 0-4-4H2Z" }
                            path { d: "M22 4a2 2 0 0 0-2-2h-6a4 4 0 0 0-4 4v16a4 4 0 0 1 4-4h8Z" }
                        }
                    }
                    div {
                        div { class: "text-sm font-semibold tracking-tight", "Knowledge" }
                        div { class: "text-[10px] text-muted-foreground", "Local Markdown vault" }
                    }
                }
                div { class: "px-3",
                    button {
                        class: "flex w-full items-center justify-center gap-2 rounded-lg bg-primary px-3 py-2 text-xs font-semibold text-primary-foreground shadow-sm transition-opacity hover:opacity-90",
                        onclick: move |_| {
                            create_error.set(String::new());
                            create_open.set(true);
                        },
                        Icon { class: "h-3.5 w-3.5", path { d: "M12 5v14" } path { d: "M5 12h14" } }
                        "New note"
                    }
                }
                nav { class: "mt-5 px-2",
                    div { class: "px-2 pb-1.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground/70", "Library" }
                    button { class: "flex w-full items-center gap-2 rounded-lg bg-sidebar-accent px-2.5 py-2 text-xs font-medium text-sidebar-accent-foreground",
                        Icon { class: "h-3.5 w-3.5", path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" } path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z" } }
                        span { class: "flex-1 text-left", "All notes" }
                        span { class: "rounded-full bg-foreground/[0.07] px-1.5 py-0.5 text-[9px] tabular-nums text-muted-foreground", "{total}" }
                    }
                    div { class: "px-2 pb-1.5 pt-5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground/70", "Build with" }
                    div { class: "grid grid-cols-2 gap-1",
                        for (label, seed) in KNOWLEDGE_USE_CASES {
                            button {
                                key: "{label}",
                                class: "truncate rounded-md px-2 py-1.5 text-left text-[10px] text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                                title: "Create {label}",
                                onclick: move |_| {
                                    create_title.set(seed.to_string());
                                    create_error.set(String::new());
                                    create_open.set(true);
                                },
                                "{label}"
                            }
                        }
                    }
                }
                div { class: "mt-auto border-t border-sidebar-border px-4 py-4",
                    div { class: "mb-1 flex items-center gap-1.5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/70",
                        Icon { class: "h-3 w-3", path { d: "M3 6h18" } path { d: "M3 12h18" } path { d: "M3 18h18" } }
                        "Vault"
                    }
                    div { class: "truncate text-[10px] leading-relaxed text-muted-foreground", title: "{vault_path}", "{compact_path(&vault_path())}" }
                }
            }

            section { class: "flex w-[340px] min-w-[280px] shrink-0 flex-col border-r border-border bg-background/70",
                header { class: "border-b border-border px-3 py-3",
                    div { class: "flex items-center gap-2",
                        div { class: "relative min-w-0 flex-1",
                            Icon { class: "pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground",
                                circle { cx: "11", cy: "11", r: "8" }
                                path { d: "m21 21-4.3-4.3" }
                            }
                            input {
                                class: "h-9 w-full rounded-lg border border-border bg-foreground/[0.035] pl-9 pr-3 text-xs outline-none transition-colors placeholder:text-muted-foreground/70 focus:border-ring focus:bg-background focus:ring-1 focus:ring-ring/30",
                                placeholder: "Search notes and contents…",
                                value: "{query}",
                                oninput: move |event| {
                                    let value = event.value();
                                    query.set(value.clone());
                                    schedule_query(
                                        value,
                                        search_generation,
                                        latest_request,
                                        query_error,
                                    );
                                },
                            }
                        }
                        button {
                            class: "grid h-9 w-9 shrink-0 place-items-center rounded-lg border border-border text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground",
                            title: "Refresh notes",
                            onclick: move |_| {
                                let next = *latest_request.peek() + 1;
                                latest_request.set(next);
                                if let Err(error) = emit_query(&query.peek(), next, 0) {
                                    query_error.set(error.to_string());
                                }
                                let selected = selected_path.peek().clone();
                                if !selected.is_empty() {
                                    preview.set(None);
                                    request_read(
                                        &selected,
                                        latest_read,
                                        preview_loading,
                                        preview_error,
                                    );
                                }
                            },
                            Icon { class: "h-3.5 w-3.5", path { d: "M20 6v6h-6" } path { d: "M4 18v-6h6" } path { d: "M18.5 9A7 7 0 0 0 6 5.5L4 8" } path { d: "M5.5 15A7 7 0 0 0 18 18.5l2-2.5" } }
                        }
                    }
                    div { class: "mt-2 flex items-center justify-between px-0.5 text-[10px] text-muted-foreground",
                        span { if query().is_empty() { "Recently changed" } else { "Search results" } }
                        span { "{total} notes" }
                    }
                }
                div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                    if notes().is_empty() {
                        div { class: "grid h-full place-items-center px-6 text-center",
                            div {
                                div { class: "mx-auto mb-3 grid h-10 w-10 place-items-center rounded-2xl bg-foreground/[0.04] text-muted-foreground",
                                    Icon { class: "h-5 w-5", path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" } path { d: "M14 2v6h6" } path { d: "M9 13h6" } path { d: "M9 17h3" } }
                                }
                                div { class: "text-xs font-medium",
                                    if !query_error().is_empty() { "Could not load notes" } else if query().is_empty() { "No notes yet" } else { "Nothing found" }
                                }
                                div { class: "mt-1 text-[10px] leading-relaxed text-muted-foreground",
                                    if !query_error().is_empty() { "{query_error}" } else if query().is_empty() { "Create your first Markdown note." } else { "Try a different search." }
                                }
                            }
                        }
                    } else {
                        for note in notes() {
                            {
                                let selected = note.path == selected_path();
                                let path = note.path.clone();
                                let open_path = note.path.clone();
                                rsx! {
                                    button {
                                        key: "{note.path}",
                                        class: if selected {
                                            "mb-1.5 w-full rounded-xl bg-accent px-3 py-3 text-left text-accent-foreground ring-1 ring-inset ring-ring/25"
                                        } else {
                                            "mb-1.5 w-full rounded-xl px-3 py-3 text-left transition-colors hover:bg-foreground/[0.045]"
                                        },
                                        onclick: move |_| {
                                            selected_path.set(path.clone());
                                            preview.set(None);
                                            request_read(
                                                &path,
                                                latest_read,
                                                preview_loading,
                                                preview_error,
                                            );
                                        },
                                        ondoubleclick: move |_| request_open(&open_path, latest_open, toast_error),
                                        div { class: "flex items-start gap-2.5",
                                            div { class: if selected { "mt-0.5 text-primary" } else { "mt-0.5 text-muted-foreground/70" },
                                                Icon { class: "h-3.5 w-3.5", path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" } path { d: "M14 2v6h6" } }
                                            }
                                            div { class: "min-w-0 flex-1",
                                                div { class: "truncate text-xs font-semibold", "{note.title}" }
                                                if !note.excerpt.is_empty() {
                                                    div { class: "mt-1 line-clamp-2 text-[10px] leading-relaxed text-muted-foreground", "{note.excerpt}" }
                                                }
                                                div { class: "mt-2 flex items-center gap-2 text-[9px] text-muted-foreground/75",
                                                    span { class: "min-w-0 flex-1 truncate", "{note.relative_path}" }
                                                    span { class: "shrink-0", "{modified_label(note.modified_at)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if has_more() {
                            button {
                                class: "mb-2 mt-1 w-full rounded-lg border border-border px-3 py-2 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.04] hover:text-foreground",
                                onclick: move |_| {
                                    let next = *latest_request.peek() + 1;
                                    latest_request.set(next);
                                    if let Err(error) = emit_query(
                                        &query.peek(),
                                        next,
                                        notes.peek().len() as u32,
                                    ) {
                                        query_error.set(error.to_string());
                                    }
                                },
                                "Load more"
                            }
                        }
                    }
                }
            }

            main { class: "min-w-0 flex-1 overflow-y-auto bg-background",
                if let Some(note) = preview() {
                    div { class: "mx-auto w-full max-w-[820px] px-10 pb-24 pt-9",
                        div { class: "mb-8 flex items-start gap-6 border-b border-border pb-5",
                            div { class: "min-w-0 flex-1",
                                div { class: "mb-2 flex items-center gap-2 text-[10px] text-muted-foreground",
                                    span { class: "truncate", "{note.relative_path}" }
                                    span { "·" }
                                    span { class: "shrink-0", "{note.word_count} words" }
                                    span { "·" }
                                    span { class: "shrink-0", "{modified_label(note.modified_at)}" }
                                }
                                h1 { class: "truncate text-[28px] font-semibold tracking-[-0.025em]", "{note.title}" }
                            }
                            div { class: "mt-1 shrink-0 text-[10px] text-muted-foreground",
                                if write_pending() {
                                    "Saving…"
                                } else if !write_error().is_empty() {
                                    span { class: "text-destructive", "Save failed" }
                                } else {
                                    "Saved"
                                }
                            }
                        }
                        if editing() {
                            textarea {
                                id: "knowledge-note-editor",
                                class: "min-h-[65vh] w-full resize-none border-0 bg-transparent font-mono text-sm leading-7 text-foreground outline-none placeholder:text-muted-foreground",
                                autofocus: true,
                                spellcheck: "true",
                                value: "{draft}",
                                oninput: {
                                    let path = note.path.clone();
                                    move |event| {
                                        let source = event.value();
                                        draft.set(source.clone());
                                        schedule_write(
                                            path.clone(),
                                            source,
                                            write_generation,
                                            latest_write,
                                            write_pending,
                                            write_error,
                                        );
                                    }
                                },
                                onblur: {
                                    let path = note.path.clone();
                                    let saved_source = note.source.clone();
                                    move |_| {
                                        if draft() != saved_source {
                                            write_generation
                                                .set(write_generation().wrapping_add(1));
                                            submit_write(
                                                path.clone(),
                                                draft(),
                                                latest_write,
                                                write_pending,
                                                write_error,
                                            );
                                        }
                                        editing.set(false);
                                    }
                                },
                                onkeydown: {
                                    let path = note.path.clone();
                                    move |event: Event<KeyboardData>| {
                                        let data = event.data();
                                        let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
                                            return;
                                        };
                                        if raw.key() == "Escape" {
                                            event.prevent_default();
                                            editing.set(false);
                                        } else if raw.key().eq_ignore_ascii_case("s")
                                            && (raw.meta_key() || raw.ctrl_key())
                                        {
                                            event.prevent_default();
                                            write_generation.set(write_generation().wrapping_add(1));
                                            submit_write(
                                                path.clone(),
                                                draft(),
                                                latest_write,
                                                write_pending,
                                                write_error,
                                            );
                                        }
                                    }
                                },
                            }
                        } else {
                            article {
                                class: "knowledge-md min-h-[40vh] cursor-text rounded-xl px-1 outline-none transition-colors hover:bg-foreground/[0.015]",
                                tabindex: "0",
                                title: "Click to edit",
                                onclick: move |_| editing.set(true),
                                onkeydown: move |event| {
                                    let activate = match event.key() {
                                        Key::Enter => true,
                                        Key::Character(value) => value == " ",
                                        _ => false,
                                    };
                                    if activate {
                                        event.prevent_default();
                                        editing.set(true);
                                    }
                                },
                                dangerous_inner_html: note.html.clone(),
                            }
                        }
                        if !write_error().is_empty() {
                            div { class: "mt-3 rounded-lg border border-destructive/30 bg-destructive/[0.06] px-3 py-2 text-xs text-destructive", "{write_error}" }
                        }
                    }
                } else if preview_loading() {
                    div { class: "grid h-full place-items-center",
                        div { class: "h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-primary" }
                    }
                } else if !preview_error().is_empty() {
                    div { class: "grid h-full place-items-center px-8 text-center",
                        div { class: "max-w-sm",
                            div { class: "text-sm font-semibold", "Could not read note" }
                            div { class: "mt-2 text-xs leading-relaxed text-muted-foreground", "{preview_error}" }
                        }
                    }
                } else {
                    div { class: "grid h-full place-items-center px-8 text-center",
                        div { class: "max-w-sm",
                            div { class: "mx-auto mb-5 grid h-16 w-16 place-items-center rounded-[22px] bg-primary/[0.08] text-primary ring-1 ring-inset ring-primary/15",
                                Icon { class: "h-7 w-7", path { d: "M2 4a2 2 0 0 1 2-2h6a4 4 0 0 1 4 4v16a4 4 0 0 0-4-4H2Z" } path { d: "M22 4a2 2 0 0 0-2-2h-6a4 4 0 0 0-4 4v16a4 4 0 0 1 4-4h8Z" } }
                            }
                            h2 { class: "text-lg font-semibold tracking-tight", "Build your knowledge base" }
                            p { class: "mt-2 text-xs leading-relaxed text-muted-foreground", "Plain Markdown. Local files. Searchable from one quiet workspace." }
                            button {
                                class: "mt-5 rounded-lg bg-primary px-4 py-2 text-xs font-semibold text-primary-foreground transition-opacity hover:opacity-90",
                                onclick: move |_| {
                                    create_error.set(String::new());
                                    create_open.set(true);
                                },
                                "Create first note"
                            }
                        }
                    }
                }
            }
        }

        if create_open() {
            div {
                class: "fixed inset-0 z-50 grid place-items-center bg-scrim px-4",
                onclick: move |_| {
                    if !create_pending() {
                        create_open.set(false);
                    }
                },
                div {
                    class: "w-full max-w-sm rounded-2xl border border-border bg-card p-5 shadow-2xl",
                    onclick: move |event| event.stop_propagation(),
                    div { class: "text-sm font-semibold", "New note" }
                    div { class: "mt-1 text-[10px] text-muted-foreground", "A Markdown file will be created in your vault." }
                    input {
                        class: "mt-4 h-10 w-full rounded-lg border border-border bg-background px-3 text-sm outline-none focus:border-ring focus:ring-1 focus:ring-ring/30",
                        autofocus: true,
                        disabled: create_pending(),
                        placeholder: "Note title",
                        value: "{create_title}",
                        oninput: move |event| create_title.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Enter {
                                event.prevent_default();
                                submit_create(
                                    create_title(),
                                    create_pending,
                                    create_error,
                                    latest_create,
                                );
                            } else if event.key() == Key::Escape && !create_pending() {
                                create_open.set(false);
                            }
                        },
                    }
                    if !create_error().is_empty() {
                        div { class: "mt-2 text-[10px] text-destructive", "{create_error}" }
                    }
                    div { class: "mt-5 flex justify-end gap-2",
                        button {
                            class: "rounded-lg px-3 py-2 text-xs text-muted-foreground hover:bg-foreground/[0.05] disabled:opacity-40",
                            disabled: create_pending(),
                            onclick: move |_| create_open.set(false),
                            "Cancel"
                        }
                        button {
                            class: "rounded-lg bg-primary px-3 py-2 text-xs font-semibold text-primary-foreground hover:opacity-90 disabled:cursor-wait disabled:opacity-60",
                            disabled: create_pending(),
                            onclick: move |_| {
                                submit_create(
                                    create_title(),
                                    create_pending,
                                    create_error,
                                    latest_create,
                                );
                            },
                            if create_pending() { "Creating…" } else { "Create and edit" }
                        }
                    }
                }
            }
        }
        if !toast_error().is_empty() {
            div { class: "fixed bottom-4 right-4 z-50 flex max-w-sm items-start gap-3 rounded-xl border border-destructive/30 bg-card px-4 py-3 shadow-xl",
                div { class: "mt-0.5 text-destructive",
                    Icon { class: "h-4 w-4", circle { cx: "12", cy: "12", r: "10" } path { d: "M12 8v4" } path { d: "M12 16h.01" } }
                }
                div { class: "min-w-0 flex-1 text-xs leading-relaxed", "{toast_error}" }
                button { class: "text-muted-foreground hover:text-foreground", onclick: move |_| toast_error.set(String::new()), "×" }
            }
        }
    }
}

fn compact_path(path: &str) -> String {
    path.rfind("/.vmux/")
        .map(|index| format!("~{}", &path[index..]))
        .unwrap_or_else(|| path.to_string())
}

fn modified_label(modified_at: i64) -> String {
    let age = (js_sys::Date::now() as i64 - modified_at).max(0);
    let minute = 60_000;
    let hour = 60 * minute;
    let day = 24 * hour;
    match age {
        age if age < minute => "just now".to_string(),
        age if age < hour => format!("{}m", age / minute),
        age if age < day => format!("{}h", age / hour),
        age if age < 7 * day => format!("{}d", age / day),
        _ => js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(modified_at as f64))
            .to_locale_date_string("en-US", &wasm_bindgen::JsValue::UNDEFINED)
            .as_string()
            .unwrap_or_default(),
    }
}

const KNOWLEDGE_CSS: &str = r#"
.knowledge-md{font-size:15px;line-height:1.75;color:var(--foreground);word-break:break-word}
.knowledge-md>*:first-child{margin-top:0}
.knowledge-md>*:last-child{margin-bottom:0}
.knowledge-md h1,.knowledge-md h2,.knowledge-md h3,.knowledge-md h4,.knowledge-md h5,.knowledge-md h6{font-weight:650;line-height:1.25;letter-spacing:-.018em;margin:1.6em 0 .55em}
.knowledge-md h1{font-size:2em}.knowledge-md h2{font-size:1.5em;border-bottom:1px solid var(--border);padding-bottom:.3em}.knowledge-md h3{font-size:1.22em}.knowledge-md h4{font-size:1.05em}
.knowledge-md p{margin:.85em 0;color:color-mix(in oklab,var(--foreground) 88%,transparent)}
.knowledge-md ul,.knowledge-md ol{margin:.8em 0;padding-left:1.6em}.knowledge-md ul{list-style:disc}.knowledge-md ol{list-style:decimal}.knowledge-md li{margin:.3em 0}.knowledge-md li::marker{color:color-mix(in oklab,var(--foreground) 48%,transparent)}
.knowledge-md strong{font-weight:650}.knowledge-md em{font-style:italic}
.knowledge-md a{color:var(--primary);text-decoration:none;border-bottom:1px solid color-mix(in oklab,var(--primary) 35%,transparent)}.knowledge-md a:hover{border-bottom-color:var(--primary)}
.knowledge-md code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:.88em;background:color-mix(in oklab,var(--foreground) 8%,transparent);padding:.16em .38em;border-radius:.35em}
.knowledge-md pre{margin:1.1em 0;overflow-x:auto;border:1px solid var(--border);border-radius:.8rem;background:color-mix(in oklab,var(--foreground) 5%,transparent);padding:1em 1.1em}.knowledge-md pre code{background:none;padding:0;font-size:.86em}
.knowledge-md blockquote{margin:1em 0;border-left:3px solid color-mix(in oklab,var(--primary) 55%,transparent);border-radius:0 .6rem .6rem 0;background:color-mix(in oklab,var(--primary) 6%,transparent);padding:.3em 1em;color:var(--muted-foreground)}
.knowledge-md hr{border:0;border-top:1px solid var(--border);margin:2em 0}
.knowledge-md table{width:100%;border-collapse:separate;border-spacing:0;margin:1.2em 0;font-size:.92em;overflow:hidden;border:1px solid var(--border);border-radius:.7rem}.knowledge-md th,.knowledge-md td{padding:.55em .75em;text-align:left;border-bottom:1px solid var(--border)}.knowledge-md th{background:color-mix(in oklab,var(--foreground) 5%,transparent);font-weight:600}.knowledge-md tr:last-child td{border-bottom:0}
.knowledge-md img{max-width:100%;border-radius:.8rem;border:1px solid var(--border);margin:1.2em auto}
.knowledge-md input[type=checkbox]{margin-right:.45em;accent-color:var(--primary)}
"#;
