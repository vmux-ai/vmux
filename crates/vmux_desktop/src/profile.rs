pub(crate) use vmux_layout::profile::*;

// Serializes process-wide env mutations across tests (HOME, PATH, etc.).
// Any test that calls std::env::set_var or remove_var must hold this lock.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
