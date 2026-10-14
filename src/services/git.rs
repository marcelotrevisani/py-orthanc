use std::path::Path;

use crate::error::OrthancError;
use crate::models::github::GitHubRepo;

/// Open the git repository at `repo_path`, read the "origin" remote URL,
/// and derive the GitHub owner / repo name.
pub fn get_repo_from_path(repo_path: &Path) -> Result<GitHubRepo, OrthancError> {
    let repo = git2::Repository::open(repo_path).map_err(|_| {
        OrthancError::Other(format!("Not a git repository: {}", repo_path.display()))
    })?;

    let remote = repo.find_remote("origin").map_err(|_| {
        OrthancError::Other("No remote 'origin' URL found in .git/config".to_string())
    })?;

    let url = remote
        .url()
        .ok_or_else(|| OrthancError::Other("Remote 'origin' has no URL".to_string()))?
        .to_string();

    parse_github_url(&url)
}

/// Read the current branch (or short commit hash for detached HEAD).
pub fn get_current_branch(repo_path: &Path) -> Result<String, OrthancError> {
    let repo = git2::Repository::open(repo_path).map_err(|_| {
        OrthancError::Other(format!("Not a git repository: {}", repo_path.display()))
    })?;
    head_name(&repo).map(|(name, _)| name)
}

/// Branch name and whether HEAD is detached (then the name is the short hash).
fn head_name(repo: &git2::Repository) -> Result<(String, bool), OrthancError> {
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
            return Ok((unborn_branch_name(repo), false))
        }
        Err(e) => return Err(OrthancError::Other(format!("Failed to read HEAD: {}", e))),
    };

    if head.is_branch() {
        Ok((head.shorthand().unwrap_or("HEAD").to_string(), false))
    } else {
        let oid = head
            .target()
            .ok_or_else(|| OrthancError::Other("HEAD has no target".to_string()))?;
        let hash = oid.to_string();
        Ok((hash[..8.min(hash.len())].to_string(), true))
    }
}

/// Branch HEAD points at before the first commit (e.g. "main" right after `git init`).
fn unborn_branch_name(repo: &git2::Repository) -> String {
    repo.find_reference("HEAD")
        .ok()
        .and_then(|r| r.symbolic_target().map(str::to_string))
        .and_then(|t| t.strip_prefix("refs/heads/").map(str::to_string))
        .unwrap_or_else(|| "HEAD".to_string())
}

/// Branch, working-tree changes and ahead/behind counts for a repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    pub detached: bool,
    /// Upstream branch, e.g. "origin/main".
    pub upstream: Option<String>,
    pub staged: usize,
    pub modified: usize,
    pub untracked: usize,
    pub conflicted: usize,
    /// Commits ahead of / behind the upstream, as of the last fetch.
    pub ahead: usize,
    pub behind: usize,
}

impl GitStatus {
    pub fn is_clean(&self) -> bool {
        self.staged + self.modified + self.untracked + self.conflicted == 0
    }

    /// Number of distinct kinds of change summed; a file both staged and modified counts twice.
    pub fn changed(&self) -> usize {
        self.staged + self.modified + self.untracked + self.conflicted
    }
}

/// Git status of the repository containing `repo_path`, or `None` if it is not in one.
/// Purely local: ahead/behind uses whatever was last fetched.
pub fn get_status(repo_path: &Path) -> Result<Option<GitStatus>, OrthancError> {
    let repo = match git2::Repository::discover(repo_path) {
        Ok(r) => r,
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if repo.is_bare() {
        return Ok(None);
    }

    let (branch, detached) = head_name(&repo)?;
    let mut status = GitStatus {
        branch,
        detached,
        ..Default::default()
    };

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    for entry in repo.statuses(Some(&mut opts))?.iter() {
        let s = entry.status();
        if s.is_conflicted() {
            status.conflicted += 1;
            continue;
        }
        if s.is_wt_new() {
            status.untracked += 1;
        }
        if s.intersects(
            git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED
                | git2::Status::INDEX_TYPECHANGE,
        ) {
            status.staged += 1;
        }
        if s.intersects(
            git2::Status::WT_MODIFIED
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        ) {
            status.modified += 1;
        }
    }

    if !detached {
        if let Ok(local) = repo.find_branch(&status.branch, git2::BranchType::Local) {
            if let Ok(upstream) = local.upstream() {
                status.upstream = upstream.name().ok().flatten().map(str::to_string);
                if let (Some(l), Some(u)) = (local.get().target(), upstream.get().target()) {
                    let (ahead, behind) = repo.graph_ahead_behind(l, u)?;
                    status.ahead = ahead;
                    status.behind = behind;
                }
            }
        }
    }

    Ok(Some(status))
}

/// Parse a GitHub URL (HTTPS or SSH) into owner and repo name.
fn parse_github_url(url: &str) -> Result<GitHubRepo, OrthancError> {
    // Handle SSH: git@github.com:owner/repo.git
    // Handle HTTPS: https://github.com/owner/repo.git

    let path = if let Some(rest) = url.strip_prefix("git@") {
        // git@github.com:owner/repo.git
        rest.split(':')
            .nth(1)
            .ok_or_else(|| OrthancError::Other(format!("Invalid SSH git URL: {}", url)))?
            .to_string()
    } else if url.starts_with("https://") || url.starts_with("http://") {
        // https://github.com/owner/repo.git
        let parsed = url
            .split("//")
            .nth(1)
            .and_then(|rest| {
                let parts: Vec<&str> = rest.splitn(2, '/').collect();
                if parts.len() == 2 {
                    Some(parts[1].to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| OrthancError::Other(format!("Invalid HTTPS git URL: {}", url)))?;
        parsed
    } else {
        return Err(OrthancError::Other(format!(
            "Unrecognized git URL format: {}",
            url
        )));
    };

    // Strip .git suffix
    let path = path.strip_suffix(".git").unwrap_or(&path);

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 {
        return Err(OrthancError::Other(format!(
            "Could not parse owner/repo from URL: {}",
            url
        )));
    }

    let owner = parts[0].to_string();
    let name = parts[1].to_string();
    let full_name = format!("{}/{}", owner, name);

    Ok(GitHubRepo {
        owner,
        name,
        full_name,
        default_branch: "main".to_string(), // Default; actual value needs API call
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_https_url() {
        let repo = parse_github_url("https://github.com/owner/repo.git").unwrap();
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.name, "repo");
        assert_eq!(repo.full_name, "owner/repo");
    }

    #[test]
    fn test_parse_https_url_no_git_suffix() {
        let repo = parse_github_url("https://github.com/owner/repo").unwrap();
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.name, "repo");
    }

    #[test]
    fn test_parse_ssh_url() {
        let repo = parse_github_url("git@github.com:owner/repo.git").unwrap();
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.name, "repo");
        assert_eq!(repo.full_name, "owner/repo");
    }
}
