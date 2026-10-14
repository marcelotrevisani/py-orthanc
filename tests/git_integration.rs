use tempfile::TempDir;

use orthanc_lib::services::git;

#[test]
fn test_get_repo_from_real_git_dir_https() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "https://github.com/octo-org/myproject.git")
        .unwrap();

    let result = git::get_repo_from_path(dir.path()).unwrap();
    assert_eq!(result.owner, "octo-org");
    assert_eq!(result.name, "myproject");
    assert_eq!(result.full_name, "octo-org/myproject");
}

#[test]
fn test_get_repo_from_real_git_dir_ssh() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("origin", "git@github.com:MyOrg/my-project.git")
        .unwrap();

    let result = git::get_repo_from_path(dir.path()).unwrap();
    assert_eq!(result.owner, "MyOrg");
    assert_eq!(result.name, "my-project");
}

#[test]
fn test_get_repo_from_path_no_git_dir() {
    let dir = TempDir::new().unwrap();
    let result = git::get_repo_from_path(dir.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Not a git repository"));
}

#[test]
fn test_get_repo_from_path_no_remote_origin() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.remote("upstream", "https://github.com/other/repo.git")
        .unwrap();

    let result = git::get_repo_from_path(dir.path());
    assert!(result.is_err());
}

#[test]
fn test_get_current_branch() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();

    // Create an initial commit so HEAD points to a real branch
    let sig = git2::Signature::now("test", "test@test.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();

    // Create and switch to a feature branch
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature/my-feature", &head_commit, false)
        .unwrap();
    repo.set_head("refs/heads/feature/my-feature").unwrap();

    let branch = git::get_current_branch(dir.path()).unwrap();
    assert_eq!(branch, "feature/my-feature");
}

#[test]
fn test_get_current_branch_detached_head() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();

    // Create an initial commit
    let sig = git2::Signature::now("test", "test@test.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();

    // Detach HEAD
    repo.set_head_detached(oid).unwrap();

    let branch = git::get_current_branch(dir.path()).unwrap();
    assert_eq!(branch.len(), 8); // Short hash
}

#[test]
fn test_get_current_branch_main() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init_opts(
        dir.path(),
        git2::RepositoryInitOptions::new().initial_head("main"),
    )
    .unwrap();

    // Create an initial commit so HEAD resolves
    let sig = git2::Signature::now("test", "test@test.com").unwrap();
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();

    let branch = git::get_current_branch(dir.path()).unwrap();
    assert_eq!(branch, "main");
}

// ── get_status ──────────────────────────────────────────────────────────

/// Commit everything in the working tree onto HEAD and return the new commit id.
fn commit_all(repo: &git2::Repository, msg: &str) -> git2::Oid {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("Test", "test@test.com").unwrap();
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parents: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents)
        .unwrap()
}

#[test]
fn test_get_status_not_a_repo() {
    let dir = TempDir::new().unwrap();
    assert_eq!(git::get_status(dir.path()).unwrap(), None);
}

#[test]
fn test_get_status_clean_after_commit() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    commit_all(&repo, "init");

    let status = git::get_status(dir.path()).unwrap().unwrap();
    assert!(status.is_clean());
    assert!(!status.detached);
    assert_eq!(status.upstream, None);
    assert_eq!((status.ahead, status.behind), (0, 0));
}

#[test]
fn test_get_status_unborn_branch_uses_real_name() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    repo.set_head("refs/heads/trunk").unwrap();
    let status = git::get_status(dir.path()).unwrap().unwrap();
    assert_eq!(status.branch, "trunk");
}

#[test]
fn test_get_status_counts_changes() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("tracked.txt"), "v1").unwrap();
    std::fs::write(dir.path().join("staged.txt"), "v1").unwrap();
    std::fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
    commit_all(&repo, "init");

    std::fs::write(dir.path().join("tracked.txt"), "v2").unwrap(); // modified
    std::fs::write(dir.path().join("staged.txt"), "v2").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("staged.txt")).unwrap(); // staged
    index.write().unwrap();
    std::fs::create_dir(dir.path().join("new")).unwrap();
    std::fs::write(dir.path().join("new/one.txt"), "x").unwrap(); // untracked
    std::fs::write(dir.path().join("new/two.txt"), "x").unwrap(); // untracked
    std::fs::write(dir.path().join("ignored.txt"), "x").unwrap(); // ignored

    let status = git::get_status(dir.path()).unwrap().unwrap();
    assert_eq!(status.staged, 1);
    assert_eq!(status.modified, 1);
    assert_eq!(status.untracked, 2);
    assert_eq!(status.conflicted, 0);
    assert!(!status.is_clean());
    assert_eq!(status.changed(), 4);
}

#[test]
fn test_get_status_detached_head() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let oid = commit_all(&repo, "init");
    repo.set_head_detached(oid).unwrap();

    let status = git::get_status(dir.path()).unwrap().unwrap();
    assert!(status.detached);
    assert_eq!(status.branch, oid.to_string()[..8]);
    assert_eq!(status.upstream, None);
}

#[test]
fn test_get_status_ahead_behind_upstream() {
    let dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), "1").unwrap();
    let base = commit_all(&repo, "base");
    let local_name = repo.head().unwrap().shorthand().unwrap().to_string();

    // "upstream" branch gets one commit the local branch doesn't have.
    let base_commit = repo.find_commit(base).unwrap();
    repo.branch("upstream", &base_commit, false).unwrap();
    repo.set_head("refs/heads/upstream").unwrap();
    std::fs::write(dir.path().join("a.txt"), "upstream").unwrap();
    commit_all(&repo, "upstream change");

    // Local branch gets two commits of its own.
    repo.set_head(&format!("refs/heads/{}", local_name))
        .unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    for i in 0..2 {
        std::fs::write(dir.path().join("a.txt"), format!("local {}", i)).unwrap();
        commit_all(&repo, "local change");
    }
    repo.find_branch(&local_name, git2::BranchType::Local)
        .unwrap()
        .set_upstream(Some("upstream"))
        .unwrap();

    let status = git::get_status(dir.path()).unwrap().unwrap();
    assert_eq!(status.branch, local_name);
    assert_eq!(status.upstream.as_deref(), Some("upstream"));
    assert_eq!((status.ahead, status.behind), (2, 1));
    assert!(status.is_clean());
}
