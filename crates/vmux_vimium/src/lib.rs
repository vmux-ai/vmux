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
mod overlay;
#[cfg(target_arch = "wasm32")]
mod runtime;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    runtime::install();
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
