#[cfg(target_arch = "wasm32")]
use vmux_history_poc::App;

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
