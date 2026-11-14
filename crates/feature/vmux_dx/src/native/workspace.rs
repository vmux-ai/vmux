use std::ffi::OsString;

use crate::Result;
use crate::execution::Execution;
use crate::native::Native;
use crate::workspace::Executable;

impl Native {
    pub fn workspace_compile(
        &self,
        package: &str,
        binary: &str,
        cargo: &std::path::Path,
    ) -> Result<Executable> {
        let mut command = self.command("cargo");
        command
            .env_remove("CARGO_BUILD_TARGET")
            .args(["build", "--locked", "--manifest-path"])
            .arg(cargo)
            .args(["-p", package, "--bin", binary, "--target"])
            .arg(env!("VMUX_DX_HOST_TARGET"))
            .arg("--target-dir")
            .arg(&self.checkout.target);
        Execution::new(command).run()?;
        Executable::snapshot(&self.checkout.directory("debug").join(binary))
    }

    pub fn workspace_invoke(&self, arguments: &[OsString]) -> Result<()> {
        let mut command = self.command(self.environment.required("VMUX_DX_PROVIDER_EXECUTABLE")?);
        command.env("VMUX_DX_PROVIDER_CHILD", "1").args(arguments);
        Execution::new(command).run()
    }
}
