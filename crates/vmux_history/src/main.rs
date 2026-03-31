//! Web binary entry: [`dioxus::launch`] → [`app::App`] (wasm32 only).

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod cef;
#[cfg(target_arch = "wasm32")]
mod payload;

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "vmux_history: wasm32 only — from repo root run `cargo build -p vmux_history --target wasm32-unknown-unknown` (native `cargo build -p vmux_history` runs the web_dist pipeline in build.rs)."
    );
}
