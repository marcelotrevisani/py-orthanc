use std::path::Path;
use tokio::process::Command;

use crate::error::OrthancError;
use crate::models::config::UvConfig;

/// A `uv` command that doesn't flash a console window on Windows (the release
/// build has no console of its own).
fn uv_command() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new("uv");
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

/// Environment variables that point uv at the private index, if configured.
///
/// The index is passed as `name=url` so uv applies
/// `UV_INDEX_<NAME>_USERNAME/PASSWORD` to it. With `use_public_index` it goes
/// in `UV_INDEX` (searched first, PyPI stays as fallback); otherwise in
/// `UV_DEFAULT_INDEX`, replacing PyPI.
fn index_env(cfg: &UvConfig) -> Vec<(String, String)> {
    let name = cfg
        .index_name
        .to_lowercase()
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    let env_name = name.to_uppercase().replace('-', "_");
    let mut vars = Vec::new();
    if let Some(url) = cfg.default_index.as_deref().filter(|u| !u.is_empty()) {
        let var = if cfg.use_public_index {
            "UV_INDEX"
        } else {
            "UV_DEFAULT_INDEX"
        };
        let spec = if name.is_empty() {
            url.to_string()
        } else {
            format!("{name}={url}")
        };
        vars.push((var.to_string(), spec));
    }
    if !env_name.is_empty() {
        for (suffix, value) in [("USERNAME", &cfg.username), ("PASSWORD", &cfg.password)] {
            if let Some(v) = value.as_deref().filter(|v| !v.is_empty()) {
                vars.push((format!("UV_INDEX_{env_name}_{suffix}"), v.to_string()));
            }
        }
    }
    vars
}

/// Create a new Python virtual environment using `uv venv`.
pub async fn create_venv(
    venv_path: &Path,
    python_version: &str,
    seed: bool,
    extra_args: &[String],
    uv_config: &UvConfig,
) -> Result<(), OrthancError> {
    let mut cmd = uv_command();
    cmd.arg("venv")
        .arg(venv_path)
        .arg("--python")
        .arg(python_version);

    if seed {
        cmd.arg("--seed");
    }

    for arg in extra_args {
        cmd.arg(arg);
    }

    cmd.envs(index_env(uv_config));

    let output = cmd
        .output()
        .await
        .map_err(|e| OrthancError::UvError(format!("Failed to run uv venv: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrthancError::UvError(format!(
            "uv venv failed (exit {}): {}",
            output.status, stderr
        )));
    }

    log::info!("Created venv at {}", venv_path.display());
    Ok(())
}

/// Install the project in dev mode after venv creation.
///
/// Tries `uv pip install -e . --extra=dev` first.
/// If that fails, falls back to `uv pip install -e .`.
/// Returns `Ok(None)` on success, `Ok(Some(warning))` if install failed.
pub async fn install_dev(
    venv_path: &Path,
    repo_path: &Path,
    uv_config: &UvConfig,
) -> Option<String> {
    let pyproject = repo_path.join("pyproject.toml");
    if !pyproject.exists() {
        return Some("No pyproject.toml found, skipping dev install".to_string());
    }

    // Try with --extra=dev first
    let mut cmd = uv_command();
    cmd.arg("pip")
        .arg("install")
        .arg("-e")
        .arg(".")
        .arg("--extra=dev")
        .env("VIRTUAL_ENV", venv_path)
        .current_dir(repo_path);
    cmd.envs(index_env(uv_config));
    let output = cmd.output().await;

    match output {
        Ok(o) if o.status.success() => {
            log::info!(
                "Installed project in dev mode with extras at {}",
                repo_path.display()
            );
            return None;
        }
        _ => {
            log::warn!("uv pip install -e . --extra=dev failed, trying without extras");
        }
    }

    // Fallback: without --extra=dev
    let mut cmd = uv_command();
    cmd.arg("pip")
        .arg("install")
        .arg("-e")
        .arg(".")
        .env("VIRTUAL_ENV", venv_path)
        .current_dir(repo_path);
    cmd.envs(index_env(uv_config));
    let output = cmd.output().await;

    match output {
        Ok(o) if o.status.success() => {
            log::info!("Installed project in dev mode at {}", repo_path.display());
            None
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            log::warn!("uv pip install -e . failed: {}", stderr);
            Some(format!(
                "Dependencies could not be installed: {}",
                stderr.trim()
            ))
        }
        Err(e) => {
            log::warn!("Failed to run uv pip install: {}", e);
            Some(format!("Dependencies could not be installed: {}", e))
        }
    }
}

/// Delete a virtual environment by removing its directory.
pub async fn delete_venv(venv_path: &Path) -> Result<(), OrthancError> {
    if venv_path.exists() {
        tokio::fs::remove_dir_all(venv_path).await?;
        log::info!("Deleted venv at {}", venv_path.display());
    } else {
        log::warn!("Venv path does not exist: {}", venv_path.display());
    }
    Ok(())
}

/// Sync an environment by installing dependencies.
///
/// If `pyproject.toml` exists in the repo, runs `uv pip install -U -e .`.
/// Otherwise, if `requirements.txt` exists, runs `uv pip install -U -r requirements.txt`.
pub async fn sync_environment(
    venv_path: &Path,
    repo_path: &Path,
    uv_config: &UvConfig,
) -> Result<String, OrthancError> {
    let pyproject = repo_path.join("pyproject.toml");
    let requirements = repo_path.join("requirements.txt");

    let mut cmd = uv_command();
    cmd.arg("pip")
        .arg("install")
        .arg("-U")
        .env("VIRTUAL_ENV", venv_path);

    if pyproject.exists() {
        cmd.arg("-e").arg(".").current_dir(repo_path);
    } else if requirements.exists() {
        cmd.arg("-r").arg(&requirements);
    } else {
        return Err(OrthancError::UvError(
            "No pyproject.toml or requirements.txt found in repo".to_string(),
        ));
    }

    cmd.envs(index_env(uv_config));

    let output = cmd
        .output()
        .await
        .map_err(|e| OrthancError::UvError(format!("Failed to run uv pip install: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(OrthancError::UvError(format!(
            "uv pip install failed (exit {}): {}",
            output.status, stderr
        )));
    }

    log::info!("Synced environment at {}", venv_path.display());
    Ok(format!("{}\n{}", stdout, stderr).trim().to_string())
}

/// List installed packages using `uv pip list --format=json`.
/// Returns a list of (name, version, editable) tuples.
pub async fn pip_list(
    venv_path: &Path,
    uv_config: &UvConfig,
) -> Result<Vec<(String, String, bool)>, OrthancError> {
    let mut cmd = uv_command();
    cmd.arg("pip")
        .arg("list")
        .arg("--format=json")
        .env("VIRTUAL_ENV", venv_path);
    cmd.envs(index_env(uv_config));
    let output = cmd
        .output()
        .await
        .map_err(|e| OrthancError::UvError(format!("Failed to run uv pip list: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrthancError::UvError(format!(
            "uv pip list failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<serde_json::Value> = serde_json::from_str(&stdout)
        .map_err(|e| OrthancError::UvError(format!("Failed to parse uv pip list output: {}", e)))?;

    let packages = entries
        .iter()
        .map(|entry| {
            let name = entry["name"].as_str().unwrap_or("").to_string();
            let version = entry["version"].as_str().unwrap_or("").to_string();
            let is_editable = entry
                .get("editable_project_location")
                .is_some_and(|v| !v.is_null());
            (name, version, is_editable)
        })
        .collect();

    Ok(packages)
}

/// Show package metadata using `uv pip show`.
/// Returns a map of field name -> value.
pub async fn pip_show(
    venv_path: &Path,
    package: &str,
    uv_config: &UvConfig,
) -> Result<std::collections::HashMap<String, String>, OrthancError> {
    let mut cmd = uv_command();
    cmd.arg("pip")
        .arg("show")
        .arg(package)
        .env("VIRTUAL_ENV", venv_path);
    cmd.envs(index_env(uv_config));
    let output = cmd
        .output()
        .await
        .map_err(|e| OrthancError::UvError(format!("Failed to run uv pip show: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrthancError::UvError(format!(
            "uv pip show {} failed: {}",
            package,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut fields = std::collections::HashMap::new();
    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once(": ") {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        } else if let Some(key) = line.strip_suffix(':') {
            fields.insert(key.trim().to_string(), String::new());
        }
    }

    Ok(fields)
}

/// Show metadata for multiple packages at once using `uv pip show`.
/// Returns a Vec of (name, version, requires, required_by) tuples.
pub async fn pip_show_all(
    venv_path: &Path,
    packages: &[String],
    uv_config: &UvConfig,
) -> Result<Vec<(String, String, Vec<String>, Vec<String>)>, OrthancError> {
    if packages.is_empty() {
        return Ok(Vec::new());
    }

    let mut cmd = uv_command();
    cmd.arg("pip").arg("show").env("VIRTUAL_ENV", venv_path);
    for pkg in packages {
        cmd.arg(pkg);
    }
    cmd.envs(index_env(uv_config));

    let output = cmd
        .output()
        .await
        .map_err(|e| OrthancError::UvError(format!("Failed to run uv pip show: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrthancError::UvError(format!(
            "uv pip show failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();
    let mut name = String::new();
    let mut version = String::new();
    let mut requires = Vec::new();
    let mut required_by = Vec::new();

    let parse_csv = |s: &str| -> Vec<String> {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    };

    for line in stdout.lines() {
        if line == "---" {
            if !name.is_empty() {
                results.push((
                    std::mem::take(&mut name),
                    std::mem::take(&mut version),
                    std::mem::take(&mut requires),
                    std::mem::take(&mut required_by),
                ));
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(": ") {
            match key.trim() {
                "Name" => name = value.trim().to_string(),
                "Version" => version = value.trim().to_string(),
                "Requires" => requires = parse_csv(value),
                "Required-by" => required_by = parse_csv(value),
                _ => {}
            }
        } else if let Some(key) = line.strip_suffix(':') {
            match key.trim() {
                "Requires" => requires = Vec::new(),
                "Required-by" => required_by = Vec::new(),
                _ => {}
            }
        }
    }
    // Push the last entry
    if !name.is_empty() {
        results.push((name, version, requires, required_by));
    }

    Ok(results)
}

/// Install or update a package.
/// `spec` can be "requests", "requests==2.31.0", a local folder, or a
/// "git+https://..." URL. `editable` installs a local folder in dev mode (`-e`).
pub async fn pip_install(
    venv_path: &Path,
    spec: &str,
    editable: bool,
    uv_config: &UvConfig,
) -> Result<String, OrthancError> {
    let mut cmd = uv_command();
    cmd.arg("pip").arg("install");
    if editable {
        cmd.arg("-e");
    }
    cmd.arg(spec).env("VIRTUAL_ENV", venv_path);
    cmd.envs(index_env(uv_config));
    let output = cmd
        .output()
        .await
        .map_err(|e| OrthancError::UvError(format!("Failed to run uv pip install: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrthancError::UvError(format!(
            "uv pip install {} failed: {}",
            spec,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout)
}

/// Uninstall a package.
pub async fn pip_uninstall(
    venv_path: &Path,
    package: &str,
    uv_config: &UvConfig,
) -> Result<String, OrthancError> {
    let mut cmd = uv_command();
    cmd.arg("pip")
        .arg("uninstall")
        .arg(package)
        .env("VIRTUAL_ENV", venv_path);
    cmd.envs(index_env(uv_config));
    let output = cmd
        .output()
        .await
        .map_err(|e| OrthancError::UvError(format!("Failed to run uv pip uninstall: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrthancError::UvError(format!(
            "uv pip uninstall {} failed: {}",
            package,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout)
}

/// Check if `uv` is available on the system PATH.
pub async fn is_uv_available() -> Result<bool, OrthancError> {
    match uv_command().arg("--version").output().await {
        Ok(output) => Ok(output.status.success()),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_env_names_index_and_picks_var() {
        let mut cfg = UvConfig {
            default_index: Some("https://pypi.example.com/simple".into()),
            index_name: "My Corp".into(),
            username: Some("u".into()),
            password: Some(String::new()),
            ..UvConfig::default()
        };
        let env = |cfg: &UvConfig| index_env(cfg);
        assert_eq!(
            env(&cfg),
            vec![
                (
                    "UV_DEFAULT_INDEX".to_string(),
                    "my-corp=https://pypi.example.com/simple".to_string()
                ),
                ("UV_INDEX_MY_CORP_USERNAME".to_string(), "u".to_string()),
            ]
        );
        cfg.use_public_index = true;
        assert_eq!(env(&cfg)[0].0, "UV_INDEX");
        cfg.default_index = None;
        assert_eq!(env(&cfg).len(), 1); // credentials only, no index override
    }
}
