use std::ffi::OsString;
use std::sync::Arc;

use bevy_ecs::prelude::*;

use crate::Result;
use crate::ci_scope::Scope;
use crate::consumer::Consumer;
use crate::environment::Environment;
use crate::native::Native;
use crate::repository::Checkout;
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
    fn retired(&self) -> bool {
        matches!(
            self,
            Self::Acquire
                | Self::Codesign
                | Self::Tool(_, _)
                | Self::Cargo(_)
                | Self::ClearReady
                | Self::SignDebug
                | Self::BundleDebug
                | Self::PublishDebug
                | Self::Run(_, _)
                | Self::LocalEntries
                | Self::LocalLaunch
                | Self::Stop
                | Self::WorkspaceCompile(_, _, _)
                | Self::WorkspaceAcquire
                | Self::WorkspaceInvoke(_)
        )
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
        lease: Option<Arc<TargetLock>>,
    ) -> Result<Outcome> {
        crate::signals::Signals::check()?;
        if self.retired() {
            return Err("Migrated DX work requires its concrete typed request".into());
        }
        let _lease = lease;
        let mut native = Native::new(checkout, environment.clone());
        match self {
            Self::Check | Self::Preflight | Self::Finish(_) | Self::Status | Self::Clean(_) => {
                return Err("Storage work requires its registered ECS systems".into());
            }
            Self::Seed => native.seed()?,
            Self::Native => native.ensure_native()?,
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
            Self::Package(_) => {
                return Err("Desktop packaging requires its registered ECS systems".into());
            }
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
            Self::Simulator => native.simulator()?,
            Self::Reset => return Err("Layout reset requires its registered ECS systems".into()),
            Self::Doctor => native.doctor()?,
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
            Self::Acquire
            | Self::Codesign
            | Self::Tool(_, _)
            | Self::Cargo(_)
            | Self::ClearReady
            | Self::SignDebug
            | Self::BundleDebug
            | Self::PublishDebug
            | Self::Run(_, _)
            | Self::LocalEntries
            | Self::LocalLaunch
            | Self::Stop
            | Self::WorkspaceCompile(_, _, _)
            | Self::WorkspaceAcquire
            | Self::WorkspaceInvoke(_)
            | Self::CliCompile
            | Self::CliPackage(_) => {
                return Err("CLI packaging requires its registered ECS systems".into());
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

impl Clone for Outcome {
    fn clone(&self) -> Self {
        match self {
            Self::LocalEntries(entries) => Self::LocalEntries(entries.clone()),
            Self::Executable(executable) => Self::Executable(executable.clone()),
            Self::Done => Self::Done,
            Self::Lease(lease) => Self::Lease(lease.clone()),
            Self::Identity(identity) => Self::Identity(identity.clone()),
            Self::Artifacts(paths) => Self::Artifacts(paths.clone()),
            Self::ToolPath(name, path) => Self::ToolPath(name.clone(), path.clone()),
        }
    }
}
