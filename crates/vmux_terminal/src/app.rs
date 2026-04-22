#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_terminal::event::*;
use vmux_ui::hooks::{use_event_listener, use_theme};

#[component]
pub fn App() -> Element {
    use_theme();
    let mut viewport = use_signal(TermViewportEvent::default);
    let mut theme = use_signal(|| None::<TermThemeEvent>);

    let _listener = use_event_listener::<TermViewportEvent, _>(
        TERM_VIEWPORT_EVENT,
        move |data| {
            viewport.set(data);
        },
    );

    let _theme_listener = use_event_listener::<TermThemeEvent, _>(
        TERM_THEME_EVENT,
        move |data| {
            theme.set(Some(data));
        },
    );

    let vp = viewport();

    // Install resize observer to report character cell dimensions.
    use_effect(|| {
        document::eval(
            r#"setTimeout(() => {
  var measure = document.createElement('span');
  measure.style.cssText = 'position:absolute;visibility:hidden;white-space:pre;font:inherit';
  measure.className = 'font-mono text-sm';
  measure.textContent = 'X';
  var container = document.querySelector('.font-mono');
  if (container) container.appendChild(measure);
  function emitResize() {
    var cw = measure.getBoundingClientRect().width;
    var ch = parseFloat(getComputedStyle(container).lineHeight) || measure.getBoundingClientRect().height;
    var padEl = container.firstElementChild || container;
    var cs = getComputedStyle(padEl);
    var px = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
    var py = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    var vw = container.clientWidth - px;
    var vh = container.clientHeight - py;
    if (cw > 0 && ch > 0 && window.cef && window.cef.emit) {
      window.cef.emit({char_width: cw, char_height: ch, viewport_width: vw, viewport_height: vh});
    }
  }
  emitResize();
  if (window.ResizeObserver) {
    new ResizeObserver(emitResize).observe(document.body);
  }
}, 100);"#,
        );
    });

    let theme_style = {
        let t = theme();
        match t {
            Some(t) => {
                let [fr, fg, fb] = t.foreground;
                let [br, bg, bb] = t.background;
                let [cr, cg, cb] = t.cursor;
                let mut s = format!(
                    "--term-fg:rgb({fr},{fg},{fb});--term-bg:rgb({br},{bg},{bb});--term-cursor:rgb({cr},{cg},{cb});"
                );
                for (i, [r, g, b]) in t.ansi.iter().enumerate() {
                    s.push_str(&format!("--ansi-{i}:rgb({r},{g},{b});"));
                }
                if !t.font_family.is_empty() {
                    s.push_str(&format!("font-family:\"{}\",monospace;", t.font_family));
                }
                if t.font_size > 0.0 {
                    s.push_str(&format!("font-size:{}px;", t.font_size));
                }
                if t.line_height > 0.0 {
                    s.push_str(&format!("line-height:{};", t.line_height));
                }
                s
            }
            None => String::new(),
        }
    };

    let padding = theme().map(|t| t.padding).unwrap_or(4.0);
    let cursor_blink = theme().map(|t| t.cursor_blink).unwrap_or(true);
    let cursor_style = theme().map(|t| t.cursor_style.clone()).unwrap_or_else(|| "block".into());

    rsx! {
        div {
            class: "relative h-full w-full overflow-hidden bg-term-bg text-term-fg font-mono text-sm leading-tight",
            style: "{theme_style}",

            div { style: "padding:{padding}px;",
                for (row_idx , line) in vp.lines.iter().enumerate() {
                    div {
                        key: "{row_idx}",
                        class: "flex whitespace-pre",
                        style: "height: 1.2em;",
                        for (span_idx , span) in line.spans.iter().enumerate() {
                            span {
                                key: "{span_idx}",
                                class: "{span_classes(span)}",
                                style: "{span_inline_style(span)}",
                                "{span.text}"
                            }
                        }
                        if row_idx == vp.cursor.row as usize && vp.cursor.visible {
                            {
                                let blink_css = if cursor_blink { "animation:blink 1s step-end infinite;" } else { "" };
                                let cursor_cls = match cursor_style.as_str() {
                                    "underline" => "absolute border-b-2 border-term-cursor",
                                    "bar" => "absolute border-l-2 border-term-cursor",
                                    _ => "absolute bg-term-cursor",
                                };
                                let color_css = if cursor_style == "block" { "color:var(--term-bg);" } else { "" };
                                rsx! {
                                    span {
                                        class: "{cursor_cls}",
                                        style: "left:calc({padding}px + {vp.cursor.col}ch);{color_css}{blink_css}",
                                        "{cursor_char(&vp, row_idx)}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn span_classes(span: &TermSpan) -> String {
    let mut classes = Vec::new();

    let (fg, bg) = if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    };

    match fg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("text-term-bg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("text-ansi-{i}")),
        TermColor::Rgb(..) => {}
    }

    match bg {
        TermColor::Default => {
            if span.flags & FLAG_INVERSE != 0 {
                classes.push("bg-term-fg".into());
            }
        }
        TermColor::Indexed(i) => classes.push(format!("bg-ansi-{i}")),
        TermColor::Rgb(..) => {}
    }

    if span.flags & FLAG_BOLD != 0 { classes.push("font-bold".into()); }
    if span.flags & FLAG_ITALIC != 0 { classes.push("italic".into()); }
    if span.flags & FLAG_UNDERLINE != 0 { classes.push("underline".into()); }
    if span.flags & FLAG_STRIKETHROUGH != 0 { classes.push("line-through".into()); }
    if span.flags & FLAG_DIM != 0 { classes.push("opacity-50".into()); }

    classes.join(" ")
}

fn span_inline_style(span: &TermSpan) -> String {
    let mut parts = Vec::new();

    let (fg, bg) = if span.flags & FLAG_INVERSE != 0 {
        (&span.bg, &span.fg)
    } else {
        (&span.fg, &span.bg)
    };

    if let TermColor::Rgb(r, g, b) = fg {
        parts.push(format!("color:rgb({r},{g},{b})"));
    }
    if let TermColor::Rgb(r, g, b) = bg {
        parts.push(format!("background:rgb({r},{g},{b})"));
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
