use chrono::{DateTime, Utc};
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};

use crate::error::OrthancError;
use crate::models::github::{
    PRState, PullRequest, WorkflowConclusion, WorkflowRun, WorkflowStatus,
};

// -- Intermediate GitHub API response structs --

#[derive(Debug, Deserialize)]
struct GhWorkflowRunsResponse {
    workflow_runs: Vec<GhWorkflowRun>,
}

#[derive(Debug, Deserialize)]
struct GhWorkflowRun {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: String,
    head_branch: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct GhUser {
    login: String,
}

// GitHub search API response for issues/PRs
#[derive(Debug, Deserialize)]
struct GhSearchResult {
    items: Vec<GhSearchItem>,
}

#[derive(Debug, Deserialize)]
struct GhSearchItem {
    number: u64,
    title: String,
    state: String,
    html_url: String,
    created_at: DateTime<Utc>,
    user: Option<GhUser>,
    pull_request: Option<GhSearchPr>,
    repository_url: String,
}

#[derive(Debug, Deserialize)]
struct GhSearchPr {
    merged_at: Option<DateTime<Utc>>,
}

// -- Query parameter structs --

#[derive(Serialize)]
struct WorkflowRunsParams<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<&'a str>,
}

#[derive(Serialize)]
struct SearchParams<'a> {
    q: &'a str,
    sort: &'a str,
    order: &'a str,
    per_page: u32,
}

// -- Conversion helpers --

fn parse_workflow_status(s: &str) -> WorkflowStatus {
    match s {
        "queued" => WorkflowStatus::Queued,
        "in_progress" => WorkflowStatus::InProgress,
        "completed" => WorkflowStatus::Completed,
        "waiting" => WorkflowStatus::Waiting,
        _ => WorkflowStatus::Queued,
    }
}

fn parse_workflow_conclusion(s: &str) -> WorkflowConclusion {
    match s {
        "success" => WorkflowConclusion::Success,
        "failure" => WorkflowConclusion::Failure,
        "cancelled" => WorkflowConclusion::Cancelled,
        "skipped" => WorkflowConclusion::Skipped,
        "timed_out" => WorkflowConclusion::TimedOut,
        _ => WorkflowConclusion::Failure,
    }
}

fn parse_pr_state(s: &str, merged_at: Option<DateTime<Utc>>) -> PRState {
    if merged_at.is_some() {
        return PRState::Merged;
    }
    match s {
        "open" => PRState::Open,
        "closed" => PRState::Closed,
        _ => PRState::Closed,
    }
}

// -- GitHubClient --

#[derive(Clone)]
pub struct GitHubClient {
    octocrab: Octocrab,
    base_url: String,
}

impl GitHubClient {
    /// Fails when `base_url` is not a valid URI (it comes from user settings).
    pub fn new(token: String, base_url: String) -> Result<Self, OrthancError> {
        let mut builder = Octocrab::builder().personal_token(token);

        if base_url != "https://api.github.com" {
            builder = builder.base_uri(&base_url).map_err(|e| {
                OrthancError::ConfigError(format!(
                    "invalid GitHub API base URL '{}': {}",
                    base_url, e
                ))
            })?;
        }

        let octocrab = builder.build()?;

        Ok(Self { octocrab, base_url })
    }

    pub async fn list_workflow_runs(
        &self,
        owner: &str,
        repo: &str,
        branch: Option<&str>,
    ) -> Result<Vec<WorkflowRun>, OrthancError> {
        let route = format!("/repos/{}/{}/actions/runs", owner, repo);
        let params = WorkflowRunsParams { branch };

        let gh_response: GhWorkflowRunsResponse = self.octocrab.get(&route, Some(&params)).await?;

        let runs = gh_response
            .workflow_runs
            .into_iter()
            .map(|run| WorkflowRun {
                id: run.id,
                name: run.name,
                status: parse_workflow_status(&run.status),
                conclusion: run.conclusion.as_deref().map(parse_workflow_conclusion),
                html_url: run.html_url,
                head_branch: run.head_branch,
                created_at: run.created_at,
                updated_at: run.updated_at,
            })
            .collect();

        Ok(runs)
    }

    pub async fn rerun_workflow(
        &self,
        owner: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<(), OrthancError> {
        let url = format!(
            "{}/repos/{}/{}/actions/runs/{}/rerun",
            self.base_url, owner, repo, run_id
        );
        // The rerun endpoint returns 201 with empty body, so use _post
        // which returns the raw response without attempting JSON deserialization.
        self.octocrab._post(&url, None::<&()>).await?;
        Ok(())
    }

    /// Search pull requests across an entire organization.
    pub async fn search_org_pull_requests(
        &self,
        org: &str,
        state: Option<&str>,
        author: Option<&str>,
    ) -> Result<Vec<PullRequest>, OrthancError> {
        let mut query = format!("type:pr org:{}", org);
        if let Some(s) = state {
            query.push_str(&format!(" state:{}", s));
        }
        if let Some(a) = author {
            if !a.is_empty() {
                query.push_str(&format!(" author:{}", a));
            }
        }

        let params = SearchParams {
            q: &query,
            sort: "updated",
            order: "desc",
            per_page: 100,
        };

        let result: GhSearchResult = self.octocrab.get("/search/issues", Some(&params)).await?;

        let prs = result
            .items
            .into_iter()
            .map(|item| {
                let author = item.user.map(|u| u.login).unwrap_or_default();
                let merged_at = item.pull_request.and_then(|pr| pr.merged_at);
                let repo_full_name = item
                    .repository_url
                    .rsplit_once("/repos/")
                    .map(|(_, name)| name.to_string())
                    .unwrap_or_default();
                PullRequest {
                    number: item.number,
                    title: item.title,
                    state: parse_pr_state(&item.state, merged_at),
                    head_branch: String::new(),
                    base_branch: String::new(),
                    html_url: item.html_url,
                    created_at: item.created_at,
                    author,
                    repo_full_name,
                }
            })
            .collect();

        Ok(prs)
    }
}
