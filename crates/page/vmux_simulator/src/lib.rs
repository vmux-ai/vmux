//! Mirrors a booted iOS Simulator at `vmux://simulator/ios/<version>` and forwards input to it.
//!
//! Video and input both go through the `axe` CLI, which injects HID events into the guest.
//! Nothing here drives the host Simulator.app window: `CGEventPostToPid` is silently dropped by
//! Simulator.app, and a global event tap needs that window visible and unobstructed, which a
//! mirror drawn over it cannot guarantee. Reaching the guest instead also means neither Screen
//! Recording nor Accessibility permission is involved.

pub mod event;
pub mod url;

#[cfg(ui)]
pub mod page;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
