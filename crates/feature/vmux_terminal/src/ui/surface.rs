#![allow(non_snake_case)]

use super::state::{TerminalRowState, TerminalState};
use crate::event::*;
use crate::render_model::TermSpanPresentation;
use dioxus::html::Modifiers;
use dioxus::html::geometry::{ClientPoint, WheelDelta};
use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use std::collections::BTreeMap;
use std::rc::Rc;
use unicode_width::UnicodeWidthChar;
use vmux_api::terminal::CursorStyle;
use vmux_ecs::scroll::{BottomPadding, EDGE_TRIGGER_K, ScrollWindow};
use vmux_ui::agent_accent::AgentAccent;
use vmux_ui::favicon::{Favicon, FaviconSource};
use vmux_ui::hooks::Unclaimed;
use vmux_ui::hooks::{DeferredSend, send, use_key_claim, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::matrix_rain::MatrixRain;
use vmux_ui::prompt_ghost::PromptGhost;

const CONTAINER_ID: &str = "term-container";

const MEASURE_COLS: usize = 80;
const MEASURE_ROWS: usize = 8;

#[::vmux_page::page(
    component = Page,
    subtree,
    states = [vmux_ui::theme::ThemeUiState, vmux_ecs::event::TerminalUiState, vmux_api::input::KeyClaimsUiState],
    claims = crate::Terminal
)]
pub(crate) struct TerminalPage;

#[derive(Clone, Copy, Default, PartialEq)]
struct Viewport {
    cell: (f64, f64),
    client: (f64, f64),
    origin: (f64, f64),
}

impl Viewport {
    fn cell_at(&self, at: ClientPoint, padding: f64) -> Option<(u16, u16)> {
        let (cw, ch) = self.cell;
        if cw <= 0.0 || ch <= 0.0 {
            return None;
        }
        let col = ((at.x - self.origin.0 - padding) / cw).floor().max(0.0);
        let row = ((at.y - self.origin.1 - padding) / ch).floor().max(0.0);
        Some((col as u16, row as u16))
    }

    fn container_resized(&mut self, client: (f64, f64), padding: f64) -> bool {
        if !client.0.is_finite()
            || !client.1.is_finite()
            || client.0 <= 2.0 * padding + 1.0
            || client.1 <= 2.0 * padding + 1.0
        {
            return false;
        }
        self.client = client;
        true
    }

    fn cell_measured(&mut self, cell: (f64, f64)) {
        if cell.0.is_finite() && cell.1.is_finite() && cell.0 > 0.0 && cell.1 > 0.0 {
            self.cell = cell;
        }
    }

    fn cell_style(&self) -> String {
        let (cw, ch) = self.cell;
        if cw > 0.0 && ch > 0.0 {
            format!("--cw:{cw}px;--ch:{ch}px;")
        } else {
            String::new()
        }
    }

    fn key_context(alt: bool, copy_mode: bool) -> Vec<String> {
        let mut context = vec!["terminal".to_string()];
        if alt {
            context.push("terminal.alt".to_string());
        }
        if copy_mode {
            context.push("terminal.copy-mode".to_string());
        }
        context
    }

    fn locate_container(
        container: Signal<Option<Rc<MountedData>>>,
        theme: Memo<Option<TermThemeEvent>>,
        mut viewport: Signal<Viewport>,
    ) {
        spawn(async move {
            let Some(element) = container() else {
                return;
            };
            for _ in 0..16 {
                if let Ok(rect) = element.get_client_rect().await {
                    let padding = theme.peek().as_ref().map(|t| t.padding).unwrap_or(4.0) as f64;
                    let mut view = viewport.write();
                    if view.container_resized((rect.size.width, rect.size.height), padding) {
                        view.origin = (rect.origin.x, rect.origin.y);
                        return;
                    }
                }
                vmux_ui::platform::Platform::sleep(32).await;
            }
        });
    }

    fn handle_mouse_button(&self, e: &Event<MouseData>, padding: f64, pressed: bool) {
        if let Some((col, row)) = self.cell_at(e.client_coordinates(), padding) {
            emit_mouse(
                trigger_button_id(e),
                col,
                row,
                modifier_bits(e.modifiers()),
                pressed,
                false,
            );
        }
    }

    fn handle_mouse_move(
        &self,
        e: &Event<MouseData>,
        padding: f64,
        last_mouse_cell: &mut Signal<(i32, i32)>,
    ) {
        if let Some((col, row)) = self.cell_at(e.client_coordinates(), padding) {
            let last = (*last_mouse_cell)();
            if col as i32 == last.0 && row as i32 == last.1 {
                return;
            }
            last_mouse_cell.set((col as i32, row as i32));
            emit_mouse(
                held_button_id(e),
                col,
                row,
                modifier_bits(e.modifiers()),
                true,
                true,
            );
        }
    }

    fn theme_style(theme: Option<TermThemeEvent>) -> String {
        let Some(t) = theme else {
            return String::new();
        };
        let [fr, fg, fb] = t.foreground.components();
        let [br, bg, bb] = t.background.components();
        let [cr, cg, cb] = t.cursor.components();
        let mut s = format!(
            "--term-fg:rgb({fr},{fg},{fb});--term-bg:rgb({br},{bg},{bb});--term-cursor:rgb({cr},{cg},{cb});"
        );
        for (i, color) in t.ansi.iter().enumerate() {
            let [r, g, b] = color.components();
            s.push_str(&format!("--ansi-{i}:rgb({r},{g},{b});"));
        }
        if !t.font_family.is_empty() {
            s.push_str(&format!(
                "font-family:\"{}\",var(--font-mono);",
                t.font_family
            ));
        }
        if t.font_size > 0.0 {
            s.push_str(&format!("font-size:{}px;", t.font_size));
        }
        if t.line_height > 0.0 {
            s.push_str(&format!("line-height:{};", t.line_height));
        }
        s
    }

    fn handle_wheel(&self, e: &Event<WheelData>, padding: f64, wheel_accum: &mut Signal<f64>) {
        let (_, ch) = self.cell;
        let line_px = if ch > 0.0 { ch } else { 16.0 };
        let px = match e.data().delta() {
            WheelDelta::Pixels(delta) => delta.y,
            WheelDelta::Lines(delta) => delta.y * line_px,
            WheelDelta::Pages(delta) => delta.y * line_px * 3.0,
        };
        let total = (*wheel_accum)() + px;
        let notches = (total / line_px).trunc();
        wheel_accum.set(total - notches * line_px);
        let count = (notches as i32).clamp(-10, 10);
        if count == 0 {
            return;
        }
        if let Some((col, row)) = self.cell_at(e.client_coordinates(), padding) {
            let button = if count < 0 { 64 } else { 65 };
            let modifiers = modifier_bits(e.modifiers());
            for _ in 0..count.unsigned_abs() {
                emit_mouse(button, col, row, modifiers, true, false);
            }
        }
    }

    fn handle_scroll(
        &self,
        e: &Event<ScrollData>,
        padding: f64,
        total_rows: u32,
        loaded_first: u32,
        loaded_len: u32,
        following: &mut Signal<bool>,
        last_scroll_req: &mut Signal<u32>,
    ) {
        let (_, ch) = self.cell;
        if ch <= 0.0 {
            return;
        }
        let scrolled = e.scroll_top();
        let visible = e.client_height() as f64;
        let vis_first = (((scrolled - padding) / ch).floor()).max(0.0) as u32;
        let vis_rows = (visible / ch).ceil() as u32 + 1;
        let follow = e.scroll_height() as f64 - scrolled - visible <= ch.max(2.0) + 1.0;
        if follow != *following.peek() {
            following.set(follow);
            last_scroll_req.set(if follow { u32::MAX } else { vis_first });
            let _ = send(&TermScrollEvent {
                top_row: vis_first,
                follow,
            });
            if follow {
                return;
            }
        }
        if follow {
            return;
        }
        let trigger = (vis_rows as f32 * EDGE_TRIGGER_K).ceil() as u32;
        if ScrollWindow::new(total_rows, vis_first, vis_rows).needs_refetch(
            loaded_first,
            loaded_len,
            trigger,
        ) && (*last_scroll_req)() != vis_first
        {
            last_scroll_req.set(vis_first);
            let _ = send(&TermScrollEvent {
                top_row: vis_first,
                follow: false,
            });
        }
    }

    fn announce(&self, padding: f64) -> bool {
        let (cw, ch) = self.cell;
        if cw <= 0.0
            || ch <= 0.0
            || self.client.0 <= 2.0 * padding + cw
            || self.client.1 <= 2.0 * padding + ch
        {
            return false;
        }
        let result = DeferredSend::new(&TermResizeEvent {
            char_width: cw as f32,
            char_height: ch as f32,
            viewport_width: (self.client.0 - 2.0 * padding) as f32,
            viewport_height: (self.client.1 - 2.0 * padding) as f32,
        })
        .send();
        #[cfg(host)]
        if let Err(error) = &result {
            bevy::log::warn!(%error, "terminal viewport report failed");
        }
        result.is_ok()
    }
}

#[component]
fn ViewportSizing(viewport: Signal<Viewport>, padding: f64) -> Element {
    let revision = vmux_ui::transport::HostBinding::current()
        .map_or(0, |binding| binding.navigation_revision());
    let reported = use_hook(|| Rc::new(std::cell::Cell::new(None)));
    use_effect(use_reactive!(|revision, padding| {
        let view = viewport();
        let current = (revision, view.cell, view.client, padding);
        if reported.get() != Some(current) && view.announce(padding) {
            reported.set(Some(current));
        }
    }));
    rsx! {}
}

fn localized_terminal_title(title: &str) -> String {
    if title == "Terminal" {
        translate("command-terminal")
    } else if let Some(detail) = title
        .strip_prefix("Terminal (")
        .and_then(|value| value.strip_suffix(')'))
    {
        translate_with(
            "command-terminal-path",
            &[("path", TranslationValue::String(detail))],
        )
    } else {
        title.to_string()
    }
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let terminal = TerminalState::use_state();
    let mut following = use_signal(|| true);
    let mut last_scroll_req = use_signal(|| u32::MAX);
    let mut viewport = use_signal(Viewport::default);
    let mut container = use_signal(|| None::<Rc<MountedData>>);
    let mut last_mouse_cell = use_signal(|| (-1i32, -1i32));
    let mut wheel_accum = use_signal(|| 0.0f64);
    let keys = use_key_claim(Unclaimed::Forwards, move || {
        Viewport::key_context((terminal.alt)(), (terminal.copy_mode)())
    });

    let theme: Option<TermThemeEvent> = (terminal.theme)();
    let theme_style = Viewport::theme_style(theme.clone());
    let padding = theme.map(|t| t.padding).unwrap_or(4.0) as f64;
    let cell_style = viewport().cell_style();
    let passthrough = (terminal.alt)() || (terminal.copy_mode)() || (terminal.mouse)();
    let overflow_class = if passthrough {
        "overflow-hidden"
    } else {
        "overflow-auto"
    };
    let _ = localized_terminal_title(&(terminal.raw_title)());

    rsx! {
        ViewportSizing { viewport, padding }
        div {
            id: CONTAINER_ID,
            "data-vmux-initial-scroll": if passthrough { "0" } else { "bottom" },
            "data-vmux-follow-scroll": (following() && !passthrough).to_string(),
            "data-vmux-preserve-scroll": "auto",
            tabindex: "0",
            class: "relative h-full w-full {overflow_class} bg-term-bg text-term-fg font-mono text-sm leading-tight select-none outline-none",
            style: "{theme_style}{cell_style}",
            onmounted: move |e: Event<MountedData>| async move {
                container.set(Some(e.data()));
                Viewport::locate_container(container, terminal.theme, viewport);
                if let Err(error) = e.data().set_focus(true).await {
                    dioxus::logger::tracing::warn!("focusing the terminal failed: {error:?}");
                }
            },
            onresize: move |event: Event<ResizeData>| {
                if let Ok(size) = event.get_border_box_size() {
                    viewport.write().container_resized((size.width, size.height), padding);
                }
                Viewport::locate_container(container, terminal.theme, viewport);
            },
            onmousedown: move |e: Event<MouseData>| viewport().handle_mouse_button(&e, padding, true),
            onkeydown: move |e: Event<KeyboardData>| keys.on_keydown(&e, |_| false),
            onmouseup: move |e: Event<MouseData>| viewport().handle_mouse_button(&e, padding, false),
            onmousemove: move |e: Event<MouseData>| {
                viewport().handle_mouse_move(&e, padding, &mut last_mouse_cell);
            },
            oncontextmenu: move |e: Event<MouseData>| e.prevent_default(),
            onwheel: move |e: Event<WheelData>| {
                if (terminal.alt)() || (terminal.copy_mode)() || (terminal.mouse)() {
                    e.prevent_default();
                    viewport().handle_wheel(&e, padding, &mut wheel_accum);
                }
            },
            onscroll: move |e: Event<ScrollData>| {
                if !((terminal.alt)() || (terminal.copy_mode)() || (terminal.mouse)()) {
                    viewport().handle_scroll(
                        &e,
                        padding,
                        (terminal.total_rows)(),
                        (terminal.first_row)(),
                        terminal.rows.read().len() as u32,
                        &mut following,
                        &mut last_scroll_req,
                    );
                }
            },
            TerminalSurfaceBody {
                rows: terminal.rows,
                total_rows: terminal.total_rows,
                cols: terminal.cols,
                cursor: terminal.cursor,
                selection: terminal.selection,
                copy_mode: terminal.copy_mode,
                service_error: terminal.service_error,
                loading: terminal.loading,
                prompt_draft: terminal.prompt_draft,
                viewport,
                padding,
            }
        }
    }
}

#[component]
fn TerminalSurfaceBody(
    rows: Memo<BTreeMap<u32, TerminalRowState>>,
    total_rows: Memo<u32>,
    cols: Memo<u16>,
    cursor: Memo<Option<TermCursor>>,
    selection: Memo<Option<TermSelectionRange>>,
    copy_mode: Memo<bool>,
    service_error: Memo<String>,
    loading: Memo<Option<(String, String)>>,
    prompt_draft: Memo<(String, bool)>,
    mut viewport: Signal<Viewport>,
    padding: f64,
) -> Element {
    let (_, ch) = viewport().cell;
    let client_h = viewport().client.1;
    let content_h = total_rows() as f64 * ch;
    let bottom_pad = if ch > 0.0 && content_h + 2.0 * padding > client_h {
        BottomPadding::aligned(client_h as f32, padding as f32, ch as f32).pixels() as f64
    } else {
        0.0
    };
    let spacer_h = content_h + bottom_pad;
    let measure_text = vec!["X".repeat(MEASURE_COLS); MEASURE_ROWS].join("\n");
    let base_rows = rows();
    let service_error = service_error.read().clone();
    let loading = loading.read().clone();

    rsx! {
        if copy_mode() {
            if let Some(cursor) = cursor() {
                div {
                    class: "absolute right-2 top-1 z-10 rounded bg-term-fg px-1 text-xs text-term-bg",
                    "[{cursor.row.saturating_add(1)}/{base_rows.len().max(1)}]"
                }
            }
        }
        if !service_error.is_empty() {
            div {
                class: "absolute inset-0 z-50 flex items-center justify-center bg-black/60",
                div {
                    class: "rounded-md border border-ansi-1 bg-term-bg px-4 py-2 text-sm text-ansi-1",
                    "{service_error}"
                }
            }
        }
        if let Some((label, segment)) = loading {
            {
                let (draft_text, draft_skipped) = prompt_draft.read().clone();
                rsx! {
                    TerminalLoadingOverlay {
                        label,
                        segment,
                        draft_text,
                        draft_skipped,
                    }
                }
            }
        }
        span {
            class: "invisible absolute left-0 top-0 whitespace-pre [font:inherit]",
            onresize: move |e: Event<ResizeData>| {
                if let Ok(size) = e.get_border_box_size() {
                    viewport.write().cell_measured((
                        size.width / MEASURE_COLS as f64,
                        size.height / MEASURE_ROWS as f64,
                    ));
                }
            },
            {measure_text}
        }
        div {
            style: "padding:{padding}px;",
            div {
                class: "relative",
                style: "height:{spacer_h}px;",
                for (doc_row, row) in base_rows.iter() {
                    div {
                        key: "{doc_row}",
                        class: "absolute inset-x-0 top-[var(--terminal-row-top)]",
                        style: "--terminal-row-top:{*doc_row as f64 * ch}px;",
                        TerminalRow {
                            row_idx: *doc_row as usize,
                            row: row.clone(),
                            selection,
                            cols,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TerminalLoadingOverlay(
    label: String,
    segment: String,
    draft_text: String,
    draft_skipped: bool,
) -> Element {
    let accent = AgentAccent::for_agent(&segment);
    let display_label = if segment == "terminal" {
        translate("command-terminal")
    } else {
        label
    };
    let favicon_url = FaviconSource::agent(&segment).unwrap_or_default();
    let words = vec![display_label.to_uppercase()];
    let composing = !draft_skipped && !draft_text.is_empty();
    rsx! {
        div {
            class: "pointer-events-none absolute inset-0 z-40 overflow-hidden bg-term-bg",
            MatrixRain { accent_rgb: accent.rain_rgb.to_string(), words }
            div {
                class: "relative z-10 flex h-full w-full items-center justify-center",
                div {
                    class: "flex items-center gap-3 rounded-lg bg-background/85 px-5 py-4 backdrop-blur-md",
                    div {
                        class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-foreground/[0.06] ring-1 ring-inset ring-foreground/10",
                        Favicon {
                            favicon_url: "".to_string(),
                            url: favicon_url,
                            class: "h-5 w-5 shrink-0 rounded object-contain".to_string(),
                            globe_class: "h-5 w-5 text-muted-foreground".to_string(),
                        }
                    }
                    div {
                        div { class: "text-sm font-semibold {accent.accent_text}", "{display_label}" }
                        if composing {
                            div {
                                class: "mt-0.5 w-80 whitespace-pre-wrap break-words font-mono text-sm text-foreground",
                                "{draft_text}"
                                span { class: "ml-px inline-block h-3.5 w-1.5 align-middle animate-pulse {accent.accent_bg}" }
                            }
                            div {
                                class: "mt-1 text-[10px] text-muted-foreground/70",
                                {translate("terminal-runs-when-ready")}
                            }
                        } else if draft_skipped {
                            div {
                                class: "flex items-center gap-1.5 text-xs text-muted-foreground",
                                span { class: "font-mono", {format!("> {}", translate("terminal-booting"))} }
                                span { class: "inline-block h-3.5 w-2 animate-pulse {accent.accent_bg}" }
                            }
                        } else {
                            div {
                                class: "mt-0.5",
                                PromptGhost {
                                    accent_bg: accent.accent_bg.to_string(),
                                    terminal: true,
                                }
                            }
                            div {
                                class: "mt-1 text-[10px] text-muted-foreground/70",
                                {translate("terminal-type-command")}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
const _TW_SAFELIST: &[&str] = &[
    "text-ansi-0",  "text-ansi-1",  "text-ansi-2",  "text-ansi-3",
    "text-ansi-4",  "text-ansi-5",  "text-ansi-6",  "text-ansi-7",
    "text-ansi-8",  "text-ansi-9",  "text-ansi-10", "text-ansi-11",
    "text-ansi-12", "text-ansi-13", "text-ansi-14", "text-ansi-15",
    "bg-ansi-0",  "bg-ansi-1",  "bg-ansi-2",  "bg-ansi-3",
    "bg-ansi-4",  "bg-ansi-5",  "bg-ansi-6",  "bg-ansi-7",
    "bg-ansi-8",  "bg-ansi-9",  "bg-ansi-10", "bg-ansi-11",
    "bg-ansi-12", "bg-ansi-13", "bg-ansi-14", "bg-ansi-15",
    "text-term-bg", "bg-term-fg",
    "border-ansi-1",
];

#[component]
fn TerminalRow(
    row_idx: usize,
    row: TerminalRowState,
    selection: Memo<Option<TermSelectionRange>>,
    cols: Memo<u16>,
) -> Element {
    let line = &row.line;
    let selected_cols = row_selection_cols(&selection(), row_idx, cols());

    rsx! {
        div {
            class: "relative isolate h-[var(--ch,1.2em)] whitespace-pre",
            for (span_idx, span) in line.spans.iter().enumerate() {
                if let Some(background) = TermSpanPresentation::new(span).background {
                    div {
                        key: "bg-{span_idx}",
                        class: "{background.class}",
                        style: "{background.style}",
                    }
                }
            }
            for (span_idx, span) in line.spans.iter().enumerate() {
                TermSpanView {
                    span: span.clone(),
                    span_idx,
                    cursor: row.cursor.clone(),
                    cursor_style: CursorStyle::Block,
                }
            }
            if let Some((sel_start, sel_end)) = selected_cols {
                div {
                    class: "pointer-events-none absolute inset-y-0 bg-white/25",
                    style: "left:calc(var(--cw, 1ch) * {sel_start});width:calc(var(--cw, 1ch) * {sel_end - sel_start});",
                }
            }
            for link in line.links.iter() {
                {
                    let url = link.url.clone();
                    let start = link.start_col;
                    let width = link.end_col - link.start_col + 1;
                    rsx! {
                        div {
                            key: "lnk-{start}",
                            class: "absolute inset-y-0 z-[2] cursor-pointer hover:border-b-2 hover:border-primary",
                            style: "left:calc(var(--cw, 1ch) * {start});width:calc(var(--cw, 1ch) * {width});",
                            onmousedown: move |e: Event<MouseData>| {
                                e.stop_propagation();
                                e.prevent_default();
                            },
                            onclick: move |e: Event<MouseData>| {
                                e.stop_propagation();
                                e.prevent_default();
                                let _ = send(&TermLinkOpenRequest { url: url.clone() });
                            },
                        }
                    }
                }
            }
        }
    }
}

fn trigger_button_id(e: &Event<MouseData>) -> u8 {
    match e.trigger_button() {
        Some(MouseButton::Primary) => 0,
        Some(MouseButton::Auxiliary) => 1,
        Some(MouseButton::Secondary) => 2,
        _ => 0,
    }
}

fn held_button_id(e: &Event<MouseData>) -> u8 {
    let held = e.held_buttons();
    if held.contains(MouseButton::Primary) {
        0
    } else if held.contains(MouseButton::Auxiliary) {
        1
    } else if held.contains(MouseButton::Secondary) {
        2
    } else {
        3
    }
}

fn modifier_bits(mods: Modifiers) -> u8 {
    let mut m = 0u8;
    if mods.contains(Modifiers::CONTROL) {
        m |= MOD_CTRL;
    }
    if mods.contains(Modifiers::ALT) {
        m |= MOD_ALT;
    }
    if mods.contains(Modifiers::SHIFT) {
        m |= MOD_SHIFT;
    }
    if mods.contains(Modifiers::META) {
        m |= MOD_SUPER;
    }
    m
}

fn emit_mouse(button: u8, col: u16, row: u16, modifiers: u8, pressed: bool, moving: bool) {
    let _ = send(&TermMouseEvent {
        button,
        col,
        row,
        modifiers,
        pressed,
        moving,
    });
}

#[component]
fn TermSpanView(
    span: TermSpan,
    span_idx: usize,
    cursor: Option<TermCursor>,
    cursor_style: CursorStyle,
) -> Element {
    let span = &span;
    let cursor = cursor.as_ref();
    let presentation = TermSpanPresentation::new(span);
    let classes = &presentation.classes;
    let style = &presentation.style;

    if let Some(cursor) = cursor
        && cursor.visible
        && span_contains_col(span, cursor.col)
    {
        let offset = span_char_offset_for_col(span, cursor.col);
        let chars = span.text.chars().collect::<Vec<_>>();
        let before = chars[..offset.min(chars.len())].iter().collect::<String>();
        let after = chars
            .get(offset.saturating_add(1)..)
            .unwrap_or(&[])
            .iter()
            .collect::<String>();
        let cursor_ch = if cursor.ch.is_empty() {
            " ".to_string()
        } else {
            cursor.ch.clone()
        };
        let (cursor_classes, cursor_style_attr) = presentation.cursor(cursor_style);

        return rsx! {
            if !before.is_empty() {
                span {
                    class: "relative z-[1] {classes}",
                    style: "{style}",
                    "{before}"
                }
            }
            span {
                class: "relative z-[1] {cursor_classes}",
                style: "{cursor_style_attr}",
                "{cursor_ch}"
            }
            if !after.is_empty() {
                span {
                    class: "relative z-[1] {classes}",
                    style: "{style}",
                    "{after}"
                }
            }
        };
    }

    rsx! {
        span {
            key: "{span_idx}",
            class: "relative z-[1] {classes}",
            style: "{style}",
            "{span.text}"
        }
    }
}

fn span_contains_col(span: &TermSpan, col: u16) -> bool {
    let end_col = if span.grid_cols > 0 {
        span.col + span.grid_cols
    } else {
        span.col + span.text.chars().count() as u16
    };
    col >= span.col && col < end_col
}

fn span_char_offset_for_col(span: &TermSpan, col: u16) -> usize {
    let target_grid_col = col.saturating_sub(span.col);
    let mut offset = 0usize;
    let mut grid_col_acc = 0u16;
    for (i, ch) in span.text.chars().enumerate() {
        if grid_col_acc >= target_grid_col {
            return i;
        }
        grid_col_acc += ch.width().unwrap_or(1) as u16;
        offset = i + 1;
    }
    offset
}

fn row_selection_cols(
    selection: &Option<TermSelectionRange>,
    row_idx: usize,
    total_cols: u16,
) -> Option<(usize, usize)> {
    let sel = selection.as_ref()?;
    let row = row_idx as u16;
    let lo_row = sel.start_row.min(sel.end_row);
    let hi_row = sel.start_row.max(sel.end_row);
    if row < lo_row || row > hi_row {
        return None;
    }
    let (sr, sc, er, ec) = if sel.is_block {
        (
            lo_row,
            sel.start_col.min(sel.end_col),
            hi_row,
            sel.start_col.max(sel.end_col),
        )
    } else if (sel.start_row, sel.start_col) <= (sel.end_row, sel.end_col) {
        (sel.start_row, sel.start_col, sel.end_row, sel.end_col)
    } else {
        (sel.end_row, sel.end_col, sel.start_row, sel.start_col)
    };

    let (start, end_exclusive) = if sel.is_block || sr == er {
        (sc as usize, ec as usize + 1)
    } else if row == sr {
        (sc as usize, total_cols as usize)
    } else if row == er {
        (0, ec as usize + 1)
    } else {
        (0, total_cols as usize)
    };

    if end_exclusive <= start {
        None
    } else {
        Some((start, end_exclusive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::{Cell, RefCell};
    use vmux_api::BinEvent;
    use vmux_ui::transport::event_listener::EventListenerError;
    use vmux_ui::transport::{HostBinding, PageHost};

    struct SizingHost {
        revision: Cell<u64>,
        reports: RefCell<Vec<(u64, TermResizeEvent)>>,
    }

    impl SizingHost {
        fn new() -> Rc<Self> {
            Rc::new(Self {
                revision: Cell::new(1),
                reports: RefCell::new(Vec::new()),
            })
        }

        fn rebind(&self) {
            self.reports.borrow_mut().clear();
            self.revision.set(self.revision.get() + 1);
        }
    }

    impl PageHost for SizingHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == TermResizeEvent::id() {
                let event =
                    rkyv::from_bytes::<TermResizeEvent, rkyv::rancor::Error>(bytes).unwrap();
                self.reports.borrow_mut().push((self.revision.get(), event));
            }
            Ok(())
        }

        fn listen(
            &self,
            _: &str,
            _: vmux_ui::transport::BytesListener,
        ) -> Result<(), EventListenerError> {
            Ok(())
        }

        fn navigation_revision(&self) -> u64 {
            self.revision.get()
        }
    }

    #[component]
    fn MeasuredViewport() -> Element {
        let viewport = use_signal(|| Viewport {
            cell: (8.0, 16.0),
            client: (1288.0, 1008.0),
            ..Default::default()
        });
        rsx! { ViewportSizing { viewport, padding: 4.0 } }
    }

    #[test]
    fn reused_page_reannounces_unchanged_geometry_to_its_new_binding() {
        let host = SizingHost::new();
        let mut dom = VirtualDom::new(MeasuredViewport);
        dom.provide_root_context(HostBinding(host.clone()));
        dom.rebuild_to_vec();
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        assert_eq!(host.reports.borrow().len(), 1);
        host.rebind();
        dom.mark_all_dirty();
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        let reports = host.reports.borrow();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0, 2);
        assert_eq!(
            (reports[0].1.viewport_width, reports[0].1.viewport_height),
            (1280.0, 1000.0)
        );
        drop(reports);
        dom.mark_all_dirty();
        dom.render_immediate_to_vec();
        dom.render_immediate_to_vec();
        assert_eq!(host.reports.borrow().len(), 1);
    }

    #[test]
    fn process_specific_terminal_pages_can_exchange_their_state_and_input() {
        let permissions = TerminalPage::page().page_permissions();
        for permission in ["TerminalUiState", "TermResizeEvent", "KeyClaimsUiState"] {
            assert!(permissions.allows("vmux://terminal/29876", permission));
        }
    }
}
