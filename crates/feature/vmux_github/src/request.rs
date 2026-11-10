use std::path::PathBuf;
use std::sync::Arc;
use vmux_api::Id;
use vmux_auth::Connection;
use vmux_connector::SyncPolicy;
use vmux_editor::Project;
use vmux_session::SessionId;

use crate::{GithubIssue, GithubRepository};

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct RepositorySearchRequest {
    pub requester: bevy_ecs::prelude::Entity,
    pub generation: u64,
    pub connection: Id<Connection>,
    pub query: String,
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct RepositoryAssociateRequest {
    pub connection: Id<Connection>,
    pub project: Id<Project>,
    pub repository: GithubRepository,
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct RepositoryCloneRequest {
    pub requester: bevy_ecs::prelude::Entity,
    pub generation: u64,
    pub connection: Id<Connection>,
    pub repository: GithubRepository,
    pub destination: PathBuf,
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct IssueSearchRequest {
    pub requester: bevy_ecs::prelude::Entity,
    pub generation: u64,
    pub connection: Id<Connection>,
    pub query: String,
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct IssueAssociateRequest {
    pub connection: Id<Connection>,
    pub session: SessionId,
    pub issue: GithubIssue,
    pub sync_policy: SyncPolicy,
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct IssueDetachRequest {
    pub session: SessionId,
    pub external_id: String,
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct PullRequestCreateRequest {
    pub connection: Id<Connection>,
    pub session: SessionId,
    pub owner: String,
    pub repository: String,
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct GithubReviewComment {
    pub path: String,
    pub body: String,
    pub line: u32,
    pub side: GithubReviewSide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) enum GithubReviewSide {
    #[serde(rename = "LEFT")]
    Left,
    #[serde(rename = "RIGHT")]
    Right,
}

impl From<vmux_git::state::GitReviewSide> for GithubReviewSide {
    fn from(side: vmux_git::state::GitReviewSide) -> Self {
        match side {
            vmux_git::state::GitReviewSide::Left => Self::Left,
            vmux_git::state::GitReviewSide::Right => Self::Right,
        }
    }
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub(crate) struct PullRequestReviewRequest {
    pub operation: bevy_ecs::prelude::Entity,
    pub connection: Id<Connection>,
    pub owner: String,
    pub repository: String,
    pub number: u64,
    pub base_sha: String,
    pub commit_id: String,
    pub body: String,
    pub comments: Arc<[GithubReviewComment]>,
    pub source: bevy_ecs::prelude::Entity,
    pub notes: Arc<[vmux_git::state::GitReviewNoteReceipt]>,
    pub link: bevy_ecs::prelude::Entity,
    pub revision: vmux_ecs::Revision,
}

#[derive(bevy_ecs::prelude::Message, Clone, Debug)]
pub struct RefreshRequest {
    pub connection: Id<Connection>,
    pub link: bevy_ecs::prelude::Entity,
}
