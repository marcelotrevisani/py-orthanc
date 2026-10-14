use std::path::Path;

use chrono::Utc;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::error::OrthancError;
use crate::models::environment::{PythonEnvironment, StalenessInfo};

/// Compute a hash of the dependency files in a repository.
///
/// Hashes the contents of `requirements.txt`, `pyproject.toml`, and `uv.lock`
/// (whichever exist) concatenated together.
pub fn compute_deps_hash(repo_path: &Path) -> Result<String, OrthancError> {
    let dep_files = ["requirements.txt", "pyproject.toml", "uv.lock"];
    let mut hasher = DefaultHasher::new();
    let mut found_any = false;

    for file_name in &dep_files {
        let file_path = repo_path.join(file_name);
        if file_path.exists() {
            let contents = std::fs::read_to_string(&file_path)?;
            (file_name, &contents).hash(&mut hasher);
            found_any = true;
        }
    }

    if !found_any {
        return Err(OrthancError::Other(
            "No dependency files found in repository".to_string(),
        ));
    }

    // ponytail: in-memory only; not stable across Rust versions, so never persist it.
    Ok(format!("{:x}", hasher.finish()))
}

/// Check whether an environment is stale by comparing the current dependency
/// hash against the stored one.
pub fn check_staleness(env: &PythonEnvironment) -> Result<StalenessInfo, OrthancError> {
    let current_hash = match compute_deps_hash(&env.repo_path) {
        Ok(hash) => hash,
        Err(e) => {
            return Ok(StalenessInfo {
                is_stale: false,
                reason: Some(format!("Could not compute deps hash: {}", e)),
                deps_hash: None,
                last_checked: Some(Utc::now()),
            });
        }
    };

    let is_stale = match &env.staleness.deps_hash {
        Some(stored_hash) => stored_hash != &current_hash,
        None => false, // First check, nothing to compare against
    };

    let reason = if is_stale {
        Some("Dependency files have changed since last sync".to_string())
    } else {
        None
    };

    Ok(StalenessInfo {
        is_stale,
        reason,
        deps_hash: Some(current_hash),
        last_checked: Some(Utc::now()),
    })
}
