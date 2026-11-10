use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;
use vmux_auth::CredentialLease;
use vmux_ecs::BlockingCancellation;

const CLONE_TIMEOUT: Duration = Duration::from_secs(600);
const CHECKOUT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TOKEN_BYTES: usize = 1024;
const TOKEN_READ_LIMIT: usize = 4;

pub struct GitCredential;

impl GitCredential {
    pub fn clone(
        owner: &str,
        repository: &str,
        destination: &Path,
        credentials: &CredentialLease,
        cancellation: &BlockingCancellation,
    ) -> Result<(), String> {
        let repository = Self::repository_url(owner, repository)?;
        Self::validate_repository_url(&repository)?;
        let token = credentials.token()?;
        Self::validate_token(token)?;
        if destination.exists() {
            return Err("Git clone destination already exists".to_string());
        }
        let parent = destination
            .parent()
            .ok_or_else(|| "Git clone destination has no parent".to_string())?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let staging = tempfile::Builder::new()
            .prefix(".vmux-github-clone-")
            .tempdir_in(parent)
            .map_err(|error| error.to_string())?;
        let staging_repository = staging.path().join("repository");

        let isolation = tempfile::tempdir().map_err(|error| error.to_string())?;
        let askpass = isolation.path().join("askpass");
        let credential_pipe = isolation.path().join("credential");
        let templates = isolation.path().join("templates");
        std::fs::create_dir(&templates).map_err(|error| error.to_string())?;
        Self::write_askpass(&askpass)?;
        let mut credential_channel = Self::credential_channel(&credential_pipe)?;
        Self::write_credential(&mut credential_channel, token)?;

        let mut clone = Self::clone_command(
            repository.as_str(),
            &staging_repository,
            isolation.path(),
            &templates,
            &askpass,
            &credential_pipe,
        );
        Self::run(
            &mut clone,
            CLONE_TIMEOUT,
            "git fetch",
            cancellation,
            credentials,
        )?;
        drop(credential_channel);

        let mut checkout = Self::checkout_command(&staging_repository, isolation.path());
        Self::run(
            &mut checkout,
            CHECKOUT_TIMEOUT,
            "git checkout",
            cancellation,
            credentials,
        )?;
        if cancellation.is_canceled() {
            return Err("git clone canceled".to_string());
        }
        std::fs::rename(&staging_repository, destination).map_err(|error| error.to_string())
    }

    fn clone_command(
        repository: &str,
        destination: &Path,
        isolation: &Path,
        templates: &Path,
        askpass: &Path,
        credential_pipe: &Path,
    ) -> Command {
        let mut command = Self::isolated_command(isolation);
        command
            .arg("-c")
            .arg("credential.helper=")
            .arg("-c")
            .arg("credential.useHttpPath=true")
            .arg("-c")
            .arg("core.hooksPath=/dev/null")
            .arg("-c")
            .arg("protocol.allow=never")
            .arg("-c")
            .arg("protocol.https.allow=always")
            .arg("-c")
            .arg("http.followRedirects=false")
            .arg("clone")
            .arg("--no-checkout")
            .arg("--no-recurse-submodules")
            .arg("--origin=origin")
            .arg("--config=core.hooksPath=/dev/null")
            .arg("--config=credential.helper=")
            .arg("--config=protocol.allow=never")
            .arg("--config=protocol.https.allow=always")
            .arg(format!("--template={}", templates.display()))
            .arg("--")
            .arg(repository)
            .arg(destination)
            .env("GIT_ASKPASS", askpass)
            .env("GIT_ASKPASS_REQUIRE", "force")
            .env("VMUX_GITHUB_CREDENTIAL_PIPE", credential_pipe)
            .stdin(Stdio::null());
        command
    }

    fn checkout_command(destination: &Path, isolation: &Path) -> Command {
        let mut command = Self::isolated_command(isolation);
        command
            .arg("-c")
            .arg("credential.helper=")
            .arg("-c")
            .arg("core.hooksPath=/dev/null")
            .arg("-c")
            .arg("protocol.allow=never")
            .arg("-c")
            .arg("protocol.https.allow=always")
            .arg("-C")
            .arg(destination)
            .arg("checkout")
            .arg("--force")
            .stdin(Stdio::null());
        command
    }

    fn isolated_command(isolation: &Path) -> Command {
        let mut command = vmux_git::GitProcess::command();
        command
            .env("HOME", isolation)
            .env("XDG_CONFIG_HOME", isolation.join("xdg"));
        vmux_git::GitProcess::prepare(&mut command);
        command
    }

    fn run(
        command: &mut Command,
        timeout: Duration,
        operation: &str,
        cancellation: &BlockingCancellation,
        credentials: &CredentialLease,
    ) -> Result<(), String> {
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let status =
            vmux_git::GitProcess::wait(&mut child, timeout, operation, cancellation, || {
                if credentials.is_active() {
                    return Ok(());
                }
                Err(format!(
                    "{operation} canceled because the connection disconnected"
                ))
            })?;
        Self::success(status, operation)
    }

    fn success(status: ExitStatus, operation: &str) -> Result<(), String> {
        if status.success() {
            return Ok(());
        }
        Err(format!("{operation} exited with {status}"))
    }

    fn write_askpass(path: &Path) -> Result<(), String> {
        let mut file = Self::file(path, 0o700)?;
        file.write_all(
            b"#!/bin/sh\nprompt=${1-}\ncase \"$prompt\" in\n\"Username for 'https://github.com':\"*|\"Username for 'https://github.com/\"*\"':\"*) printf '%s\\n' x-access-token;;\n\"Password for 'https://x-access-token@github.com':\"*|\"Password for 'https://x-access-token@github.com/\"*\"':\"*) [ -n \"${VMUX_GITHUB_CREDENTIAL_PIPE-}\" ] || exit 1; IFS= read -r token < \"$VMUX_GITHUB_CREDENTIAL_PIPE\" || exit 1; [ -n \"$token\" ] || exit 1; printf '%s\\n' \"$token\";;\n*) exit 1;;\nesac\n",
        )
        .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())
    }

    #[cfg(unix)]
    fn credential_channel(path: &Path) -> Result<std::fs::File, String> {
        let status = Command::new(Self::mkfifo_program())
            .arg("-m")
            .arg("600")
            .arg(path)
            .status()
            .map_err(|error| error.to_string())?;
        if !status.success() {
            return Err(format!("mkfifo exited with {status}"));
        }
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| error.to_string())
    }

    #[cfg(not(unix))]
    fn credential_channel(_path: &Path) -> Result<std::fs::File, String> {
        Err("Authenticated GitHub clone is unsupported on this platform".to_string())
    }

    fn write_credential(channel: &mut std::fs::File, token: &str) -> Result<(), String> {
        for _ in 0..TOKEN_READ_LIMIT {
            channel
                .write_all(token.as_bytes())
                .and_then(|_| channel.write_all(b"\n"))
                .map_err(|error| format!("Git credential channel failed: {error}"))?;
        }
        channel
            .flush()
            .map_err(|error| format!("Git credential channel failed: {error}"))
    }

    fn mkfifo_program() -> PathBuf {
        for path in ["/usr/bin/mkfifo", "/bin/mkfifo"] {
            let program = PathBuf::from(path);
            if program.is_file() {
                return program;
            }
        }
        PathBuf::from("/usr/bin/mkfifo")
    }

    fn validate_token(token: &str) -> Result<(), String> {
        if token.is_empty() || token.len() > MAX_TOKEN_BYTES || token.contains(['\r', '\n', '\0']) {
            return Err("GitHub credential is invalid".to_string());
        }
        Ok(())
    }

    fn repository_url(owner: &str, repository: &str) -> Result<url::Url, String> {
        if !Self::valid_owner(owner) || !Self::valid_repository(repository) {
            return Err("GitHub repository coordinates are invalid".to_string());
        }
        let mut url = url::Url::parse("https://github.com/").map_err(|error| error.to_string())?;
        url.path_segments_mut()
            .map_err(|_| "GitHub repository URL is invalid".to_string())?
            .push(owner)
            .push(&format!("{repository}.git"));
        Ok(url)
    }

    fn valid_owner(owner: &str) -> bool {
        if owner.is_empty() || owner.len() > 39 {
            return false;
        }
        let bytes = owner.as_bytes();
        if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
            return false;
        }
        owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }

    fn valid_repository(repository: &str) -> bool {
        if repository.is_empty()
            || repository.len() > 100
            || repository == "."
            || repository == ".."
        {
            return false;
        }
        repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    }

    fn validate_repository_url(repository: &url::Url) -> Result<(), String> {
        if repository.scheme() != "https"
            || repository.host_str() != Some("github.com")
            || !repository.username().is_empty()
            || repository.password().is_some()
            || repository.port().is_some()
            || repository.query().is_some()
            || repository.fragment().is_some()
        {
            return Err("GitHub repository URL is invalid".to_string());
        }
        Ok(())
    }

    #[cfg(unix)]
    fn file(path: &Path, mode: u32) -> Result<std::fs::File, String> {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true).mode(mode);
        options.open(path).map_err(|error| error.to_string())
    }

    #[cfg(not(unix))]
    fn file(path: &Path, _mode: u32) -> Result<std::fs::File, String> {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        options.open(path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_url_is_derived_from_validated_coordinates() {
        assert_eq!(
            GitCredential::repository_url("vmux-ai", "vmux")
                .unwrap()
                .as_str(),
            "https://github.com/vmux-ai/vmux.git"
        );
        assert!(GitCredential::repository_url("example.com/x", "vmux").is_err());
        assert!(GitCredential::repository_url("vmux-ai", "vmux@github.com").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn askpass_serves_credentials_only_for_github_https_prompts() {
        let directory = tempfile::tempdir().unwrap();
        let askpass = directory.path().join("askpass");
        GitCredential::write_askpass(&askpass).unwrap();

        let username = Command::new(&askpass)
            .arg("Username for 'https://github.com/vmux-ai/vmux.git': ")
            .output()
            .unwrap();
        assert!(username.status.success());
        assert_eq!(username.stdout, b"x-access-token\n");

        let credential_pipe = directory.path().join("credential");
        let mut credential_channel = GitCredential::credential_channel(&credential_pipe).unwrap();
        GitCredential::write_credential(&mut credential_channel, "secret").unwrap();
        let password = Command::new(&askpass)
            .arg("Password for 'https://x-access-token@github.com/vmux-ai/vmux.git': ")
            .env("VMUX_GITHUB_CREDENTIAL_PIPE", &credential_pipe)
            .output()
            .unwrap();
        assert!(password.status.success());
        assert_eq!(password.stdout, b"secret\n");

        let rejected = Command::new(&askpass)
            .arg("Password for 'https://x-access-token@github.com.evil.example/vmux': ")
            .env("VMUX_GITHUB_CREDENTIAL_PIPE", &credential_pipe)
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(rejected.stdout.is_empty());
    }
}
