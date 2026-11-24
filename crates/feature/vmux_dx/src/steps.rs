use std::ffi::OsString;
use std::path::PathBuf;

use bevy_ecs::prelude::*;

use crate::Step;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct StorageCheckStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct StoragePreflightStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct StorageFinishStep(pub String);

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct StorageStatusStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct StorageCleanStep(pub bool);

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct DesktopPackageStep(pub String);

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct CliCompileStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct CliPackageStep(pub bool);

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct ResetLayoutStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct SeedStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct NativeToolsStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct HelperStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct PackageToolsStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct DioxusStep(pub String);

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct BinaryenStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct TailwindStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct ResourcesIosStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct ReleaseIosStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct VerifyIosStep(pub bool);

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct CssStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct FlattenWebStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct SimulatorStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct DoctorStep;

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct ScopeStep(pub Vec<OsString>);

impl ScopeStep {
    pub(crate) fn run(&self, checkout: &crate::repository::Checkout) -> crate::Result<()> {
        crate::ci_scope::Scope::new(checkout.root.clone())?.run(&self.0)
    }
}

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct NativeInputsStep(pub Vec<OsString>);

impl NativeInputsStep {
    pub(crate) fn run(&self, checkout: &crate::repository::Checkout) -> crate::Result<()> {
        #[cfg(feature = "ci-syntax")]
        {
            crate::ci_native::Changes::new(checkout.root.clone())?.run(&self.0)
        }
        #[cfg(not(feature = "ci-syntax"))]
        {
            let _ = (checkout, &self.0);
            Err("ci-native requires the ci-syntax CLI feature".into())
        }
    }
}

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct ConsumerStep(pub Vec<OsString>);

impl ConsumerStep {
    pub(crate) fn run(
        &self,
        checkout: &crate::repository::Checkout,
        environment: crate::environment::Environment,
    ) -> crate::Result<()> {
        crate::consumer::Consumer::new(
            checkout.root.clone(),
            Some(checkout.target.clone()),
            environment,
        )?
        .run(&self.0)
    }
}

#[derive(Component, Clone)]
#[require(Step)]
pub(crate) struct InternalStep {
    pub task: String,
    pub arguments: Vec<OsString>,
}

#[derive(Component, Clone)]
pub(crate) struct ResolvedIdentity(pub String);

#[derive(Component, Clone)]
pub(crate) struct ProducedArtifacts(pub Vec<PathBuf>);

#[derive(Component, Clone)]
#[allow(unknown_lints, vmux_ecs_ownership)]
pub(crate) struct ResolvedExecutable(pub crate::workspace::Executable);

#[derive(Component, Clone)]
pub(crate) struct ConfiguredToolPath {
    pub key: &'static str,
    pub path: OsString,
}

#[derive(Component, Clone)]
pub(crate) struct MissingArtifact;
