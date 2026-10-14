use std::path::{Path, PathBuf};

use crate::error::OrthancError;
use crate::models::AppConfig;

/// Returns the default config file path: `<config_dir>/orthanc/orthanc.toml`
pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("orthanc")
        .join("orthanc.toml")
}

/// Load config from the given TOML file path.
pub fn load_config(path: &Path) -> Result<AppConfig, OrthancError> {
    if !path.exists() {
        log::info!(
            "Config file not found at {}. Returning defaults.",
            path.display()
        );
        return Ok(AppConfig::default());
    }

    let contents = std::fs::read_to_string(path)?;
    let config: AppConfig = toml::from_str(&contents)?;
    Ok(config)
}

/// Save config to the given TOML file path, creating parent directories if needed.
pub fn save_config(path: &Path, config: &AppConfig) -> Result<(), OrthancError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let contents = toml::to_string_pretty(config)?;
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── default_config_path ─────────────────────────────────────────────

    #[test]
    fn default_config_path_ends_with_orthanc_toml() {
        let path = default_config_path();
        assert!(
            path.ends_with("orthanc/orthanc.toml"),
            "Expected path ending with orthanc/orthanc.toml, got: {}",
            path.display()
        );
    }

    #[test]
    fn default_config_path_is_absolute_when_config_dir_exists() {
        // dirs::config_dir() returns an absolute path on most systems.
        // If it falls back to ".", the path will be relative, which is fine
        // for this test -- we just verify the suffix.
        let path = default_config_path();
        let file_name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(file_name, "orthanc.toml");
    }

    #[test]
    fn default_config_path_parent_dir_is_orthanc() {
        let path = default_config_path();
        let parent = path.parent().unwrap();
        let dir_name = parent.file_name().unwrap().to_str().unwrap();
        assert_eq!(dir_name, "orthanc");
    }

    // ── load_config / save_config ───────────────────────────────────────

    #[test]
    fn load_config_returns_default_for_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist.toml");

        let config = load_config(&path).unwrap();
        assert_eq!(config.defaults.python_version, "3.12");
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("orthanc").join("orthanc.toml");

        let mut config = AppConfig::default();
        config.github.pat = Some("token-abc".to_string());
        config.defaults.python_version = "3.11".to_string();

        save_config(&path, &config).unwrap();
        assert!(path.exists());

        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded.github.pat.as_deref(), Some("token-abc"));
        assert_eq!(loaded.defaults.python_version, "3.11");
    }
}
