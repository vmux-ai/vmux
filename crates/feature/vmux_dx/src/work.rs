use std::ffi::OsString;
use std::sync::Arc;

use bevy_ecs::prelude::*;
use vmux_ecs::cli::CliInvocation;

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
    pub fn plan(invocation: &CliInvocation) -> Result<Vec<Self>> {
        let profile = invocation.value("profile").unwrap_or("dev");
        let platform = invocation.value("platform").unwrap_or("desktop");
        let mut forwarded = invocation
            .arguments
            .get("forward")
            .cloned()
            .unwrap_or_default();
        let mut steps = Vec::new();
        match invocation.command.as_str() {
            "dx.build" => {
                steps.extend([Self::Check, Self::Acquire, Self::Seed, Self::Preflight]);
                match platform {
                    "desktop" if profile == "dev" => {
                        steps.extend([
                            Self::Native,
                            Self::Tailwind,
                            Self::Codesign,
                            Self::Helper,
                            Self::ClearReady,
                        ]);
                        steps.push(Self::cargo(&["build", "-p", "vmux_cli"]));
                        steps.push(Self::cargo(&[
                            "build",
                            "-p",
                            "vmux_app",
                            "--bin",
                            "vmux_service",
                            "--features",
                            "service-daemon",
                        ]));
                        let mut args = if cfg!(target_os = "macos") {
                            vec!["rustc", "-p", "vmux_desktop", "--bin", "vmux_desktop"]
                        } else {
                            vec!["build", "-p", "vmux_desktop"]
                        }
                        .into_iter()
                        .map(OsString::from)
                        .collect::<Vec<_>>();
                        args.extend(["--no-default-features".into(), "--features".into()]);
                        args.push(
                            invocation
                                .value_os("features")
                                .unwrap_or(std::ffi::OsStr::new("dev,full"))
                                .to_owned(),
                        );
                        args.append(&mut forwarded);
                        if cfg!(target_os = "macos") {
                            args.extend(
                                ["--", "-C", "link-arg=-Wl,-headerpad,1000"].map(OsString::from),
                            );
                        }
                        steps.push(Self::Cargo(args));
                        if cfg!(target_os = "macos") {
                            steps.extend([Self::SignDebug, Self::BundleDebug]);
                        }
                        steps.push(Self::PublishDebug);
                    }
                    "desktop" => {
                        steps.extend([
                            Self::Native,
                            Self::Tailwind,
                            Self::PackageTools,
                            Self::Codesign,
                            Self::Package(profile.into()),
                        ]);
                    }
                    "ios" if profile == "release" => {
                        steps.extend([Self::Dioxus("ios".into()), Self::ReleaseIos]);
                    }
                    "ios" | "android" => {
                        steps.push(Self::Dioxus(platform.into()));
                        let mut args = vec![
                            "build".into(),
                            format!("--{platform}").into(),
                            "-p".into(),
                            "vmux_mobile".into(),
                        ];
                        if profile != "dev" {
                            args.push("--release".into());
                        }
                        args.append(&mut forwarded);
                        steps.push(Self::Tool("dx".into(), args));
                        if platform == "ios" {
                            steps.extend([Self::ResourcesIos, Self::VerifyIos(false)]);
                        }
                    }
                    "web" => {
                        steps.extend([
                            Self::Dioxus(platform.into()),
                            Self::Binaryen,
                            Self::Tailwind,
                            Self::Css,
                        ]);
                        let mut args = vec!["build".into(), "--platform".into(), "web".into()];
                        if profile != "dev" {
                            args.extend(["--ssg".into(), "--release".into()]);
                        }
                        args.append(&mut forwarded);
                        steps.push(Self::Tool("dx".into(), args));
                        if profile != "dev" {
                            steps.push(Self::FlattenWeb);
                        }
                    }
                    _ => return Err("Unsupported build platform".into()),
                }
                steps.push(Self::Finish(profile.into()));
            }
            "dx.run" => {
                steps.push(Self::Acquire);
                if platform != "desktop" {
                    steps.push(Self::Dioxus(platform.into()));
                    if platform == "web" {
                        steps.extend([Self::Binaryen, Self::Tailwind, Self::Css]);
                    }
                    if platform == "ios" {
                        steps.push(Self::Simulator);
                    }
                }
                steps.push(Self::Run(profile.into(), platform.into()));
            }
            "dx.stop" => steps.push(Self::Stop),
            "dx.reset-layout" => steps.push(Self::Reset),
            "dx.test" => {
                steps.extend([Self::Acquire, Self::Seed]);
                let mut args = vec![
                    "test".into(),
                    "--workspace".into(),
                    "--exclude".into(),
                    "bevy_cef_core".into(),
                ];
                if profile != "dev" {
                    args.push("--release".into());
                }
                if let Some(features) = invocation.value_os("features") {
                    args.extend(["--features".into(), features.to_owned()]);
                }
                args.append(&mut forwarded);
                steps.push(Self::Cargo(args));
            }
            "dx.lint" => {
                steps.extend([Self::Acquire, Self::Seed]);
                steps.push(Self::cargo(&["fmt", "--all", "--", "--check"]));
                let mut args = vec![
                    "clippy".into(),
                    "--workspace".into(),
                    "--all-targets".into(),
                ];
                if profile != "dev" {
                    args.push("--release".into());
                }
                if let Some(features) = invocation.value_os("features") {
                    args.extend(["--features".into(), features.to_owned()]);
                }
                args.append(&mut forwarded);
                args.extend(["--".into(), "-D".into(), "warnings".into()]);
                steps.push(Self::Cargo(args));
            }
            "dx.doctor" => steps.push(Self::Doctor),
            "dx.setup" => {
                if platform == "desktop" {
                    steps.extend([
                        Self::Check,
                        Self::Acquire,
                        Self::Native,
                        Self::Tailwind,
                        Self::PackageTools,
                        Self::Codesign,
                        Self::Helper,
                    ]);
                } else {
                    steps.push(Self::Dioxus(platform.into()));
                    if platform == "web" {
                        steps.extend([Self::Binaryen, Self::Tailwind]);
                    }
                }
            }
            "dx.cache.status" => steps.push(Self::Status),
            "dx.cache.clean" => {
                steps.extend([Self::Acquire, Self::Clean(invocation.flag("apply"))])
            }
            "dx.ci-scope" => steps.push(Self::Scope(forwarded)),
            "dx.ci-native" => steps.push(Self::NativeInputs(forwarded)),
            "dx.framework-consumer" => steps.extend([Self::Acquire, Self::Consumer(forwarded)]),
            "dx.internal" => {
                let task = invocation.value("task").ok_or("Missing integration task")?;
                if task == "cli-release" {
                    let preview = match forwarded.as_slice() {
                        [] => false,
                        [value] if value == "preview" => true,
                        _ => {
                            return Err(
                                "cli-release accepts only an optional preview channel".into()
                            );
                        }
                    };
                    steps.extend([
                        Self::Check,
                        Self::Acquire,
                        Self::Preflight,
                        Self::CliCompile,
                        Self::CliPackage(preview),
                        Self::Finish("release".into()),
                    ]);
                    return Ok(steps);
                }
                if task == "cargo" {
                    steps.extend([Self::Acquire, Self::Seed]);
                }
                steps.push(Self::Internal(task.into(), forwarded));
            }
            _ => return Err("Unknown development command".into()),
        }
        Ok(steps)
    }

    fn cargo(args: &[&str]) -> Self {
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
    Executable(crate::workspace::Executable),
    Done,
    Lease(Arc<TargetLock>),
    Identity(String),
    Artifacts(Vec<std::path::PathBuf>),
    ToolPath(OsString, OsString),
}
