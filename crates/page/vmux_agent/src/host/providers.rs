pub mod anthropic_plugin;
pub mod mistral_plugin;
pub mod openai_plugin;

pub use vmux_service::providers::{anthropic, builtin, mistral, openai, openai_shared};

pub use builtin::{BUILTIN_PROVIDERS, BuiltinProvider, ECHO_DEFAULT, resolve_default_app_provider};
