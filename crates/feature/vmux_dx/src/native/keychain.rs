use std::ffi::OsString;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::Result;
use crate::environment::Environment;
use crate::execution::Execution;
use crate::temporary::Temporary;

pub(super) struct Keychain {
    original: Vec<OsString>,
    path: Option<PathBuf>,
    temporary: Option<Temporary>,
    root: PathBuf,
    environment: Environment,
}

impl Keychain {
    pub(super) fn capture(root: &Path, environment: &Environment) -> Result<Self> {
        let mut command = environment.command("security", root);
        command.args(["list-keychains", "-d", "user"]);
        let output = String::from_utf8(Execution::new(command).output(None)?.stdout)?;
        let mut original = Vec::new();
        for line in output.lines() {
            let path = line.trim().trim_matches('"');
            if !path.is_empty() {
                original.push(path.into());
            }
        }
        Ok(Self {
            original,
            path: None,
            temporary: None,
            root: root.to_path_buf(),
            environment: environment.clone(),
        })
    }

    pub(super) fn install(&mut self, root: &Path, environment: &Environment) -> Result<PathBuf> {
        let certificate = environment.required("APPLE_CERTIFICATE")?.to_os_string();
        let import_password = environment
            .required("APPLE_CERTIFICATE_PASSWORD")?
            .to_os_string();
        let identity = environment
            .required("APPLE_SIGNING_IDENTITY")?
            .to_os_string();
        self.temporary = Some(Temporary::create(&std::env::temp_dir(), "vmux-build")?);
        let directory = self
            .temporary
            .as_ref()
            .ok_or("Missing signing directory")?
            .path();
        let path = directory.join("signing.keychain-db");
        let certificate_path = directory.join("cert.p12");
        self.path = Some(path.clone());
        let command = environment.command("base64", root);
        let mut decode = Execution::new({
            let mut command = command;
            command.arg("--decode");
            command
        });
        let certificate = decode.output(Some(certificate.as_encoded_bytes()))?.stdout;
        fs::write(&certificate_path, certificate)?;
        fs::set_permissions(&certificate_path, Permissions::from_mode(0o600))?;
        let command = environment.command("uuidgen", root);
        let password = String::from_utf8(Execution::new(command).output(None)?.stdout)?;
        let password = password.trim();
        for args in [
            vec![
                OsString::from("create-keychain"),
                "-p".into(),
                password.into(),
                path.as_os_str().to_owned(),
            ],
            vec![
                "set-keychain-settings".into(),
                "-lut".into(),
                "21600".into(),
                path.as_os_str().to_owned(),
            ],
            vec![
                "unlock-keychain".into(),
                "-p".into(),
                password.into(),
                path.as_os_str().to_owned(),
            ],
            vec![
                "import".into(),
                certificate_path.into_os_string(),
                "-P".into(),
                import_password,
                "-A".into(),
                "-f".into(),
                "pkcs12".into(),
                "-k".into(),
                path.as_os_str().to_owned(),
            ],
            vec![
                "set-key-partition-list".into(),
                "-S".into(),
                "apple-tool:,apple:".into(),
                "-k".into(),
                password.into(),
                path.as_os_str().to_owned(),
            ],
        ] {
            let mut command = environment.command("security", root);
            command.args(args);
            Execution::new(command).run()?;
        }
        let mut activate = environment.command("security", root);
        activate
            .args(["list-keychains", "-d", "user", "-s"])
            .arg(&path)
            .args(&self.original);
        Execution::new(activate).run()?;
        let mut find = environment.command("security", root);
        find.args(["find-identity", "-v", "-p", "codesigning"])
            .arg(&path);
        let output = String::from_utf8(Execution::new(find).output(None)?.stdout)?;
        if !output.contains(&format!("\"{}\"", identity.to_string_lossy())) {
            return Err("APPLE_SIGNING_IDENTITY does not match the imported identity".into());
        }
        Ok(path)
    }
}

impl Drop for Keychain {
    fn drop(&mut self) {
        if !self.original.is_empty() || self.path.is_some() {
            let _ = self
                .environment
                .command("security", &self.root)
                .args(["list-keychains", "-d", "user", "-s"])
                .args(&self.original)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        if let Some(path) = &self.path {
            let _ = self
                .environment
                .command("security", &self.root)
                .arg("delete-keychain")
                .arg(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}
