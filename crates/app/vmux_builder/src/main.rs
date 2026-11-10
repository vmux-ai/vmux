use std::ffi::OsString;
use std::path::PathBuf;

#[cfg(unix)]
use bootstrap::Bootstrap;
#[cfg(unix)]
use execution::Failure;
use policy::Policy;
#[cfg(unix)]
use runner::Runner;
#[cfg(unix)]
use signals::Signals;
use storage::Storage;

#[cfg(unix)]
mod bootstrap;
#[cfg(unix)]
mod debug;
mod environment;
#[cfg(unix)]
mod execution;
#[cfg(unix)]
mod mac_package;
mod policy;
mod processes;
mod profile;
#[cfg(unix)]
mod runner;
#[cfg(unix)]
mod signals;
mod storage;
#[cfg(unix)]
mod target_lock;
#[cfg(unix)]
mod temporary;

#[cfg(not(unix))]
compile_error!("vmux_builder requires a Unix host");

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    let result = Arguments::execute();
    if let Err(error) = result {
        let code = error.downcast_ref::<Failure>().map_or(1, Failure::code);
        eprintln!("Build storage: {error}");
        std::process::exit(code);
    } else if let Ok(code) = result {
        std::process::exit(code);
    }
}

struct Arguments {
    root: PathBuf,
    target: Option<PathBuf>,
    mode: String,
    profile: String,
    ensure: bool,
    command: Vec<OsString>,
}

impl Arguments {
    fn execute() -> Result<i32> {
        let args = Self::parse(std::env::args_os().skip(1))?;
        let _signals = Signals::install()?;
        if args.ensure {
            Bootstrap::new(args.root.clone())?.ensure()?;
        }
        args.run()
    }

    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self> {
        let mut root = None;
        let mut target = None;
        let mut mode = None;
        let mut profile = String::from("dev");
        let mut ensure = false;
        let mut command = Vec::new();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--root") => root = args.next().map(PathBuf::from),
                Some("--target") => target = args.next().map(PathBuf::from),
                Some("--ensure") => ensure = true,
                Some(value @ ("dev" | "local" | "release")) => {
                    profile = value.to_owned();
                    mode = Some(String::from("build"));
                    command.extend(args);
                    if command.first().is_some_and(|arg| arg == "--") {
                        command.remove(0);
                    }
                    break;
                }
                Some(value @ ("prepare-dev" | "prepare-local" | "prepare-release")) => {
                    mode = Some(value.to_owned());
                    command.extend(args);
                    break;
                }
                Some("--profile") => {
                    profile = args
                        .next()
                        .ok_or("Missing build profile")?
                        .into_string()
                        .map_err(|_| "Invalid build profile")?;
                    if !matches!(profile.as_str(), "dev" | "local" | "release") {
                        return Err("Unknown build profile".into());
                    }
                }
                Some(
                    value @ ("check" | "preflight" | "finish" | "status" | "clean" | "inherited"
                    | "open-local" | "open-release" | "--test" | "--print-bin"),
                ) => {
                    if mode.replace(value.to_owned()).is_some() {
                        return Err("Specify one storage mode".into());
                    }
                }
                _ => return Err(format!("Unknown storage argument: {arg:?}").into()),
            }
        }
        Ok(Self {
            root: root.ok_or("Missing --root")?,
            target,
            mode: mode.unwrap_or_else(|| "--print-bin".into()),
            profile,
            ensure,
            command,
        })
    }

    fn run(self) -> Result<i32> {
        if self.mode == "--print-bin" {
            println!("{}", std::env::current_exe()?.display());
            return Ok(0);
        }
        if self.mode == "--test" {
            Bootstrap::new(self.root)?.tests()?;
            return Ok(0);
        }
        let runner = Runner::new(self.root, self.target)?;
        if let Some(profile) = self.mode.strip_prefix("prepare-") {
            runner.prepare(profile, &self.command)?;
            return Ok(0);
        }
        if self.mode == "inherited" {
            return Ok(if runner.inherited() { 0 } else { 1 });
        }
        if self.mode == "build" {
            runner.build(&self.profile, &self.command)?;
            return Ok(0);
        }
        if self.mode == "open-local" || self.mode == "open-release" {
            runner.open(if self.mode == "open-local" {
                "local"
            } else {
                "release"
            })?;
            return Ok(0);
        }
        if self.mode == "clean" {
            runner.clean()?;
            return Ok(0);
        }
        let storage = Storage::new(
            runner.root().to_path_buf(),
            runner.target().to_path_buf(),
            Policy::environment()?,
        )?;
        match self.mode.as_str() {
            "check" => storage.check(),
            "preflight" => storage.preflight(),
            "finish" => storage.finish(&self.profile),
            "status" => storage.report(),
            _ => unreachable!(),
        }?;
        Ok(0)
    }
}
