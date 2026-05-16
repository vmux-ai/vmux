#[cfg(target_arch = "wasm32")]
mod app;

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("vmux_settings: wasm binary is for wasm32 (see build.rs).");
}
