use dioxus::prelude::*;

pub use super::gallery::UiLibraryGallery;

#[component]
pub fn App() -> Element {
    rsx! {
        UiLibraryGallery {}
    }
}
