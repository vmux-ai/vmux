#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_terminal::event::*;
use vmux_ui::hooks::{use_event_listener, use_theme};

#[component]
pub fn App() -> Element {
    use_theme();
    let mut viewport = use_signal(TermViewportEvent::default);

    let _listener = use_event_listener::<TermViewportEvent, _>(
        TERM_VIEWPORT_EVENT,
        move |data| {
            viewport.set(data);
        },
    );

    let vp = viewport();

    // Install raw JS keydown handler for reliable key capture in CEF OSR.
    use_effect(|| {
        document::eval(
            r#"setTimeout(() => {
  var el = document.getElementById('term-input');
  if (!el) return;
  el.focus();
  if (el._bound) return;
  el._bound = true;
  el.addEventListener('keydown', function(e) {
    e.preventDefault();
    e.stopPropagation();
    var mods = 0;
    if (e.ctrlKey) mods |= 1;
    if (e.altKey) mods |= 2;
    if (e.shiftKey) mods |= 4;
    if (e.metaKey) mods |= 8;
    var text = e.key.length === 1 ? e.key : null;
    window.__cef_emit('term_key', {key: e.code, modifiers: mods, text: text});
  }, true);
}, 100);"#,
        );
    });

    rsx! {
        div {
            class: "relative h-full w-full overflow-hidden bg-background font-mono text-sm leading-tight",
            onclick: move |_| {
                document::eval("document.getElementById('term-input')?.focus()");
            },
            textarea {
                id: "term-input",
                class: "absolute opacity-0 w-0 h-0",
                autofocus: true,
            }
            div { class: "p-1",
                for (row_idx , line) in vp.lines.iter().enumerate() {
                    div {
                        key: "{row_idx}",
                        class: "flex whitespace-pre",
                        style: "height: 1.2em;",
                        for (span_idx , span) in line.spans.iter().enumerate() {
                            span {
                                key: "{span_idx}",
                                style: "{span_style(span)}",
                                "{span.text}"
                            }
                        }
                        if row_idx == vp.cursor.row as usize && vp.cursor.visible {
                            span {
                                class: "absolute",
                                style: "left: calc(0.25rem + {vp.cursor.col}ch); background: var(--foreground); color: var(--background); animation: blink 1s step-end infinite;",
                                "{cursor_char(&vp, row_idx)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn span_style(span: &TermSpan) -> String {
    let mut parts = Vec::new();
    if let Some([r, g, b]) = span.fg {
        parts.push(format!("color:rgb({r},{g},{b})"));
    }
    if let Some([r, g, b]) = span.bg {
        parts.push(format!("background:rgb({r},{g},{b})"));
    }
    if span.flags & FLAG_BOLD != 0 {
        parts.push("font-weight:bold".into());
    }
    if span.flags & FLAG_ITALIC != 0 {
        parts.push("font-style:italic".into());
    }
    if span.flags & FLAG_UNDERLINE != 0 {
        parts.push("text-decoration:underline".into());
    }
    if span.flags & FLAG_STRIKETHROUGH != 0 {
        parts.push("text-decoration:line-through".into());
    }
    if span.flags & FLAG_DIM != 0 {
        parts.push("opacity:0.5".into());
    }
    parts.join(";")
}

fn cursor_char(vp: &TermViewportEvent, row: usize) -> String {
    if let Some(line) = vp.lines.get(row) {
        let col = vp.cursor.col as usize;
        let mut pos = 0;
        for span in &line.spans {
            for c in span.text.chars() {
                if pos == col {
                    return c.to_string();
                }
                pos += 1;
            }
        }
    }
    " ".to_string()
}
