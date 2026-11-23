use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde::Deserialize;

use super::VaultStorage;
use super::repository::{GitHubCli, GitRepository};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRepository {
    pub name: String,
    pub url: String,
    pub private: bool,
    pub empty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepositoryVisibility {
    Private,
    Public,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhRepository {
    name_with_owner: String,
    is_private: bool,
    url: String,
    is_empty: bool,
}

#[derive(Deserialize)]
struct GhAuthStatus {
    hosts: HashMap<String, Vec<GhAuthAccount>>,
}

#[derive(Deserialize)]
struct GhAuthAccount {
    login: String,
}

#[derive(Deserialize)]
struct GhViewerResponse {
    data: GhViewerData,
}

#[derive(Deserialize)]
struct GhViewerData {
    viewer: GhViewer,
}

#[derive(Deserialize)]
struct GhViewer {
    login: String,
    organizations: GhOrganizations,
}

#[derive(Deserialize)]
struct GhOrganizations {
    nodes: Vec<GhOrganization>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhOrganization {
    login: String,
    viewer_can_create_repositories: bool,
}

pub(super) struct GitHubAuthorization;

impl GitHubAuthorization {
    pub(super) fn run<F, C>(
        command: &mut Command,
        progress: &mut F,
        canceled: &C,
    ) -> Result<(), String>
    where
        F: FnMut(String),
        C: Fn() -> bool,
    {
        let mut child = command
            .env("BROWSER", "/usr/bin/true")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("failed to run gh: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "failed to read gh output".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "failed to read gh errors".to_string())?;
        let (sender, receiver) = mpsc::channel();
        let stdout_reader = Self::spawn_line_reader(stdout, sender.clone());
        let stderr_reader = Self::spawn_line_reader(stderr, sender);
        let mut lines = Vec::new();
        let mut reported_code = false;
        let status = loop {
            if canceled() {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("GitHub authorization canceled".to_string());
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    if !reported_code && let Some(code) = Self::device_code(&line) {
                        progress(code);
                        reported_code = true;
                    }
                    lines.push(line);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {}
            }
            match child
                .try_wait()
                .map_err(|error| format!("failed to wait for gh: {error}"))?
            {
                Some(status) => break status,
                None => continue,
            }
        };
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();
        for line in receiver.try_iter() {
            if !reported_code && let Some(code) = Self::device_code(&line) {
                progress(code);
                reported_code = true;
            }
            lines.push(line);
        }
        if status.success() {
            Ok(())
        } else {
            let message = lines
                .into_iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or_else(|| "GitHub authorization failed".to_string());
            Err(message)
        }
    }

    pub(super) fn device_code(line: &str) -> Option<String> {
        line.split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
            .find(|token| {
                let bytes = token.as_bytes();
                bytes.len() == 9
                    && bytes[4] == b'-'
                    && bytes.iter().enumerate().all(|(index, byte)| {
                        index == 4 || byte.is_ascii_uppercase() || byte.is_ascii_digit()
                    })
            })
            .map(str::to_string)
    }

    fn spawn_line_reader<R>(reader: R, sender: mpsc::Sender<String>) -> thread::JoinHandle<()>
    where
        R: Read + Send + 'static,
    {
        thread::spawn(move || {
            for line in BufReader::new(reader).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        })
    }
}

impl VaultStorage {
    pub fn connect_github_with_progress<F, C>(
        &self,
        mut progress: F,
        canceled: C,
    ) -> Result<String, String>
    where
        F: FnMut(String),
        C: Fn() -> bool,
    {
        let has_saved_account = GitHubAuthorization::has_saved_account()?;
        let mut command = GitHubCli::command()?;
        if has_saved_account {
            command.args([
                "auth",
                "refresh",
                "--hostname",
                "github.com",
                "--reset-scopes",
                "--clipboard",
            ]);
        } else {
            command.args([
                "auth",
                "login",
                "--hostname",
                "github.com",
                "--git-protocol",
                "https",
                "--web",
                "--clipboard",
                "--skip-ssh-key",
            ]);
        }
        GitHubAuthorization::run(&mut command, &mut progress, &canceled)?;
        if canceled() {
            return Err("GitHub authorization canceled".to_string());
        }
        let output = GitHubCli::command()?
            .args(["api", "user", "--jq", ".login"])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?;
        GitRepository::output_text(output)
    }
}

impl GitHubAuthorization {
    const VIEWER_QUERY: &'static str = "query { viewer { login organizations(first: 100) { nodes { login viewerCanCreateRepositories } } } }";

    pub(super) fn identity_and_repositories()
    -> Result<(String, Vec<String>, Vec<VaultRepository>), String> {
        if !Self::has_saved_account()? {
            return Ok((String::new(), Vec::new(), Vec::new()));
        }
        let mut command = GitHubCli::command()?;
        command
            .args(["api", "graphql", "-f"])
            .arg(format!("query={}", Self::VIEWER_QUERY));
        let output = command
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?;
        let source = GitRepository::output_text(output)?;
        let (owner, owners) = Self::owners_from_graphql(&source)?;
        let mut repositories = Vec::new();
        for repository_owner in &owners {
            let output = GitHubCli::command()?
                .args([
                    "repo",
                    "list",
                    repository_owner,
                    "--limit",
                    "100",
                    "--json",
                    "nameWithOwner,isPrivate,url,isEmpty",
                ])
                .output()
                .map_err(|error| format!("failed to run gh: {error}"))?;
            let Ok(source) = GitRepository::output_text(output) else {
                continue;
            };
            repositories.extend(
                serde_json::from_str::<Vec<GhRepository>>(&source)
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|repository| VaultRepository {
                        name: repository.name_with_owner,
                        url: repository.url,
                        private: repository.is_private,
                        empty: repository.is_empty,
                    }),
            );
        }
        repositories.sort_by(|left, right| {
            right
                .empty
                .cmp(&left.empty)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok((owner, owners, repositories))
    }

    pub(super) fn owners_from_graphql(source: &str) -> Result<(String, Vec<String>), String> {
        let viewer = serde_json::from_str::<GhViewerResponse>(source)
            .map_err(|error| error.to_string())?
            .data
            .viewer;
        let owner = viewer.login;
        let mut owners = vec![owner.clone()];
        owners.extend(
            viewer
                .organizations
                .nodes
                .into_iter()
                .filter(|organization| organization.viewer_can_create_repositories)
                .map(|organization| organization.login),
        );
        owners.sort();
        owners.dedup();
        if let Some(index) = owners.iter().position(|candidate| candidate == &owner) {
            owners.swap(0, index);
        }
        Ok((owner, owners))
    }

    fn has_saved_account() -> Result<bool, String> {
        let output = GitHubCli::command()?
            .args([
                "auth",
                "status",
                "--hostname",
                "github.com",
                "--json",
                "hosts",
            ])
            .output()
            .map_err(|error| format!("failed to run gh: {error}"))?;
        let source = String::from_utf8_lossy(&output.stdout);
        Self::saved_account(&source)
    }

    pub(super) fn saved_account(source: &str) -> Result<bool, String> {
        let status =
            serde_json::from_str::<GhAuthStatus>(source).map_err(|error| error.to_string())?;
        Ok(status
            .hosts
            .get("github.com")
            .is_some_and(|accounts| accounts.iter().any(|account| !account.login.is_empty())))
    }
}
