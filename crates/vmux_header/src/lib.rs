pub mod event;

#[cfg(not(target_arch = "wasm32"))]
pub mod bundle;

#[cfg(not(target_arch = "wasm32"))]
pub mod system;

#[cfg(not(target_arch = "wasm32"))]
pub use bundle::{Header, HeaderBundle, HEADER_WEBVIEW_URL};

#[cfg(not(target_arch = "wasm32"))]
pub use system::{NavigationState, PageMetadata, HEADER_HEIGHT_PX};

#[cfg(not(target_arch = "wasm32"))]
include!("plugin.rs");
