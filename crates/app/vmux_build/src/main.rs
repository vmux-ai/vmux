use std::ffi::OsString;
use std::path::PathBuf;

mod policy;
mod processes;
mod profile;
mod storage;

use policy::Policy;
use processes::Processes;
use storage::Storage;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = Arguments::parse(std::env::args_os().skip(1)).and_then(Arguments::run) {
        eprintln!("Build storage: {error}");
        std::process::exit(1);
    }
}

struct Arguments {
    root: PathBuf,
    target: PathBuf,
    mode: String,
    profile: String,
}

impl Arguments {
    fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Self> {
        let mut root = None;
        let mut target = None;
        let mut mode = None;
        let mut profile = String::from("dev");
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--root") => root = args.next().map(PathBuf::from),
                Some("--target") => target = args.next().map(PathBuf::from),
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
                Some(value @ ("check" | "preflight" | "finish" | "status" | "clean")) => {
                    if mode.replace(value.to_owned()).is_some() {
                        return Err("Specify one storage mode".into());
                    }
                }
                _ => return Err(format!("Unknown storage argument: {arg:?}").into()),
            }
        }
        Ok(Self {
            root: root.ok_or("Missing --root")?,
            target: target.ok_or("Missing --target")?,
            mode: mode.ok_or("Missing storage mode")?,
            profile,
        })
    }

    fn run(self) -> Result<()> {
        let storage = Storage::new(self.root, self.target, Policy::environment()?)?;
        match self.mode.as_str() {
            "check" => storage.check(),
            "preflight" => storage.preflight(),
            "finish" => storage.finish(&self.profile),
            "status" => storage.report(),
            "clean" => {
                let apply = std::env::var("VMUX_BUILD_STORAGE_APPLY").as_deref() == Ok("1");
                storage.prune_caches(&Processes::capture()?, apply)?;
                storage.report()
            }
            _ => unreachable!(),
        }
    }
}
