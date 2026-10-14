#[cfg(test)]
mod tests {
    use crate::models::environment::{EnvStatus, PythonEnvironment, StalenessInfo};
    use crate::services::staleness::{check_staleness, compute_deps_hash};
    use chrono::Utc;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_env(repo_path: std::path::PathBuf) -> PythonEnvironment {
        PythonEnvironment {
            id: "test-id".to_string(),
            name: "test-env".to_string(),
            repo_path: repo_path.clone(),
            venv_path: repo_path.join(".venv"),
            python_version: "3.12".to_string(),
            status: EnvStatus::Ready,
            created_at: Utc::now(),
            last_synced_at: None,
            staleness: StalenessInfo::default(),
            env_vars: HashMap::new(),
        }
    }

    #[test]
    fn test_compute_deps_hash_with_requirements_txt() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "requests==2.31.0\nflask==3.0.0\n",
        )
        .unwrap();

        let hash = compute_deps_hash(dir.path()).unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_compute_deps_hash_with_pyproject_toml() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let hash = compute_deps_hash(dir.path()).unwrap();
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_compute_deps_hash_with_multiple_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests==2.31.0\n").unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();

        assert!(compute_deps_hash(dir.path()).is_ok());
    }

    #[test]
    fn test_compute_deps_hash_no_dep_files() {
        let dir = TempDir::new().unwrap();
        let result = compute_deps_hash(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_deps_hash_deterministic() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests==2.31.0\n").unwrap();

        let hash1 = compute_deps_hash(dir.path()).unwrap();
        let hash2 = compute_deps_hash(dir.path()).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_deps_hash_changes_on_content_change() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests==2.31.0\n").unwrap();
        let hash1 = compute_deps_hash(dir.path()).unwrap();

        std::fs::write(
            dir.path().join("requirements.txt"),
            "requests==2.32.0\nflask==3.0.0\n",
        )
        .unwrap();
        let hash2 = compute_deps_hash(dir.path()).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_check_staleness_not_stale_first_time() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests==2.31.0\n").unwrap();

        let env = create_test_env(dir.path().to_path_buf());
        let info = check_staleness(&env).unwrap();

        // No previous hash to compare against
        assert!(!info.is_stale);
        assert!(info.deps_hash.is_some());
        assert!(info.last_checked.is_some());
    }

    #[test]
    fn test_check_staleness_not_stale_same_content() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests==2.31.0\n").unwrap();

        let hash = compute_deps_hash(dir.path()).unwrap();
        let mut env = create_test_env(dir.path().to_path_buf());
        env.staleness.deps_hash = Some(hash);

        let info = check_staleness(&env).unwrap();
        assert!(!info.is_stale);
        assert!(info.reason.is_none());
    }

    #[test]
    fn test_check_staleness_stale_when_content_changed() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests==2.31.0\n").unwrap();

        let mut env = create_test_env(dir.path().to_path_buf());
        env.staleness.deps_hash = Some("old-hash-that-doesnt-match".to_string());

        let info = check_staleness(&env).unwrap();
        assert!(info.is_stale);
        assert!(info.reason.is_some());
        assert!(info.reason.unwrap().contains("changed"));
    }

    #[test]
    fn test_check_staleness_no_dep_files_graceful() {
        let dir = TempDir::new().unwrap();
        // No dep files at all
        let env = create_test_env(dir.path().to_path_buf());

        let info = check_staleness(&env).unwrap();
        assert!(!info.is_stale);
        assert!(info.reason.is_some()); // Reason explains the failure
    }
}
