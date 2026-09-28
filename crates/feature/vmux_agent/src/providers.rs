pub mod anthropic;
#[cfg(feature = "app")]
#[path = "host/providers/anthropic_plugin.rs"]
pub mod anthropic_plugin;
#[cfg(feature = "app")]
#[path = "host/providers/builtin.rs"]
mod builtin;
pub mod mistral;
#[cfg(feature = "app")]
#[path = "host/providers/mistral_plugin.rs"]
pub mod mistral_plugin;
pub mod openai;
#[cfg(feature = "app")]
#[path = "host/providers/openai_plugin.rs"]
pub mod openai_plugin;
pub mod openai_shared;

#[cfg(feature = "app")]
pub use builtin::{BUILTIN_PROVIDERS, BuiltinProvider, ECHO_DEFAULT, resolve_default_app_provider};
