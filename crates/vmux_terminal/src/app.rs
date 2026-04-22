#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_terminal::event::*;
use vmux_ui::hooks::{use_event_listener, use_theme};

// Tailwind safelist — these classes are generated dynamically via format!() and
// must appear as literal strings for Tailwind's content scanner to detect them.
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
];

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

    // Install resize observer and mouse event handlers.
    use_effect(|| {
        document::eval(
            r#"setTimeout(() => {
  // Load Nerd Font programmatically (CSS @font-face may not resolve
  // relative URLs correctly under the custom vmux:// scheme).
  (async function() {
    try {
      var variants = [
        ['JetBrainsMonoNerdFontMono-Regular.woff2', '400', 'normal'],
        ['JetBrainsMonoNerdFontMono-Bold.woff2', '700', 'normal'],
        ['JetBrainsMonoNerdFontMono-Italic.woff2', '400', 'italic'],
        ['JetBrainsMonoNerdFontMono-BoldItalic.woff2', '700', 'italic'],
      ];
      for (var v of variants) {
        var resp = await fetch('./assets/fonts/' + v[0]);
        if (resp.ok) {
          var buf = await resp.arrayBuffer();
          var face = new FontFace('JetBrainsMono NF', buf, {weight: v[1], style: v[2]});
          await face.load();
          document.fonts.add(face);
        }
      }
    } catch(e) { /* font loading is best-effort */ }
  })();

  var measure = document.createElement('span');
  measure.style.cssText = 'position:absolute;visibility:hidden;white-space:pre;font:inherit';
  measure.className = 'font-mono text-sm';
  measure.textContent = 'X';
  var container = document.querySelector('.font-mono');
  if (container) container.appendChild(measure);

  // Shared state for mouse position calculation
  window.__termCW = 0;
  window.__termCH = 0;
  window.__termButtons = 0;
  window.__termLastCol = -1;
  window.__termLastRow = -1;

  function emitResize() {
    var cw = measure.getBoundingClientRect().width;
    var ch = parseFloat(getComputedStyle(container).lineHeight) || measure.getBoundingClientRect().height;
    var padEl = container.firstElementChild || container;
    var cs = getComputedStyle(padEl);
    var px = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
    var py = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    var vw = container.clientWidth - px;
    var vh = container.clientHeight - py;
    window.__termCW = cw;
    window.__termCH = ch;
    if (cw > 0 && ch > 0 && window.cef && window.cef.emit) {
      window.cef.emit({char_width: cw, char_height: ch, viewport_width: vw, viewport_height: vh});
    }
  }

  function mousePos(e) {
    var padEl = container.firstElementChild;
    if (!padEl || window.__termCW <= 0 || window.__termCH <= 0) return null;
    var cs = getComputedStyle(padEl);
    var rect = padEl.getBoundingClientRect();
    var x = e.clientX - rect.left - parseFloat(cs.paddingLeft);
    var y = e.clientY - rect.top - parseFloat(cs.paddingTop);
    return {
      col: Math.max(0, Math.floor(x / window.__termCW)),
      row: Math.max(0, Math.floor(y / window.__termCH))
    };
  }

  function mouseMods(e) {
    var m = 0;
    if (e.ctrlKey) m |= 1;
    if (e.altKey) m |= 2;
    if (e.shiftKey) m |= 4;
    if (e.metaKey) m |= 8;
    return m;
  }

  container.addEventListener('mousedown', function(e) {
    var pos = mousePos(e);
    if (!pos || !window.cef || !window.cef.emit) return;
    window.__termButtons |= (1 << e.button);
    window.cef.emit({button: e.button, col: pos.col, row: pos.row, modifiers: mouseMods(e), pressed: true, moving: false});
  });

  container.addEventListener('mouseup', function(e) {
    var pos = mousePos(e);
    if (!pos || !window.cef || !window.cef.emit) return;
    window.__termButtons &= ~(1 << e.button);
    window.cef.emit({button: e.button, col: pos.col, row: pos.row, modifiers: mouseMods(e), pressed: false, moving: false});
  });

  container.addEventListener('mousemove', function(e) {
    var pos = mousePos(e);
    if (!pos || !window.cef || !window.cef.emit) return;
    // Only emit if cell position changed (throttle)
    if (pos.col === window.__termLastCol && pos.row === window.__termLastRow) return;
    window.__termLastCol = pos.col;
    window.__termLastRow = pos.row;
    var btn = 3;
    if (window.__termButtons & 1) btn = 0;
    else if (window.__termButtons & 4) btn = 2;
    else if (window.__termButtons & 2) btn = 1;
    window.cef.emit({button: btn, col: pos.col, row: pos.row, modifiers: mouseMods(e), pressed: true, moving: true});
  });

  container.addEventListener('contextmenu', function(e) { e.preventDefault(); });

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
                    // Always include bundled Nerd Font as fallback for PUA glyphs
                    s.push_str(&format!(
                        "font-family:\"{}\",\"JetBrainsMono NF\",monospace;",
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
