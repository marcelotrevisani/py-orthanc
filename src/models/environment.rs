use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PythonEnvironment {
    pub id: String,
    pub name: String,
    pub repo_path: PathBuf,
    pub venv_path: PathBuf,
    pub python_version: String,
    pub status: EnvStatus,
    pub created_at: DateTime<Utc>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub staleness: StalenessInfo,
    pub env_vars: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnvStatus {
    Ready,
    Creating,
    Syncing,
    Error(String),
    Stale,
}

#[derive(Debug, Clone, Default)]
pub struct StalenessInfo {
    pub is_stale: bool,
    pub reason: Option<String>,
    pub deps_hash: Option<String>,
    pub last_checked: Option<DateTime<Utc>>,
}
