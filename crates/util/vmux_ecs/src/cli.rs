use std::collections::BTreeMap;
use std::ffi::OsString;
use std::thread;

use bevy_ecs::prelude::*;

#[derive(Clone, Resource)]
pub struct CliWake(thread::Thread);

impl CliWake {
    pub fn current() -> Self {
        Self(thread::current())
    }

    pub fn wake(&self) {
        self.0.unpark();
    }
}

#[derive(Clone, Component, Debug)]
pub struct CliInvocation {
    pub command: String,
    pub arguments: BTreeMap<String, Vec<OsString>>,
}

impl CliInvocation {
    pub fn is(&self, command: &str) -> bool {
        self.command == command
    }

    pub fn value(&self, id: &str) -> Option<&str> {
        self.arguments
            .get(id)
            .and_then(|values| values.first())
            .and_then(|value| value.to_str())
    }

    pub fn value_os(&self, id: &str) -> Option<&std::ffi::OsStr> {
        self.arguments
            .get(id)
            .and_then(|values| values.first())
            .map(OsString::as_os_str)
    }

    pub fn flag(&self, id: &str) -> bool {
        self.value(id) == Some("true")
    }
}

#[derive(Component, Debug)]
pub struct CliResult(pub Result<u8, String>);

impl Default for CliResult {
    fn default() -> Self {
        Self(Ok(0))
    }
}

impl From<std::io::Result<i32>> for CliResult {
    fn from(result: std::io::Result<i32>) -> Self {
        Self(
            result
                .map(|code| u8::try_from(code).unwrap_or(1))
                .map_err(|error| error.to_string()),
        )
    }
}

impl From<std::io::Result<()>> for CliResult {
    fn from(result: std::io::Result<()>) -> Self {
        Self(result.map(|()| 0).map_err(|error| error.to_string()))
    }
}
