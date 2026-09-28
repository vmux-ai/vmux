pub use vmux_api::service as event;

pub mod remote;

#[cfg(host)]
mod host;
#[cfg(host)]
pub use host::*;
