//! Web binary entry: [`dioxus::launch`] → [`app::App`] (wasm32 only).

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod bridge;
#[cfg(target_arch = "wasm32")]
mod payload;

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(app::App);
}

/// This `main` is only used for the wasm32 Dioxus bundle. Host `cargo build -p vmux_status_bar` still
/// builds the library + `build.rs` (wasm-bindgen → `dist/`); this stub runs if you execute the host
/// binary target by mistake.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "vmux_status_bar: this binary is the Dioxus web UI; build for wasm32 (see crate `build.rs`)."
    );
}
