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

/// Native `cargo check` / `cargo build` do not compile the Dioxus stack; use
/// `dx build --platform web` or `cargo build --target wasm32-unknown-unknown`.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "vmux_status_bar: this binary is the Dioxus web UI; build for wasm32-unknown-unknown (e.g. dx build --platform web)."
    );
}
