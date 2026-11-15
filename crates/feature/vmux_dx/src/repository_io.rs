use crate::environment::Environment;
use crate::execution::Execution;
use bevy_ecs::prelude::Component;
use std::path::PathBuf;
use std::process::Output;
use vmux_ecs::blocking::Lane;
use vmux_ecs::jobs::Job;

#[derive(Component, Clone)]
#[component(immutable)]
pub struct Read {
    pub root: PathBuf,
    pub arguments: Vec<&'static str>,
}

impl Job for Read {
    type Output = Result<Output, String>;

    fn run(self) -> Self::Output {
        let mut command = Environment::current().command("git", &self.root);
        command.args(self.arguments);
        let output = Execution::new(command)
            .output_any(None)
            .map_err(|error| error.to_string())?;
        if output.stdout.len() > 1024 * 1024 || output.stderr.len() > 1024 * 1024 {
            return Err("Git repository metadata exceeds 1 MiB".into());
        }
        Ok(output)
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }
}
