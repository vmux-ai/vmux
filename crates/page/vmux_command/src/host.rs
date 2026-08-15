//! Everything that needs a host to run on, rather than the browser bundle.
//!
//! One `not(web)` gate for the lot, rather than an attribute on each declaration: these modules
//! are built on Bevy, which the wasm page bundle does not link. The crate's public paths are
//! unchanged — `lib.rs` re-exports this module's contents, so `vmux_command::open` still resolves
//! from outside and `crate::open` still resolves from within.

/// The `vmux://command-bar/` page this crate serves.
///
/// `command_bar: false` because the launcher does not list itself.
pub const COMMAND_BAR_PAGE_MANIFEST: vmux_core::page::PageManifest =
    vmux_core::page::PageManifest {
        host: "command-bar",
        title: "Command Bar",
        title_message_id: None,
        replaces_command: None,
        keywords: &[],
        icon: None,
        command_bar: false,
    };

pub mod plugin;
pub use plugin::CommandPlugin;

pub mod bundle;
pub mod command;
pub mod issued;
pub mod open;
pub mod page_key;
pub mod payload;
pub mod shortcut;
pub mod snapshot;
pub mod surface;

pub use bundle::{COMMAND_BAR_PAGE_URL, CommandBar};
pub use command::*;
pub use issued::{CommandIssued, CommandIssuer};
pub use open::*;
pub use page_key::{PageKeyPlugin, ScopedKeys};
pub use payload::{
    CommandBarEntry, build_command_bar_open_payload, command_bar_open_payload, command_list,
    localized_command_name,
};
pub use snapshot::*;
