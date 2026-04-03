//! Web binary entry: [`dioxus::launch`] → [`vmux_ui::components::app::App`] (wasm32 only).

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(vmux_ui::components::app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("vmux_ui: wasm binary is for wasm32 (see build.rs).");
}
