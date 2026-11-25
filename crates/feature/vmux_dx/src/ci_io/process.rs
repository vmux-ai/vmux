use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Output;

use bevy_ecs::prelude::Component;
use vmux_job::Job;
use vmux_job::blocking::Lane;

use crate::environment::Environment;
use crate::execution::Execution;

use super::IoJob;

#[derive(Clone)]
pub(crate) struct Capture {
    pub(crate) root: PathBuf,
    pub(crate) environment: Environment,
    pub(crate) program: OsString,
    pub(crate) arguments: Vec<OsString>,
    pub(crate) input: Option<Vec<u8>>,
}

impl IoJob for Capture {
    type Data = Captured;
}

impl Job for Capture {
    type Output = Result<Captured, String>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        let mut command = self.environment.command(self.program, &self.root);
        command.args(self.arguments);
        Execution::new(command)
            .output_any(self.input.as_deref())
            .map(Captured)
            .map_err(|error| error.to_string())
    }
}

#[derive(Component, Clone)]
pub(crate) struct Captured(pub Output);

impl Captured {
    pub(crate) fn text(&self) -> Result<&str, String> {
        if !self.0.status.success() {
            return Err(format!(
                "CI subprocess failed: {}",
                String::from_utf8_lossy(&self.0.stderr)
            ));
        }
        std::str::from_utf8(&self.0.stdout).map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub(crate) struct ProbeCapture(pub Capture);

impl IoJob for ProbeCapture {
    type Data = ProbedCapture;
}

impl Job for ProbeCapture {
    type Output = Result<ProbedCapture, String>;

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn run(self) -> Self::Output {
        Ok(ProbedCapture(self.0.run()))
    }
}

#[derive(Component, Clone)]
pub(crate) struct ProbedCapture(pub Result<Captured, String>);

impl ProbedCapture {
    pub(crate) fn text(&self) -> Result<&str, String> {
        match &self.0 {
            Ok(output) => output.text(),
            Err(error) => Err(error.clone()),
        }
    }
}
