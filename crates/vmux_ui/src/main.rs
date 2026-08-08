//! Web binary entry: [`dioxus::launch`] → [`vmux_ui::components::app::App`] (wasm32 only).

#[cfg(web)]
fn main() {
    dioxus::launch(vmux_ui::components::app::App);
}

#[cfg(not(web))]
fn main() {
    eprintln!("vmux_ui: wasm binary is for wasm32 (see build.rs).");
}
