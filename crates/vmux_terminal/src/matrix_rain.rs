//! Matrix-style digital rain canvas rendered behind the agent loading console.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const FONT_PX: f64 = 16.0;
const GLYPHS: &str = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789";

/// Full-bleed Matrix rain. `accent_rgb` is a `"r g b"` triple (from
/// `AgentAccent::rain_rgb`); `words` are uppercased agent tokens woven into a few
/// columns so the agent name stays legible in the rain.
#[component]
pub fn MatrixRain(accent_rgb: String, words: Vec<String>) -> Element {
    let canvas_id = use_hook(|| format!("matrix-rain-{}", (js_sys::Math::random() * 1.0e9) as u64));
    let running: Rc<RefCell<bool>> = use_hook(|| Rc::new(RefCell::new(true)));
    let raf: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let handle: Rc<RefCell<Option<i32>>> = use_hook(|| Rc::new(RefCell::new(None)));

    use_effect({
        let canvas_id = canvas_id.clone();
        let accent_rgb = accent_rgb.clone();
        let words = words.clone();
        let running = running.clone();
        let raf = raf.clone();
        let handle = handle.clone();
        move || {
            start_rain(
                canvas_id.clone(),
                accent_rgb.clone(),
                words.clone(),
                running.clone(),
                raf.clone(),
                handle.clone(),
            );
        }
    });

    use_drop({
        let running = running.clone();
        let raf = raf.clone();
        let handle = handle.clone();
        move || {
            *running.borrow_mut() = false;
            if let Some(id) = handle.borrow_mut().take()
                && let Some(win) = web_sys::window()
            {
                let _ = win.cancel_animation_frame(id);
            }
            *raf.borrow_mut() = None;
        }
    });

    rsx! {
        canvas { id: "{canvas_id}", class: "absolute inset-0 h-full w-full" }
    }
}

fn brighten(accent_rgb: &str) -> String {
    let parts: Vec<u16> = accent_rgb
        .split_whitespace()
        .filter_map(|p| p.parse::<u16>().ok())
        .collect();
    if parts.len() != 3 {
        return "rgb(220 230 255)".to_string();
    }
    let mix = |c: u16| -> u16 { c + (255 - c) * 7 / 10 };
    format!("rgb({} {} {})", mix(parts[0]), mix(parts[1]), mix(parts[2]))
}

/// Scale an `"r g b"` accent toward black by `pct`%, returning a bare `"r g b"`
/// triple (no wrapper) so callers can build `rgb(...)` or `rgb(... / a)`. Used
/// for the light-mode rain, where the dark-mode `brighten` would vanish.
fn darken(accent_rgb: &str, pct: u16) -> String {
    let parts: Vec<u16> = accent_rgb
        .split_whitespace()
        .filter_map(|p| p.parse::<u16>().ok())
        .collect();
    if parts.len() != 3 {
        return "20 24 33".to_string();
    }
    let mix = |c: u16| -> u16 { c * pct / 100 };
    format!("{} {} {}", mix(parts[0]), mix(parts[1]), mix(parts[2]))
}

fn pick_glyph(glyphs: &[char], words: &[Vec<char>], col: usize, head_row: f64) -> char {
    if !words.is_empty() && col % 7 == 3 {
        let word = &words[col % words.len()];
        if !word.is_empty() {
            let idx = (head_row.max(0.0) as usize) % word.len();
            return word[idx];
        }
    }
    let r = (js_sys::Math::random() * glyphs.len() as f64) as usize;
    glyphs[r.min(glyphs.len() - 1)]
}

fn start_rain(
    canvas_id: String,
    accent_rgb: String,
    words: Vec<String>,
    running: Rc<RefCell<bool>>,
    raf: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    handle: Rc<RefCell<Option<i32>>>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(el) = document.get_element_by_id(&canvas_id) else {
        return;
    };
    let Ok(canvas) = el.dyn_into::<web_sys::HtmlCanvasElement>() else {
        return;
    };
    let Ok(Some(ctx_obj)) = canvas.get_context("2d") else {
        return;
    };
    let Ok(ctx) = ctx_obj.dyn_into::<web_sys::CanvasRenderingContext2d>() else {
        return;
    };

    let dpr = window.device_pixel_ratio().max(1.0);

    let reduced = window
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .map(|m| m.matches())
        .unwrap_or(false);

    let dark = window
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
        .map(|m| m.matches())
        .unwrap_or(true);
    let bg = if dark {
        "rgb(30 30 46)"
    } else {
        "rgb(209 213 219)"
    };
    let fade = if dark {
        "rgba(30, 30, 46, 0.08)"
    } else {
        "rgba(209, 213, 219, 0.1)"
    };

    if reduced {
        let w = canvas.client_width().max(1) as f64;
        let h = canvas.client_height().max(1) as f64;
        canvas.set_width((w * dpr) as u32);
        canvas.set_height((h * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);
        ctx.set_fill_style_str(bg);
        ctx.fill_rect(0.0, 0.0, w, h);
        return;
    }

    let glyphs: Vec<char> = GLYPHS.chars().collect();
    let word_chars: Vec<Vec<char>> = words
        .iter()
        .filter(|w| !w.is_empty())
        .map(|w| w.chars().collect())
        .collect();
    let head_color = if dark {
        brighten(&accent_rgb)
    } else {
        format!("rgb({})", darken(&accent_rgb, 42))
    };
    let trail_color = if dark {
        format!("rgb({accent_rgb} / 0.85)")
    } else {
        format!("rgb({} / 0.8)", darken(&accent_rgb, 62))
    };

    let mut cols = (canvas.client_width().max(1) as f64 / FONT_PX)
        .floor()
        .max(1.0) as usize;
    let mut drops: Vec<f64> = (0..cols)
        .map(|_| -(js_sys::Math::random() * 40.0))
        .collect();

    let win = window.clone();
    let raf_inner = raf.clone();
    let running_inner = running.clone();
    let handle_inner = handle.clone();
    let closure = Closure::wrap(Box::new(move || {
        let w = canvas.client_width().max(1) as f64;
        let h = canvas.client_height().max(1) as f64;
        let want_w = (w * dpr) as u32;
        let want_h = (h * dpr) as u32;
        if canvas.width() != want_w || canvas.height() != want_h {
            canvas.set_width(want_w);
            canvas.set_height(want_h);
            let _ = ctx.reset_transform();
            let _ = ctx.scale(dpr, dpr);
            let new_cols = (w / FONT_PX).floor().max(1.0) as usize;
            if new_cols != cols {
                drops.resize_with(new_cols, || -(js_sys::Math::random() * 40.0));
                cols = new_cols;
            }
        }

        ctx.set_font(&format!("{FONT_PX}px monospace"));
        ctx.set_text_baseline("top");

        ctx.set_fill_style_str(fade);
        ctx.fill_rect(0.0, 0.0, w, h);

        for (i, drop) in drops.iter_mut().enumerate().take(cols) {
            let x = i as f64 * FONT_PX;
            let head_row = *drop;
            let y = head_row * FONT_PX;
            if y >= 0.0 {
                let ch = pick_glyph(&glyphs, &word_chars, i, head_row).to_string();
                ctx.set_fill_style_str(&trail_color);
                let _ = ctx.fill_text(&ch, x, y);
                ctx.set_fill_style_str(&head_color);
                let _ = ctx.fill_text(&ch, x, y);
            }
            if y > h && js_sys::Math::random() > 0.975 {
                *drop = 0.0;
            } else {
                *drop += 1.0;
            }
        }

        if *running_inner.borrow()
            && let Some(cb) = raf_inner.borrow().as_ref()
            && let Ok(id) = win.request_animation_frame(cb.as_ref().unchecked_ref())
        {
            *handle_inner.borrow_mut() = Some(id);
        }
    }) as Box<dyn FnMut()>);

    *raf.borrow_mut() = Some(closure);
    if let Some(cb) = raf.borrow().as_ref()
        && let Ok(id) = window.request_animation_frame(cb.as_ref().unchecked_ref())
    {
        *handle.borrow_mut() = Some(id);
    }
}
