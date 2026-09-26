use std::collections::HashMap;

use dioxus::prelude::*;
use vmux_core::event::{
    DiagSeverity, FileDefinitionRequest, FileDiagnostic, FileFoldToggle, FileHover,
    FileHoverRequest, FileLine, FileLineLayout, FilePointerEvent, FoldGutter,
};
use vmux_git::event::GitLineStatus;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::platform::sleep_ms;

use super::text_geometry::{column_in_line, gutter_px};
use super::{
    HOVER_DELAY_MS, diff_marker_row_class, diff_marker_sign, diff_marker_text_class,
    focus_file_input,
};
use crate::page_model::{
    CellMetrics, ColumnRuler, line_severity, severity_color_class, span_style, squiggle_style,
};

#[component]
pub(super) fn EditorLines(
    lines: Signal<Vec<FileLine>>,
    line_layouts: Signal<Vec<FileLineLayout>>,
    first_row: Signal<u32>,
    diagnostics: Signal<Vec<FileDiagnostic>>,
    git_line_markers: ReadSignal<HashMap<u32, GitLineStatus>>,
    wrap_columns: Signal<u16>,
    cell_height: f64,
    gutter_chars: usize,
    total_lines: Signal<u32>,
    cell_dims: Signal<CellMetrics>,
    ctx_menu: Signal<Option<(f64, f64, u32, u32)>>,
    editor_dragging: Signal<bool>,
    editor_drag_origin: Signal<Option<(i32, i32)>>,
    gutter_hover: Signal<bool>,
    hover_pos: Signal<Option<(u32, u32)>>,
    lsp_hover: Signal<Option<FileHover>>,
    hover_diag: Signal<Option<FileDiagnostic>>,
) -> Element {
    let chunks = LineChunk::split(&lines(), &line_layouts(), first_row());
    let diags = diagnostics();
    let markers = git_line_markers();
    let wrap_cols = wrap_columns();
    rsx! {
        for chunk in chunks {
            EditorLineChunk {
                key: "{chunk.start}",
                rows: chunk.rows,
                diagnostics: diags.clone(),
                markers: markers.clone(),
                wrap_cols,
                cell_height,
                gutter_chars,
                total_lines,
                cell_dims,
                ctx_menu,
                editor_dragging,
                editor_drag_origin,
                gutter_hover,
                hover_pos,
                lsp_hover,
                hover_diag,
            }
        }
    }
}

struct LineChunk {
    start: u32,
    rows: Vec<(FileLine, FileLineLayout)>,
}

impl LineChunk {
    const LINES: u32 = 24;

    fn split(lines: &[FileLine], layouts: &[FileLineLayout], first_row: u32) -> Vec<Self> {
        let mut chunks: Vec<Self> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let found = layouts.binary_search_by_key(&line.line_no, |layout| layout.line_no);
            let layout = match found {
                Ok(at) => layouts[at],
                Err(_) => FileLineLayout {
                    line_no: line.line_no,
                    row: first_row + i as u32,
                    rows: 1,
                },
            };
            let start = line.line_no - (line.line_no % Self::LINES);
            if chunks.last().is_none_or(|chunk| chunk.start != start) {
                chunks.push(Self {
                    start,
                    rows: Vec::new(),
                });
            }
            if let Some(chunk) = chunks.last_mut() {
                chunk.rows.push((line.clone(), layout));
            }
        }
        chunks
    }
}

#[component]
fn EditorLineChunk(
    rows: Vec<(FileLine, FileLineLayout)>,
    diagnostics: Vec<FileDiagnostic>,
    markers: HashMap<u32, GitLineStatus>,
    wrap_cols: u16,
    cell_height: f64,
    gutter_chars: usize,
    total_lines: Signal<u32>,
    cell_dims: Signal<CellMetrics>,
    ctx_menu: Signal<Option<(f64, f64, u32, u32)>>,
    editor_dragging: Signal<bool>,
    editor_drag_origin: Signal<Option<(i32, i32)>>,
    gutter_hover: Signal<bool>,
    hover_pos: Signal<Option<(u32, u32)>>,
    lsp_hover: Signal<Option<FileHover>>,
    hover_diag: Signal<Option<FileDiagnostic>>,
) -> Element {
    rsx! {
        for (line, layout) in rows.iter() {
            {
                let ln = line.line_no;
                let mut line_diags: Vec<FileDiagnostic> = Vec::new();
                for d in diagnostics.iter() {
                    if d.line == ln {
                        line_diags.push(d.clone());
                    }
                }
                rsx! {
                    EditorLineRow {
                        key: "{ln}",
                        line: line.clone(),
                        layout: *layout,
                        severity: line_severity(&diagnostics, ln),
                        diff_marker: markers.get(&(ln + 1)).copied(),
                        diagnostics: line_diags,
                        cell_height,
                        gutter_chars,
                        wrap_cols,
                        total_lines,
                        cell_dims,
                        ctx_menu,
                        editor_dragging,
                        editor_drag_origin,
                        gutter_hover,
                        hover_pos,
                        lsp_hover,
                        hover_diag,
                    }
                }
            }
        }
    }
}

#[component]
fn EditorLineRow(
    line: FileLine,
    layout: FileLineLayout,
    severity: Option<DiagSeverity>,
    diff_marker: Option<GitLineStatus>,
    diagnostics: Vec<FileDiagnostic>,
    cell_height: f64,
    gutter_chars: usize,
    wrap_cols: u16,
    total_lines: Signal<u32>,
    cell_dims: Signal<CellMetrics>,
    ctx_menu: Signal<Option<(f64, f64, u32, u32)>>,
    editor_dragging: Signal<bool>,
    editor_drag_origin: Signal<Option<(i32, i32)>>,
    gutter_hover: Signal<bool>,
    hover_pos: Signal<Option<(u32, u32)>>,
    lsp_hover: Signal<Option<FileHover>>,
    hover_diag: Signal<Option<FileDiagnostic>>,
) -> Element {
    let mut ctx_menu = ctx_menu;
    let mut editor_dragging = editor_dragging;
    let mut editor_drag_origin = editor_drag_origin;
    let mut gutter_hover = gutter_hover;
    let mut hover_pos = hover_pos;
    let mut lsp_hover = lsp_hover;
    let mut hover_diag = hover_diag;
    let ln = line.line_no;
    let fold = line.fold;
    let ch = cell_height;
    let gw = gutter_chars;
    let mut line_text = String::new();
    for span in &line.spans {
        line_text.push_str(&span.text);
    }
    let pointer_text = line_text.clone();
    let menu_text = line_text.clone();
    let hover_text = line_text.clone();
    let lt = layout.row as f64 * ch;
    let line_height = layout.rows as f64 * ch;
    let text_class = if wrap_cols > 0 {
        "pointer-events-none relative box-border whitespace-pre-wrap break-all pr-8"
    } else {
        "pointer-events-none relative whitespace-pre pr-8"
    };
    let text_style = if wrap_cols > 0 {
        format!("width:calc(var(--cw) * {wrap_cols} + 2rem);")
    } else {
        String::new()
    };
    rsx! {
        div {
            class: if let Some(marker) = diff_marker { "group absolute inset-x-0 flex items-start {diff_marker_row_class(marker)}" } else { "group absolute inset-x-0 flex items-start" },
            style: "top:{lt}px;height:{line_height}px;",
            onpointerdown: move |e: Event<PointerData>| {
                e.prevent_default();
                ctx_menu.set(None);
                let cell = cell_dims();
                let (_, col) = column_in_line(
                    e.element_coordinates(),
                    gutter_px(total_lines(), cell.narrow),
                    cell,
                    &pointer_text,
                    wrap_cols,
                    true,
                );
                let at = e.client_coordinates();
                editor_dragging.set(false);
                if e.modifiers().meta() {
                    editor_drag_origin.set(None);
                    let _ = send(&FileDefinitionRequest { line: ln, col });
                } else {
                    editor_drag_origin.set(Some((at.x as i32, at.y as i32)));
                    let _ = send(&FilePointerEvent {
                        line: ln,
                        col,
                        extend: e.modifiers().shift(),
                        add: e.modifiers().alt(),
                    });
                }
                focus_file_input();
            },
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                let cell = cell_dims();
                let (_, col) = column_in_line(
                    e.element_coordinates(),
                    gutter_px(total_lines(), cell.narrow),
                    cell,
                    &menu_text,
                    wrap_cols,
                    true,
                );
                let at = e.client_coordinates();
                ctx_menu.set(Some((at.x, at.y, ln, col)));
            },
            onmousemove: move |e: Event<MouseData>| {
                let cell = cell_dims();
                let (x, col) = column_in_line(
                    e.element_coordinates(),
                    gutter_px(total_lines(), cell.narrow),
                    cell,
                    &hover_text,
                    wrap_cols,
                    editor_dragging(),
                );
                if editor_dragging() {
                    e.prevent_default();
                    let _ = send(&FilePointerEvent {
                        line: ln,
                        col,
                        extend: true,
                        add: false,
                    });
                    return;
                }
                let in_gutter = x < 0.0;
                if gutter_hover() != in_gutter {
                    gutter_hover.set(in_gutter);
                }
                if in_gutter {
                    return;
                }
                if hover_pos() != Some((ln, col)) {
                    hover_pos.set(Some((ln, col)));
                    lsp_hover.set(None);
                    spawn(async move {
                        sleep_ms(HOVER_DELAY_MS).await;
                        if hover_pos() != Some((ln, col)) {
                            return;
                        }
                        let _ = send(&FileHoverRequest { line: ln, col });
                    });
                }
            },
            span {
                class: "sticky left-0 z-[1] relative flex shrink-0 select-none items-center justify-end bg-background pl-4 pr-5 tabular-nums",
                style: "min-width:calc(var(--cw, 1ch) * {gw} + 3rem);height:{ch}px;",
                if let Some(s) = severity {
                    span { class: "pointer-events-none absolute left-1 {severity_color_class(s)}", "●" }
                }
                span {
                    class: if let Some(marker) = diff_marker { "shrink-0 text-right opacity-90 {diff_marker_text_class(marker)}" } else { "shrink-0 text-right opacity-40 group-hover:opacity-90" },
                    style: "width:calc(var(--cw, 1ch) * {gw});",
                    "{ln + 1}"
                }
                span {
                    class: if let Some(marker) = diff_marker { "ml-1 w-[1ch] shrink-0 text-center font-semibold {diff_marker_text_class(marker)}" } else { "ml-1 w-[1ch] shrink-0" },
                    if let Some(marker) = diff_marker {
                        span { title: translate("editor-changed-line"), "{diff_marker_sign(marker)}" }
                    }
                }
                match fold {
                    FoldGutter::None => rsx! {},
                    _ => rsx! {
                        FoldMarker {
                            line: ln,
                            collapsed: fold == FoldGutter::Collapsed,
                            revealed: gutter_hover(),
                        }
                    },
                }
            }
            span { class: "{text_class}", style: "{text_style}",
                IndentGuides { levels: line.indent_levels }
                for (i, s) in line.spans.iter().enumerate() {
                    span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                }
                for (di, d) in diagnostics.iter().enumerate() {
                    {
                        let color = match d.severity {
                            DiagSeverity::Error => "rgb(239,68,68)",
                            DiagSeverity::Warning => "rgb(245,158,11)",
                            DiagSeverity::Info => "rgb(56,189,248)",
                            DiagSeverity::Hint => "rgb(34,211,238)",
                        };
                        let dc = d.clone();
                        let marks = ColumnRuler::new(&line_text, cell_dims());
                        rsx! {
                            span {
                                key: "d{di}",
                                style: squiggle_style(
                                    marks.x_of(d.start_col),
                                    marks.width_between(d.start_col, d.end_col),
                                    color,
                                ),
                                onmouseenter: move |_| hover_diag.set(Some(dc.clone())),
                                onmouseleave: move |_| hover_diag.set(None),
                            }
                        }
                    }
                }
                if fold == FoldGutter::Collapsed {
                    span { class: "ml-1 rounded bg-white/10 px-1 text-foreground/40", "⋯" }
                }
            }
        }
    }
}

#[component]
pub(super) fn StickyScope(
    lines: Vec<FileLine>,
    cell_height: f64,
    gutter_chars: usize,
    on_pick: EventHandler<u32>,
) -> Element {
    if lines.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "sticky top-0 z-[12] h-0",
            div { class: "absolute inset-x-0 top-0 border-b border-foreground/10 bg-background/95 backdrop-blur",
                for line in lines {
                    StickyScopeRow {
                        key: "{line.line_no}",
                        line,
                        cell_height,
                        gutter_chars,
                        on_pick,
                    }
                }
            }
        }
    }
}

#[component]
fn StickyScopeRow(
    line: FileLine,
    cell_height: f64,
    gutter_chars: usize,
    on_pick: EventHandler<u32>,
) -> Element {
    let row = line.line_no;
    rsx! {
        div {
            class: "flex cursor-default whitespace-pre hover:bg-foreground/[0.05]",
            style: "height:{cell_height}px;",
            onclick: move |_| on_pick.call(row),
            span {
                class: "sticky left-0 flex shrink-0 select-none items-center justify-end bg-background pl-4 pr-5 tabular-nums text-muted-foreground/60",
                style: "min-width:calc(var(--cw, 1ch) * {gutter_chars} + 3rem);height:{cell_height}px;",
                span {
                    class: "shrink-0 text-right",
                    style: "width:calc(var(--cw, 1ch) * {gutter_chars});",
                    "{row + 1}"
                }
                span { class: "ml-1 w-[1ch] shrink-0" }
            }
            span { class: "pointer-events-none relative whitespace-pre pr-8",
                IndentGuides { levels: line.indent_levels }
                for (i, s) in line.spans.iter().enumerate() {
                    span { key: "{i}", style: "{span_style(s)}", "{s.text}" }
                }
            }
        }
    }
}

#[component]
fn IndentGuides(levels: u16) -> Element {
    rsx! {
        for level in 0..levels {
            div {
                key: "{level}",
                class: "pointer-events-none absolute inset-y-0 w-px bg-foreground/[0.09]",
                style: "left:calc(var(--cw, 1ch) * var(--iw, 4) * {level});",
            }
        }
    }
}

#[component]
fn FoldMarker(line: u32, collapsed: bool, revealed: bool) -> Element {
    let tone = match collapsed {
        true => "text-foreground/60 opacity-100",
        false if revealed => "text-foreground/35 opacity-100",
        false => "text-foreground/35 opacity-0",
    };
    rsx! {
        span {
            class: "absolute right-0.5 flex h-full w-4 cursor-pointer items-center justify-center transition-opacity hover:!text-foreground {tone}",
            onmousedown: move |e: Event<MouseData>| {
                e.stop_propagation();
                e.prevent_default();
                let _ = send(&FileFoldToggle { line });
            },
            svg {
                class: if collapsed { "h-3 w-3 -rotate-90" } else { "h-3 w-3" },
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "m6 9 6 6 6-6" }
            }
        }
    }
}
