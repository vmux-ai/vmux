use std::fmt;
use std::io::Write;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, ExitStatus, Output, Stdio};

use crate::Result;
use crate::signals::Signals;

pub struct Execution {
    command: Command,
}

impl Execution {
    pub fn new(command: Command) -> Self {
        Self { command }
    }

    pub fn run(&mut self) -> Result<()> {
        Signals::check()?;
        self.command.process_group(0);
        let mut child = self.command.spawn()?;
        Signals::track(child.id());
        let status = child.wait();
        Signals::clear_child();
        Signals::check()?;
        let status = status?;
        if !status.success() {
            return Err(Failure::status(status).into());
        }
        Ok(())
    }

    pub fn output(&mut self, input: Option<&[u8]>) -> Result<Output> {
        Signals::check()?;
        self.command
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if input.is_some() {
            self.command.stdin(Stdio::piped());
        }
        let mut child = self.command.spawn()?;
        Signals::track(child.id());
        if let Some(input) = input {
            let result = match child.stdin.take() {
                Some(mut pipe) => pipe.write_all(input),
                None => Err(std::io::Error::other("Missing command input")),
            };
            if let Err(error) = result {
                let _ = child.kill();
                let _ = child.wait();
                Signals::clear_child();
                Signals::check()?;
                return Err(error.into());
            }
        }
        let output = child.wait_with_output();
        Signals::clear_child();
        Signals::check()?;
        let output = output?;
        if !output.status.success() {
            return Err(Failure::status(output.status).into());
        }
        Ok(output)
    }
}

#[derive(Debug)]
pub struct Failure {
    code: i32,
    message: String,
}

impl Failure {
    fn status(status: ExitStatus) -> Self {
        let code = match status.code() {
            Some(code) => code,
            None => 128 + status.signal().unwrap_or(1),
        };
        Self {
            code,
            message: format!("Build command exited with {code}"),
        }
    }

    pub fn interrupted(signal: i32) -> Self {
        Self {
            code: 128 + signal,
            message: format!("Build cancelled by signal {signal}"),
        }
    }

    pub fn code(&self) -> i32 {
        self.code
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}
