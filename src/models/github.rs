use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct GitHubRepo {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub default_branch: String,
}

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub state: PRState,
    pub head_branch: String,
    pub base_branch: String,
    pub html_url: String,
    pub created_at: DateTime<Utc>,
    pub author: String,
    pub repo_full_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PRState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub status: WorkflowStatus,
    pub conclusion: Option<WorkflowConclusion>,
    pub html_url: String,
    pub head_branch: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowConclusion {
    Success,
    Failure,
    Cancelled,
    Skipped,
    TimedOut,
}
