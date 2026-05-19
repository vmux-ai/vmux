#[cfg(target_arch = "wasm32")]
mod page;

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(page::Page);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("vmux_setting: wasm binary is for wasm32 (see build.rs).");
}
