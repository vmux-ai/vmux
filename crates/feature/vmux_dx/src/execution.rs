use std::fmt;
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::Duration;

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
        let _group = Signals::track(child.id());
        let status = Self::wait(&mut child);
        Signals::check()?;
        let status = status?;
        if !status.success() {
            return Err(Failure::status(status).into());
        }
        Ok(())
    }

    pub fn output(&mut self, input: Option<&[u8]>) -> Result<Output> {
        let output = self.output_any(input)?;
        if !output.status.success() {
            std::io::stderr().write_all(&output.stderr)?;
            return Err(Failure::status(output.status).into());
        }
        Ok(output)
    }

    pub fn output_any(&mut self, input: Option<&[u8]>) -> Result<Output> {
        Signals::check()?;
        self.command
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if input.is_some() {
            self.command.stdin(Stdio::piped());
        }
        let mut child = self.command.spawn()?;
        let _group = Signals::track(child.id());
        let stdout = child.stdout.take().ok_or("Missing command output")?;
        let stderr = child.stderr.take().ok_or("Missing command error output")?;
        let out = thread::spawn(move || Self::capture(stdout));
        let err = thread::spawn(move || Self::capture(stderr));
        let input = if let Some(input) = input {
            let bytes = input.to_vec();
            let mut pipe = child.stdin.take().ok_or("Missing command input")?;
            Some(thread::spawn(move || pipe.write_all(&bytes)))
        } else {
            None
        };
        let status = Self::wait(&mut child);
        Signals::terminate_group(child.id());
        let stdout = out.join().map_err(|_| "Command output reader panicked")??;
        let stderr = err.join().map_err(|_| "Command error reader panicked")??;
        let input = if let Some(input) = input {
            input.join().map_err(|_| "Command input writer panicked")?
        } else {
            Ok(())
        };
        Signals::check()?;
        let status = status?;
        if status.success() {
            input?;
        }
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }

    fn wait(child: &mut Child) -> std::io::Result<ExitStatus> {
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if Signals::check().is_err() {
                Signals::terminate_group(child.id());
                return child.wait();
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn capture(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        let mut oversized = false;
        loop {
            let count = stream.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            if bytes.len() + count > 16 * 1024 * 1024 {
                oversized = true;
            }
            if !oversized {
                bytes.extend_from_slice(&buffer[..count]);
            }
        }
        if oversized {
            return Err(std::io::Error::other("Tool output exceeds 16 MiB"));
        }
        Ok(bytes)
    }
}

#[derive(Debug)]
pub struct Failure {
    code: i32,
    message: String,
}

impl Failure {
    pub fn context(mut self, context: &str) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }

    pub fn from_error(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        match error.downcast::<Self>() {
            Ok(error) => *error,
            Err(error) => Self::message(error.to_string()),
        }
    }

    pub fn message(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
        }
    }

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
