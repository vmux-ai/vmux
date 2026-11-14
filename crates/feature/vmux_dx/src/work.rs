use std::ffi::OsString;
use std::sync::Arc;

use bevy_ecs::prelude::*;

use crate::Result;
use crate::ci_scope::Scope;
use crate::consumer::Consumer;
use crate::environment::Environment;
use crate::native::Native;
use crate::processes::Processes;
use crate::repository::Checkout;
use crate::storage::Storage;
use crate::target_lock::TargetLock;

#[derive(Component, Clone)]
pub enum Work {
    Acquire,
    Check,
    Seed,
    Preflight,
    Finish(String),
    Native,
    Codesign,
    Helper,
    PackageTools,
    Dioxus(String),
    Binaryen,
    Tailwind,
    Tool(OsString, Vec<OsString>),
    Cargo(Vec<OsString>),
    ClearReady,
    SignDebug,
    BundleDebug,
    PublishDebug,
    Package(String),
    ResourcesIos,
    ReleaseIos,
    VerifyIos(bool),
    Css,
    FlattenWeb,
    Run(String, String),
    LocalEntries,
    LocalLaunch,
    Stop,
    Simulator,
    Reset,
    Doctor,
    Status,
    Clean(bool),
    Scope(Vec<OsString>),
    NativeInputs(Vec<OsString>),
    Consumer(Vec<OsString>),
    Internal(String, Vec<OsString>),
    CliCompile,
    CliPackage(bool),
    WorkspaceCompile(String, String, std::path::PathBuf),
    WorkspaceAcquire,
    WorkspaceInvoke(Vec<OsString>),
}

impl Work {
    pub(crate) fn cargo(args: &[&str]) -> Self {
        Self::Cargo(args.iter().map(OsString::from).collect())
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Acquire | Self::WorkspaceAcquire => "Acquire target lock",
            Self::Check | Self::Preflight => "Check build storage",
            Self::Seed => "Seed target cache",
            Self::Finish(_) => "Finish build storage",
            Self::Native => "Prepare native tools",
            Self::Codesign => "Prepare signing identity",
            Self::Helper => "Prepare CEF helper",
            Self::PackageTools => "Prepare packaging tools",
            Self::Dioxus(_) => "Prepare Dioxus tools",
            Self::Binaryen => "Prepare wasm optimizer",
            Self::Tailwind => "Prepare stylesheet compiler",
            Self::Tool(_, _) | Self::Cargo(_) => "Run tool",
            Self::ClearReady => "Invalidate old artifact",
            Self::SignDebug => "Sign debug binaries",
            Self::BundleDebug => "Bundle debug app",
            Self::PublishDebug => "Verify and publish debug artifact",
            Self::Package(_) => "Package app",
            Self::ResourcesIos => "Inject iOS resources",
            Self::ReleaseIos => "Sign and package iOS",
            Self::VerifyIos(_) => "Verify iOS bundle",
            Self::Css => "Build stylesheet",
            Self::FlattenWeb => "Publish web artifact",
            Self::Run(_, _) => "Run artifact",
            Self::LocalEntries => "Read completed local artifact entries",
            Self::LocalLaunch => "Launch selected local artifact",
            Self::Stop => "Stop development service",
            Self::Simulator => "Prepare simulator",
            Self::Reset => "Reset layout files",
            Self::Doctor => "Inspect tools",
            Self::Status => "Report build storage",
            Self::Clean(_) => "Clean build cache",
            Self::Scope(_) => "Select CI scope",
            Self::NativeInputs(_) => "Classify native CI inputs",
            Self::Consumer(_) => "Validate public SDK",
            Self::Internal(_, _) => "Run external integration",
            Self::CliCompile => "Build standalone release CLI",
            Self::CliPackage(_) => "Verify and package standalone release CLI",
            Self::WorkspaceCompile(_, _, _) => "Build workspace CLI provider",
            Self::WorkspaceInvoke(_) => "Run workspace CLI provider",
        }
    }

    pub fn execute(
        &self,
        checkout: &Checkout,
        environment: Environment,
        storage: &Storage,
        lease: Option<Arc<TargetLock>>,
    ) -> Result<Outcome> {
        crate::signals::Signals::check()?;
        let _lease = lease;
        let mut native = Native::new(checkout, environment.clone());
        match self {
            Self::WorkspaceAcquire => {
                storage.guard()?;
                let lease = Arc::new(TargetLock::acquire(&checkout.root, &checkout.target)?);
                storage.guard()?;
                return Ok(Outcome::Lease(lease));
            }
            Self::WorkspaceCompile(package, binary, cargo) => {
                return Ok(Outcome::Executable(
                    native.workspace_compile(package, binary, cargo)?,
                ));
            }
            Self::WorkspaceInvoke(arguments) => native.workspace_invoke(arguments)?,
            Self::Acquire => {
                let owner = std::env::var("VMUX_TARGET_LOCK_OWNER_PID")
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok());
                let inherited = std::env::var_os("VMUX_TARGET_LOCK_TARGET").as_deref()
                    == Some(checkout.target.as_os_str())
                    && std::env::var_os("VMUX_BUILD_STORAGE_ROOT").as_deref()
                        == Some(checkout.root.as_os_str())
                    && owner.is_some_and(|owner| {
                        owner != std::process::id() && crate::signals::Signals::alive(owner)
                    });
                if inherited {
                    return Ok(Outcome::Done);
                }
                return Ok(Outcome::Lease(Arc::new(TargetLock::acquire(
                    &checkout.root,
                    &checkout.target,
                )?)));
            }
            Self::Check => storage.check()?,
            Self::Seed => native.seed()?,
            Self::Preflight => storage.preflight()?,
            Self::Finish(profile) => {
                if let Err(error) = storage.finish(profile) {
                    crate::signals::Signals::check()?;
                    eprintln!("Build storage: cleanup skipped after successful build: {error}");
                }
            }
            Self::Native => native.ensure_native()?,
            Self::Codesign => return Ok(Outcome::Identity(native.identity()?)),
            Self::Helper => native.helper()?,
            Self::PackageTools => native.package_tools()?,
            Self::Dioxus(platform) => {
                native.dioxus(platform)?;
                return Ok(Outcome::ToolPath(
                    "DX_BIN".into(),
                    native.tool_path("DX_BIN", "dx"),
                ));
            }
            Self::Binaryen => native.binaryen()?,
            Self::Tailwind => {
                native.tailwind()?;
                return Ok(Outcome::ToolPath(
                    "TAILWINDCSS_BIN".into(),
                    native.tool_path("TAILWINDCSS_BIN", "tailwindcss"),
                ));
            }
            Self::Tool(program, args) => native.tool(program, args)?,
            Self::Cargo(args) => native.cargo(args)?,
            Self::ClearReady => native.clear_ready()?,
            Self::SignDebug => native.sign_debug()?,
            Self::BundleDebug => native.bundle_debug()?,
            Self::PublishDebug => {
                native.publish_debug()?;
                let name = if cfg!(target_os = "macos") {
                    "Vmux Dev.app"
                } else {
                    "vmux_desktop"
                };
                return Ok(Outcome::Artifacts(vec![
                    checkout.directory("debug").join(name),
                ]));
            }
            Self::Package(profile) => return Ok(Outcome::Artifacts(native.package(profile)?)),
            Self::ResourcesIos => native.resources_ios()?,
            Self::ReleaseIos => {
                native.ios_release()?;
                return Ok(Outcome::Artifacts(vec![
                    checkout
                        .target
                        .join("dx/vmux_mobile/release/ios/VmuxMobile.app"),
                ]));
            }
            Self::VerifyIos(signed) => native.verify_ios(*signed)?,
            Self::Css => native.css()?,
            Self::FlattenWeb => native.flatten_web()?,
            Self::Run(profile, platform) => native.run(profile, platform)?,
            Self::LocalEntries => return Ok(Outcome::LocalEntries(native.local_entries()?)),
            Self::LocalLaunch => native.launch_local(std::path::Path::new(
                environment.required("VMUX_DX_LOCAL_APP")?,
            ))?,
            Self::Stop => native.stop()?,
            Self::Simulator => native.simulator()?,
            Self::Reset => native.reset_layout()?,
            Self::Doctor => native.doctor()?,
            Self::Status => storage.report()?,
            Self::Clean(apply) => {
                let apply =
                    *apply || std::env::var("VMUX_BUILD_STORAGE_APPLY").as_deref() == Ok("1");
                storage.prune_caches(&Processes::capture()?, apply)?;
                storage.report()?;
            }
            Self::Scope(args) => Scope::new(checkout.root.clone())?.run(args)?,
            Self::NativeInputs(args) => {
                #[cfg(feature = "ci-syntax")]
                crate::ci_native::Changes::new(checkout.root.clone())?.run(args)?;
                #[cfg(not(feature = "ci-syntax"))]
                {
                    let _ = args;
                    return Err("ci-native requires the ci-syntax CLI feature".into());
                }
            }
            Self::Consumer(args) => Consumer::new(
                checkout.root.clone(),
                Some(checkout.target.clone()),
                environment,
            )?
            .run(args)?,
            Self::Internal(task, args) => native.internal(task, args)?,
            Self::CliCompile => native.cli_compile()?,
            Self::CliPackage(preview) => {
                return Ok(Outcome::Artifacts(native.cli_package(*preview)?));
            }
        }
        Ok(Outcome::Done)
    }
}

pub enum Outcome {
    LocalEntries(Vec<crate::local::Entry>),
    Executable(crate::workspace::Executable),
    Done,
    Lease(Arc<TargetLock>),
    Identity(String),
    Artifacts(Vec<std::path::PathBuf>),
    ToolPath(OsString, OsString),
}
