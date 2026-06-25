pub mod keymap;
pub mod mode;

#[cfg(not(target_arch = "wasm32"))]
pub fn preload_script() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/vimium_preload.js"))
}

pub fn is_web_scheme(uri: &str) -> bool {
    uri.starts_with("http://") || uri.starts_with("https://")
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    let document = web_sys::window().unwrap().document().unwrap();
    let marker = document.create_element("div").unwrap();
    marker.set_id("__vmux_vimium_marker");
    let html: web_sys::HtmlElement = marker.dyn_into().unwrap();
    html.set_inner_text("vmux vimium ok");
    let style = html.style();
    let _ = style.set_property("position", "fixed");
    let _ = style.set_property("bottom", "8px");
    let _ = style.set_property("right", "8px");
    let _ = style.set_property("z-index", "2147483647");
    let _ = style.set_property("background", "rgba(0,0,0,0.8)");
    let _ = style.set_property("color", "#0f0");
    let _ = style.set_property("font", "12px monospace");
    let _ = style.set_property("padding", "2px 6px");
    let _ = style.set_property("border-radius", "4px");
    if let Some(body) = document.body() {
        let _ = body.append_child(&html);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_scheme_detection() {
        assert!(is_web_scheme("https://example.com"));
        assert!(is_web_scheme("http://example.com"));
        assert!(!is_web_scheme("vmux://history/"));
        assert!(!is_web_scheme("file:///tmp/x"));
        assert!(!is_web_scheme("data:text/html,x"));
    }
}
