/// Configuration for CEF command line switches.
///
/// Used to customize CEF behavior at startup.
///
/// # Default Switches
///
/// No switches are enabled by default.
///
/// # Example
///
/// ```no_run
/// use bevy_cef_core::prelude::CommandLineConfig;
///
/// // Add switches while preserving defaults (recommended)
/// let config = CommandLineConfig::default()
///     .with_switch("disable-gpu")
///     .with_switch_value("remote-debugging-port", "9222");
///
/// // Or use direct initialization (replaces defaults)
/// let config = CommandLineConfig {
///     switches: vec!["disable-gpu"],
///     switch_values: vec![("remote-debugging-port", "9222")],
/// };
/// ```
#[derive(Clone, Debug)]
pub struct CommandLineConfig {
    pub switches: Vec<&'static str>,
    pub switch_values: Vec<(&'static str, &'static str)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_real_macos_keychain() {
        let forbidden = ["use", "-mock", "-keychain"].concat();

        assert!(
            !CommandLineConfig::default()
                .switches
                .contains(&forbidden.as_str())
        );
    }
}

impl Default for CommandLineConfig {
    fn default() -> Self {
        Self {
            switches: Vec::new(),
            switch_values: Vec::new(),
        }
    }
}

impl CommandLineConfig {
    /// Add a command line switch (e.g., "disable-gpu", "disable-web-security").
    pub fn with_switch(mut self, name: &'static str) -> Self {
        self.switches.push(name);
        self
    }

    /// Add a command line switch with a value (e.g., "remote-debugging-port", "9222").
    pub fn with_switch_value(mut self, name: &'static str, value: &'static str) -> Self {
        self.switch_values.push((name, value));
        self
    }
}
