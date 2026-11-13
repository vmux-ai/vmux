use std::io;
use std::path::PathBuf;

use vmux_vm::{EngineClient, NativeVm, NetworkProxy, Settings};

fn main() -> std::process::ExitCode {
    let invocation = match Invocation::read() {
        Ok(invocation) => invocation,
        Err(error) => return Invocation::failed(error),
    };
    let settings: Settings = match invocation.settings() {
        Ok(settings) => settings,
        Err(error) => return Invocation::failed(error),
    };
    match invocation.run(settings) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => Invocation::failed(error),
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

    fn settings(&self) -> io::Result<Settings> {
        if matches!(self.command.as_str(), "network" | "native" | "run") {
            return Settings::load(&self.path);
        }
        let path = self.path.join("settings.json");
        if path.exists() {
            Settings::load(&path)
        } else {
            Ok(Settings::new(self.path.clone(), self.shared.clone()))
        }
    }

    fn failed(error: io::Error) -> std::process::ExitCode {
        eprintln!("vmux container engine: {error}");
        std::process::ExitCode::FAILURE
    }

    fn run(self, settings: Settings) -> io::Result<()> {
        if self.command == "network" {
            let port = self
                .shared
                .to_string_lossy()
                .parse::<u16>()
                .map_err(io::Error::other)?;
            return NetworkProxy::run(settings, port);
        }
        if self.command == "native" || self.command == "run" {
            if self.command == "native" {
                return NativeVm::run(settings);
            }
            return EngineClient::new(settings).foreground(&std::env::current_exe()?);
        }
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
