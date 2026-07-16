#![allow(clippy::all)]

#[cfg(feature = "browser-process")]
mod browser_process;
#[cfg(target_os = "macos")]
mod debug;

mod dom_snapshot;
mod render_process;
mod util;

pub mod prelude {
    #[cfg(feature = "browser-process")]
    pub use crate::browser_process::*;
    #[cfg(target_os = "macos")]
    pub use crate::debug::*;
    pub use crate::dom_snapshot::*;
    pub use crate::render_process::app::*;
    pub use crate::render_process::execute_render_process;
    pub use crate::render_process::render_process_handler::*;
    pub use crate::util::*;
}
