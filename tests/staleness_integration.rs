use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

use orthanc_lib::models::environment::{EnvStatus, PythonEnvironment, StalenessInfo};
use orthanc_lib::services::staleness;

fn create_test_env(repo_path: PathBuf) -> PythonEnvironment {
    PythonEnvironment {
        id: "integration-test-env".to_string(),
        name: "integration-env".to_string(),
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
fn test_staleness_full_lifecycle() {
    let dir = TempDir::new().unwrap();

    // Step 1: Create initial requirements.txt
    std::fs::write(
        dir.path().join("requirements.txt"),
        "requests==2.31.0\nflask==3.0.0\n",
    )
    .unwrap();

    // Step 2: Compute initial hash
    let initial_hash = staleness::compute_deps_hash(dir.path()).unwrap();
    assert!(!initial_hash.is_empty());

    // Step 3: Create env with stored hash (simulating a sync)
    let mut env = create_test_env(dir.path().to_path_buf());
    env.staleness.deps_hash = Some(initial_hash.clone());
    env.staleness.last_checked = Some(Utc::now());

    // Step 4: Check staleness - should not be stale
    let info = staleness::check_staleness(&env).unwrap();
    assert!(!info.is_stale);
    assert!(info.reason.is_none());

    // Step 5: Modify requirements.txt
    std::fs::write(
        dir.path().join("requirements.txt"),
        "requests==2.32.0\nflask==3.1.0\ndjango==5.0.0\n",
    )
    .unwrap();

    // Step 6: Check staleness - should now be stale
    let info = staleness::check_staleness(&env).unwrap();
    assert!(info.is_stale);
    assert!(info.reason.is_some());

    // Step 7: "Sync" by updating the stored hash
    env.staleness.deps_hash = info.deps_hash.clone();

    // Step 8: Check again - should no longer be stale
    let info = staleness::check_staleness(&env).unwrap();
    assert!(!info.is_stale);
}

#[test]
fn test_staleness_with_pyproject_and_uv_lock() {
    let dir = TempDir::new().unwrap();

    // Create multiple dep files
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"0.1.0\"\ndependencies = [\"requests\"]\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("uv.lock"), "lock-content-v1\n").unwrap();

    let hash1 = staleness::compute_deps_hash(dir.path()).unwrap();

    // Only change uv.lock
    std::fs::write(dir.path().join("uv.lock"), "lock-content-v2\n").unwrap();

    let hash2 = staleness::compute_deps_hash(dir.path()).unwrap();
    assert_ne!(hash1, hash2, "Hash should change when uv.lock changes");

    // Only change pyproject.toml
    let hash2_baseline = staleness::compute_deps_hash(dir.path()).unwrap();
    std::fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"test\"\nversion = \"0.2.0\"\ndependencies = [\"requests\", \"flask\"]\n",
    )
    .unwrap();
    let hash3 = staleness::compute_deps_hash(dir.path()).unwrap();
    assert_ne!(
        hash2_baseline, hash3,
        "Hash should change when pyproject.toml changes"
    );
}

#[test]
fn test_staleness_empty_repo() {
    let dir = TempDir::new().unwrap();
    // No dependency files
    let env = create_test_env(dir.path().to_path_buf());

    let info = staleness::check_staleness(&env).unwrap();
    // Should not crash, just report gracefully
    assert!(!info.is_stale);
}
