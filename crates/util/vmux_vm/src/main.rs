use std::io;
use std::path::PathBuf;

use vmux_vm::{EngineClient, NativeVm, Settings};

fn main() -> std::process::ExitCode {
    match Invocation::read().and_then(Invocation::run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vmux container engine: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

struct Invocation {
    command: String,
    path: PathBuf,
    shared: PathBuf,
}

impl Invocation {
    fn read() -> io::Result<Self> {
        let mut arguments = std::env::args_os().skip(1);
        let command = arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .ok_or_else(|| {
                io::Error::other("usage: vmux_vm prepare|start|status|stop DATA_DIR [SHARED_DIR]")
            })?;
        let path = arguments
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("missing VM data path"))?;
        let shared = match arguments.next() {
            Some(path) => PathBuf::from(path),
            None => std::env::current_dir()?,
        };
        if arguments.next().is_some() {
            return Err(io::Error::other("unexpected extra argument"));
        }
        Ok(Self {
            command,
            path,
            shared,
        })
    }

    fn run(self) -> io::Result<()> {
        if self.command == "native" || self.command == "run" {
            let settings = Settings::load(&self.path)?;
            if self.command == "native" {
                return NativeVm::run(settings);
            }
            return EngineClient::new(settings).foreground(&std::env::current_exe()?);
        }
        let settings_path = self.path.join("settings.json");
        let settings = if settings_path.exists() {
            Settings::load(&settings_path)?
        } else {
            Settings::new(self.path, self.shared)
        };
        let mut engine = EngineClient::new(settings);
        match self.command.as_str() {
            "prepare" => engine.prepare(),
            "start" => {
                let status = engine.start(&std::env::current_exe()?)?;
                println!("Docker ready: unix://{}", status.socket.display());
                Ok(())
            }
            "stop" => engine.stop(),
            "status" => {
                let status = engine.status();
                println!(
                    "running={} docker_ready={} socket={} log={}",
                    status.running,
                    status.docker_ready,
                    status.socket.display(),
                    status.log.display()
                );
                Ok(())
            }
            _ => Err(io::Error::other("unknown VM command")),
        }
    }
}
