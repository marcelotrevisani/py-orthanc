use crate::error::OrthancError;

/// `owner/name` of the GitHub repo releases come from, taken from the
/// `repository` URL in Cargo.toml. `None` (update checks off) when unset.
fn release_repo(repository_url: &str) -> Option<&str> {
    let repo = repository_url
        .strip_prefix("https://github.com/")?
        .trim_end_matches('/')
        .trim_end_matches(".git");
    repo.contains('/').then_some(repo)
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: String,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub body: String,
}

/// Build an Octocrab instance with optional auth (sync so it stays out of
/// the async state machine — OctocrabBuilder is not Send).
fn build_octocrab(pat: &Option<String>) -> Result<octocrab::Octocrab, OrthancError> {
    let mut builder = octocrab::Octocrab::builder();
    if let Some(token) = pat {
        if !token.is_empty() {
            builder = builder.personal_token(token.clone());
        }
    }
    builder
        .build()
        .map_err(|e| OrthancError::Other(format!("Failed to build API client: {}", e)))
}

/// Check the latest release on GitHub and return its info.
pub async fn check_for_update(
    current_version: &str,
    pat: &Option<String>,
) -> Result<Option<ReleaseInfo>, OrthancError> {
    let Some(repo) = release_repo(env!("CARGO_PKG_REPOSITORY")) else {
        return Ok(None);
    };
    let octocrab = build_octocrab(pat)?;

    let route = format!("/repos/{}/releases/latest", repo);
    let release: serde_json::Value = match octocrab.get(&route, None::<&()>).await {
        Ok(v) => v,
        // 404: no release published yet, or a private repo and no PAT.
        Err(e) => match OrthancError::from(e) {
            OrthancError::GitHubError { status: 404, .. } => return Ok(None),
            err => return Err(err),
        },
    };

    let tag = release["tag_name"].as_str().unwrap_or("").to_string();
    let version = tag.strip_prefix('v').unwrap_or(&tag).to_string();
    let body = release["body"].as_str().unwrap_or("").to_string();

    if !is_newer(&version, current_version) {
        return Ok(None);
    }

    // Find the matching asset for this platform
    let (download_url, asset_name) = find_platform_asset(&release);

    Ok(Some(ReleaseInfo {
        tag,
        version,
        download_url,
        asset_name,
        body,
    }))
}

/// Download the release binary and replace the current executable.
pub async fn download_and_install(url: &str, pat: &Option<String>) -> Result<(), OrthancError> {
    let client = reqwest::Client::builder()
        .user_agent("orthanc-updater")
        .build()
        .map_err(|e| OrthancError::Other(e.to_string()))?;

    let mut req = client.get(url);
    if let Some(token) = pat {
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }
    req = req.header("Accept", "application/octet-stream");

    log::info!("Downloading update from {}", url);
    let resp = req
        .send()
        .await
        .map_err(|e| OrthancError::Other(format!("Failed to download update: {}", e)))?;

    if !resp.status().is_success() {
        return Err(OrthancError::Other(format!(
            "Download failed (status {})",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| OrthancError::Other(format!("Failed to read download: {}", e)))?;

    let current_exe = std::env::current_exe().map_err(|e| {
        OrthancError::Other(format!("Cannot determine current executable path: {}", e))
    })?;

    let parent = current_exe
        .parent()
        .ok_or_else(|| OrthancError::Other("Cannot determine executable directory".to_string()))?;

    let new_path = parent.join("orthanc.new");
    let old_path = parent.join("orthanc.old");

    // Write the new binary
    tokio::fs::write(&new_path, &bytes)
        .await
        .map_err(|e| OrthancError::Other(format!("Failed to write new binary: {}", e)))?;

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        tokio::fs::set_permissions(&new_path, perms)
            .await
            .map_err(|e| {
                OrthancError::Other(format!("Failed to set executable permission: {}", e))
            })?;
    }

    // Rename current -> old, new -> current
    if old_path.exists() {
        tokio::fs::remove_file(&old_path).await.ok();
    }

    tokio::fs::rename(&current_exe, &old_path)
        .await
        .map_err(|e| OrthancError::Other(format!("Failed to rename current binary: {}", e)))?;

    tokio::fs::rename(&new_path, &current_exe)
        .await
        .map_err(|e| {
            // Try to restore old binary
            let _ = std::fs::rename(&old_path, &current_exe);
            OrthancError::Other(format!("Failed to install new binary: {}", e))
        })?;

    // Clean up old binary
    tokio::fs::remove_file(&old_path).await.ok();

    log::info!("Update installed successfully, restart to apply");
    Ok(())
}

/// Compare two semver-like version strings. Returns true if `new` is greater than `current`.
fn is_newer(new: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    let new_parts = parse(new);
    let cur_parts = parse(current);

    for i in 0..new_parts.len().max(cur_parts.len()) {
        let n = new_parts.get(i).copied().unwrap_or(0);
        let c = cur_parts.get(i).copied().unwrap_or(0);
        if n > c {
            return true;
        }
        if n < c {
            return false;
        }
    }
    false
}

/// Find the download URL and asset name for the current platform.
fn find_platform_asset(release: &serde_json::Value) -> (Option<String>, Option<String>) {
    let assets = match release["assets"].as_array() {
        Some(a) => a,
        None => return (None, None),
    };

    let target = platform_asset_suffix();

    for asset in assets {
        let name = asset["name"].as_str().unwrap_or("");
        if name.contains(target) {
            let url = asset["browser_download_url"]
                .as_str()
                .unwrap_or("")
                .to_string();
            return (Some(url), Some(name.to_string()));
        }
    }
    (None, None)
}

fn platform_asset_suffix() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "darwin-x86_64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "darwin-aarch64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_release_repo_from_cargo_url() {
        assert_eq!(
            release_repo("https://github.com/me/orthanc"),
            Some("me/orthanc")
        );
        assert_eq!(
            release_repo("https://github.com/me/orthanc.git"),
            Some("me/orthanc")
        );
        assert_eq!(
            release_repo("https://github.com/me/orthanc/"),
            Some("me/orthanc")
        );
        assert_eq!(release_repo(""), None);
        assert_eq!(release_repo("https://gitlab.com/me/orthanc"), None);
    }

    #[test]
    fn test_is_newer_basic() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
    }

    #[test]
    fn test_is_newer_same_version() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_is_newer_older() {
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn test_is_newer_different_lengths() {
        assert!(is_newer("0.1.1", "0.1"));
        assert!(!is_newer("0.1", "0.1.1"));
        assert!(is_newer("1.0", "0.9.9"));
    }

    #[test]
    fn test_platform_asset_suffix_not_empty() {
        let suffix = platform_asset_suffix();
        assert!(!suffix.is_empty());
    }

    #[test]
    fn test_find_platform_asset_empty_release() {
        let release = serde_json::json!({"assets": []});
        let (url, name) = find_platform_asset(&release);
        assert!(url.is_none());
        assert!(name.is_none());
    }

    #[test]
    fn test_find_platform_asset_no_match() {
        let release = serde_json::json!({
            "assets": [
                {"name": "orthanc-freebsd-riscv64", "browser_download_url": "https://example.com/x"}
            ]
        });
        let (url, name) = find_platform_asset(&release);
        assert!(url.is_none());
        assert!(name.is_none());
    }

    #[test]
    fn test_find_platform_asset_matches_current_platform() {
        let suffix = platform_asset_suffix();
        let release = serde_json::json!({
            "assets": [
                {
                    "name": format!("orthanc-{}", suffix),
                    "browser_download_url": "https://example.com/download"
                }
            ]
        });
        let (url, name) = find_platform_asset(&release);
        assert_eq!(url.as_deref(), Some("https://example.com/download"));
        assert!(name.unwrap().contains(suffix));
    }
}
