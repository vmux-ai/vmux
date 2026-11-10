use bevy::prelude::*;

pub use model::{
    GithubCoordinates, GithubIssue, GithubPullRequest, GithubRepository, IssueSearchResult,
    IssueState, PullRequestState, RepositoryCloneResult, RepositorySearchResult,
};
pub use request::{
    IssueAssociateRequest, IssueDetachRequest, IssueSearchRequest, PullRequestCreateRequest,
    RefreshRequest, RepositoryAssociateRequest, RepositoryCloneRequest, RepositorySearchRequest,
};

mod api;
mod git_credentials;
mod host;
mod model;
mod request;
mod review;

pub struct GithubPlugin;

impl Plugin for GithubPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(vmux_ecs::manifest::FeaturePlugin::<Feature>::default())
            .add_plugins((host::GithubHostPlugin, review::GithubReviewPlugin));
    }
}

struct Feature;

impl vmux_ecs::manifest::FeatureManifestSource for Feature {
    const SOURCE: &'static str = include_str!("feature.ron");
}
