use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use vmux_auth::{CredentialLease, CredentialSource};

use crate::{GithubIssue, GithubPullRequest, GithubRepository, IssueState, PullRequestState};

#[derive(Clone)]
pub struct GithubApi {
    credentials: CredentialSource,
    client: Client,
    base: url::Url,
}

impl GithubApi {
    const MAX_ERROR_BODY_BYTES: u64 = 16 * 1024;
    const SEARCH_TIMEOUT: Duration = Duration::from_secs(5);
    pub fn new(credentials: CredentialSource) -> Result<Self, String> {
        static CLIENT: OnceLock<Client> = OnceLock::new();
        let client = if let Some(client) = CLIENT.get() {
            client.clone()
        } else {
            let client = Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|error| error.to_string())?;
            let _ = CLIENT.set(client.clone());
            client
        };
        Ok(Self {
            credentials,
            client,
            base: url::Url::parse("https://api.github.com/").map_err(|error| error.to_string())?,
        })
    }

    pub fn account(&self) -> Result<GithubAccountAccess, String> {
        let account: Account = self.json(self.get(&["user"])?)?;
        Ok(GithubAccountAccess {
            id: account.id,
            login: account.login,
            name: account.name.unwrap_or_default(),
        })
    }

    pub fn repositories(&self, query: &str) -> Result<Vec<GithubRepository>, String> {
        let response: RepositorySearch = self.json(
            self.get(&["search", "repositories"])?
                .query(&[("q", query), ("per_page", "50")])
                .timeout(Self::SEARCH_TIMEOUT),
        )?;
        let mut repositories = Vec::with_capacity(response.items.len());
        for repository in response.items {
            let repository = GithubRepository::from(repository);
            Self::cache_owner(
                &repository.owner_login,
                &repository.name,
                repository.owner_id,
            );
            repositories.push(repository);
        }
        Ok(repositories)
    }

    pub fn repository(&self, owner: &str, repository: &str) -> Result<GithubRepository, String> {
        let repository: ApiRepository = self.json(self.get(&["repos", owner, repository])?)?;
        let repository = GithubRepository::from(repository);
        Self::cache_owner(
            &repository.owner_login,
            &repository.name,
            repository.owner_id,
        );
        Ok(repository)
    }

    pub fn issues(&self, query: &str) -> Result<Vec<GithubIssue>, String> {
        let query = format!("{query} type:issue");
        let response: IssueSearch = self.json(
            self.get(&["search", "issues"])?
                .query(&[("q", query.as_str()), ("per_page", "50")])
                .timeout(Self::SEARCH_TIMEOUT),
        )?;
        let mut issues = Vec::new();
        for issue in response.items {
            if issue.pull_request.is_none() {
                let mut issue: GithubIssue = issue.try_into()?;
                issue.owner_id = Self::cached_owner(&issue.owner, &issue.repository).unwrap_or(0);
                issues.push(issue);
            }
        }
        Ok(issues)
    }

    pub(crate) fn issue_with_owner_id(
        &self,
        coordinates: &crate::GithubCoordinates,
        owner_id: Option<u64>,
    ) -> Result<GithubIssue, String> {
        let number = coordinates
            .number
            .ok_or_else(|| "GitHub issue number is missing".to_string())?;
        let number = number.to_string();
        let issue: ApiIssue = self.json(self.get(&[
            "repos",
            &coordinates.owner,
            &coordinates.repository,
            "issues",
            &number,
        ])?)?;
        let mut issue: GithubIssue = issue.try_into()?;
        issue.owner_id = if let Some(owner_id) = owner_id.filter(|owner_id| *owner_id > 0) {
            owner_id
        } else if let Some(owner_id) = Self::cached_owner(&issue.owner, &issue.repository) {
            owner_id
        } else {
            self.repository(&coordinates.owner, &coordinates.repository)?
                .owner_id
        };
        Ok(issue)
    }

    pub fn pull_request(
        &self,
        coordinates: &crate::GithubCoordinates,
    ) -> Result<GithubPullRequest, String> {
        let number = coordinates
            .number
            .ok_or_else(|| "GitHub pull request number is missing".to_string())?;
        let number = number.to_string();
        let pull: ApiPullRequest = self.json(self.get(&[
            "repos",
            &coordinates.owner,
            &coordinates.repository,
            "pulls",
            &number,
        ])?)?;
        Ok(pull.into_snapshot(&coordinates.owner, &coordinates.repository))
    }

    fn owner_cache() -> &'static Mutex<HashMap<(String, String), u64>> {
        static OWNERS: OnceLock<Mutex<HashMap<(String, String), u64>>> = OnceLock::new();
        OWNERS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn cache_owner(owner: &str, repository: &str, owner_id: u64) {
        if owner_id == 0 {
            return;
        }
        let Ok(mut owners) = Self::owner_cache().lock() else {
            return;
        };
        owners.insert(
            (owner.to_ascii_lowercase(), repository.to_ascii_lowercase()),
            owner_id,
        );
    }

    fn cached_owner(owner: &str, repository: &str) -> Option<u64> {
        Self::owner_cache()
            .lock()
            .ok()?
            .get(&(owner.to_ascii_lowercase(), repository.to_ascii_lowercase()))
            .copied()
    }

    pub fn create_pull_request(
        &self,
        request: &crate::PullRequestCreateRequest,
    ) -> Result<GithubPullRequest, String> {
        let pull: ApiPullRequest = self.json(
            self.post(&["repos", &request.owner, &request.repository, "pulls"])?
                .json(&CreatePullRequest {
                    title: &request.title,
                    body: &request.body,
                    head: &request.head,
                    base: &request.base,
                    draft: request.draft,
                }),
        )?;
        Ok(pull.into_snapshot(&request.owner, &request.repository))
    }

    pub fn create_review(
        &self,
        request: &crate::request::PullRequestReviewRequest,
    ) -> Result<(), String> {
        let number = request.number.to_string();
        let _: serde_json::Value = self.json(
            self.post(&[
                "repos",
                &request.owner,
                &request.repository,
                "pulls",
                &number,
                "reviews",
            ])?
            .json(&CreateReview {
                body: &request.body,
                event: "COMMENT",
                commit_id: &request.commit_id,
                comments: request.comments.as_ref(),
            }),
        )?;
        Ok(())
    }

    fn get(&self, segments: &[&str]) -> Result<GithubRequest, String> {
        self.request(self.client.get(self.url(segments)?))
    }

    fn post(&self, segments: &[&str]) -> Result<GithubRequest, String> {
        self.request(self.client.post(self.url(segments)?))
    }

    fn url(&self, segments: &[&str]) -> Result<url::Url, String> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|_| "GitHub API URL cannot accept path segments".to_string())?
            .clear()
            .extend(segments.iter().copied());
        Ok(url)
    }

    fn request(&self, request: RequestBuilder) -> Result<GithubRequest, String> {
        let lease = self.credentials.lease()?;
        Ok(GithubRequest {
            request: request
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "vmux"),
            lease,
        })
    }

    fn json<T: for<'de> Deserialize<'de>>(&self, request: GithubRequest) -> Result<T, String> {
        let response = request.send()?;
        Self::decode(response)
    }

    fn decode<T: for<'de> Deserialize<'de>>(response: Response) -> Result<T, String> {
        let status = response.status();
        if !status.is_success() {
            let mut body = String::new();
            let _ = response
                .take(Self::MAX_ERROR_BODY_BYTES)
                .read_to_string(&mut body);
            let body = body.trim();
            if body.is_empty() {
                return Err(format!("GitHub request failed ({status})"));
            }
            return Err(format!("GitHub request failed ({status}): {body}"));
        }
        response
            .json()
            .map_err(|error| format!("invalid GitHub response: {error}"))
    }
}

struct GithubRequest {
    request: RequestBuilder,
    lease: CredentialLease,
}

impl GithubRequest {
    fn query<T: Serialize + ?Sized>(mut self, query: &T) -> Self {
        self.request = self.request.query(query);
        self
    }

    fn json<T: Serialize + ?Sized>(mut self, value: &T) -> Self {
        self.request = self.request.json(value);
        self
    }

    fn timeout(mut self, timeout: Duration) -> Self {
        self.request = self.request.timeout(timeout);
        self
    }

    fn send(self) -> Result<Response, String> {
        let token = self.lease.token()?;
        self.request
            .bearer_auth(token)
            .send()
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct GithubAccountAccess {
    pub id: u64,
    pub login: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Account {
    pub id: u64,
    pub node_id: String,
    pub login: String,
    pub name: Option<String>,
}

#[derive(Deserialize)]
struct RepositorySearch {
    items: Vec<ApiRepository>,
}

#[derive(Deserialize)]
struct ApiRepository {
    id: u64,
    node_id: String,
    name: String,
    full_name: String,
    html_url: String,
    clone_url: String,
    owner: Account,
    default_branch: String,
    private: bool,
    updated_at: String,
}

impl From<ApiRepository> for GithubRepository {
    fn from(repository: ApiRepository) -> Self {
        Self {
            id: repository.id,
            node_id: repository.node_id,
            name: repository.name,
            full_name: repository.full_name,
            html_url: repository.html_url,
            clone_url: repository.clone_url,
            owner_id: repository.owner.id,
            owner_login: repository.owner.login,
            owner_node_id: repository.owner.node_id,
            default_branch: repository.default_branch,
            private: repository.private,
            updated_at: repository.updated_at,
        }
    }
}

#[derive(Deserialize)]
struct IssueSearch {
    items: Vec<ApiIssue>,
}

#[derive(Deserialize)]
struct ApiIssue {
    id: u64,
    node_id: String,
    number: u64,
    title: String,
    body: Option<String>,
    state: IssueState,
    html_url: String,
    repository_url: String,
    updated_at: String,
    pull_request: Option<serde_json::Value>,
}

impl TryFrom<ApiIssue> for GithubIssue {
    type Error = String;

    fn try_from(issue: ApiIssue) -> Result<Self, Self::Error> {
        let url = url::Url::parse(&issue.repository_url)
            .map_err(|error| format!("invalid GitHub repository URL: {error}"))?;
        let segments = url
            .path_segments()
            .map(|segments| segments.collect::<Vec<_>>())
            .unwrap_or_default();
        let marker = segments
            .iter()
            .position(|segment| *segment == "repos")
            .ok_or_else(|| "GitHub repository URL has no repository".to_string())?;
        let owner = segments
            .get(marker + 1)
            .ok_or_else(|| "GitHub repository URL has no owner".to_string())?
            .to_string();
        let repository = segments
            .get(marker + 2)
            .ok_or_else(|| "GitHub repository URL has no name".to_string())?
            .to_string();
        Ok(Self {
            id: issue.id,
            node_id: issue.node_id,
            number: issue.number,
            title: issue.title,
            body: issue.body.unwrap_or_default(),
            state: issue.state,
            html_url: issue.html_url,
            repository_url: issue.repository_url,
            repository,
            owner,
            owner_id: 0,
            updated_at: issue.updated_at,
        })
    }
}

#[derive(Deserialize)]
struct ApiPullRequest {
    id: u64,
    node_id: String,
    number: u64,
    title: String,
    body: Option<String>,
    state: PullRequestState,
    html_url: String,
    head: PullRef,
    base: PullRef,
    draft: bool,
    merged: bool,
    updated_at: String,
}

impl ApiPullRequest {
    fn into_snapshot(self, owner: &str, repository: &str) -> GithubPullRequest {
        let PullRef {
            reference: head,
            sha: head_sha,
            repository: head_repository,
        } = self.head;
        let PullRef {
            reference: base,
            sha: base_sha,
            repository: base_repository,
        } = self.base;
        GithubPullRequest {
            id: self.id,
            node_id: self.node_id,
            number: self.number,
            title: self.title,
            body: self.body.unwrap_or_default(),
            state: self.state,
            html_url: self.html_url,
            owner: owner.to_string(),
            repository: repository.to_string(),
            owner_id: base_repository
                .as_ref()
                .map(|repository| repository.owner.id)
                .unwrap_or_default(),
            head,
            head_owner: head_repository
                .as_ref()
                .map(|repository| repository.owner.login.clone())
                .unwrap_or_default(),
            head_repository: head_repository
                .map(|repository| repository.name)
                .unwrap_or_default(),
            head_sha,
            base,
            base_sha,
            draft: self.draft,
            merged: self.merged,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Deserialize)]
struct PullRef {
    #[serde(rename = "ref")]
    reference: String,
    sha: String,
    #[serde(rename = "repo")]
    repository: Option<PullRepository>,
}

#[derive(Deserialize)]
struct PullRepository {
    name: String,
    owner: Account,
}

#[derive(Serialize)]
struct CreatePullRequest<'a> {
    title: &'a str,
    body: &'a str,
    head: &'a str,
    base: &'a str,
    draft: bool,
}

#[derive(Serialize)]
struct CreateReview<'a> {
    body: &'a str,
    event: &'static str,
    commit_id: &'a str,
    comments: &'a [crate::request::GithubReviewComment],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_payload_uses_modern_line_and_side_coordinates() {
        let comments = vec![crate::request::GithubReviewComment {
            path: "src/lib.rs".into(),
            body: "Handle the error".into(),
            line: 12,
            side: crate::request::GithubReviewSide::Right,
        }];
        let payload = CreateReview {
            body: "Review from vmux",
            event: "COMMENT",
            commit_id: "0123456789abcdef",
            comments: &comments,
        };
        let json = serde_json::to_value(payload).unwrap();

        assert_eq!(json["event"], "COMMENT");
        assert_eq!(json["commit_id"], "0123456789abcdef");
        assert_eq!(json["comments"][0]["path"], "src/lib.rs");
        assert_eq!(json["comments"][0]["line"], 12);
        assert_eq!(json["comments"][0]["side"], "RIGHT");
    }
}
