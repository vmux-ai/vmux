use bevy::prelude::*;
use bevy_reflect::{Reflect, std_traits::ReflectDefault};
use serde::{Deserialize, Serialize};
use vmux_api::Id;
use vmux_auth::Connection;

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Default)]
#[type_path = "vmux_github"]
pub struct GithubRepository {
    pub id: u64,
    pub node_id: String,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub clone_url: String,
    pub owner_id: u64,
    pub owner_login: String,
    pub owner_node_id: String,
    pub default_branch: String,
    pub private: bool,
    pub updated_at: String,
}

impl GithubRepository {
    pub fn scope_id(&self) -> Option<Id<vmux_connector::ExternalScope>> {
        github_scope_id(self.owner_id)
    }

    pub fn external_id(&self) -> String {
        if self.node_id.is_empty() {
            self.id.to_string()
        } else {
            self.node_id.clone()
        }
    }
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Default)]
#[type_path = "vmux_github"]
pub struct GithubIssue {
    pub id: u64,
    pub node_id: String,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: IssueState,
    pub html_url: String,
    pub repository_url: String,
    pub repository: String,
    pub owner: String,
    pub owner_id: u64,
    pub updated_at: String,
}

impl GithubIssue {
    pub fn scope_id(&self) -> Option<Id<vmux_connector::ExternalScope>> {
        github_scope_id(self.owner_id)
    }

    pub fn external_id(&self) -> String {
        if self.node_id.is_empty() {
            self.id.to_string()
        } else {
            self.node_id.clone()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[type_path = "vmux_github"]
#[serde(rename_all = "snake_case")]
pub enum IssueState {
    #[default]
    Open,
    Closed,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Default)]
#[type_path = "vmux_github"]
pub struct GithubPullRequest {
    pub id: u64,
    pub node_id: String,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: PullRequestState,
    pub html_url: String,
    pub owner: String,
    pub repository: String,
    pub owner_id: u64,
    pub head: String,
    pub head_owner: String,
    pub head_repository: String,
    pub head_sha: String,
    pub base: String,
    pub base_sha: String,
    pub draft: bool,
    pub merged: bool,
    pub updated_at: String,
}

impl GithubPullRequest {
    pub fn scope_id(&self) -> Option<Id<vmux_connector::ExternalScope>> {
        github_scope_id(self.owner_id)
    }

    pub fn external_id(&self) -> String {
        if self.node_id.is_empty() {
            self.id.to_string()
        } else {
            self.node_id.clone()
        }
    }
}

fn github_scope_id(owner_id: u64) -> Option<Id<vmux_connector::ExternalScope>> {
    (owner_id > 0).then(|| Id::new(format!("github.com/accounts/{owner_id}")))
}

pub(crate) fn github_scope_owner_id(scope: &Id<vmux_connector::ExternalScope>) -> Option<u64> {
    scope
        .as_str()
        .strip_prefix("github.com/accounts/")?
        .parse()
        .ok()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[type_path = "vmux_github"]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    #[default]
    Open,
    Closed,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Reflect, Serialize, Deserialize)]
#[reflect(Component, Default)]
#[type_path = "vmux_github"]
pub struct GithubCoordinates {
    pub owner: String,
    pub repository: String,
    pub number: Option<u64>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct RepositorySearchResult {
    pub requester: Entity,
    pub generation: u64,
    pub connection: Id<Connection>,
    pub result: Result<Vec<GithubRepository>, String>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct IssueSearchResult {
    pub requester: Entity,
    pub generation: u64,
    pub connection: Id<Connection>,
    pub result: Result<Vec<GithubIssue>, String>,
}

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct RepositoryCloneResult {
    pub requester: Entity,
    pub generation: u64,
    pub connection: Id<Connection>,
    pub result: Result<std::path::PathBuf, String>,
}

#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct PullRequestCreateResult(pub GithubPullRequest);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_identity_uses_the_immutable_owner_id() {
        let mut repository = GithubRepository {
            owner_id: 42,
            owner_login: "before".into(),
            ..Default::default()
        };
        let original = repository.scope_id();
        repository.owner_login = "after".into();

        assert_eq!(repository.scope_id(), original);
        assert_eq!(
            original.as_ref().unwrap().as_str(),
            "github.com/accounts/42"
        );
        repository.owner_id = 43;
        assert_ne!(repository.scope_id(), original);
    }

    #[test]
    fn missing_owner_identity_never_creates_a_shared_scope() {
        assert!(GithubIssue::default().scope_id().is_none());
        assert!(GithubPullRequest::default().scope_id().is_none());
    }
}
