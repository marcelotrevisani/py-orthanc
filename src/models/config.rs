use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub github: GitHubConfig,
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub uv: UvConfig,
    #[serde(default)]
    pub environments: Vec<PersistedEnvConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub terminal_command: Option<String>,
    #[serde(default)]
    pub pycharm_command: Option<String>,
    #[serde(default)]
    pub theme: ThemeChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub pat: Option<String>,
    #[serde(default = "default_api_base_url")]
    pub api_base_url: String,
    pub default_org: Option<String>,
}

fn default_api_base_url() -> String {
    "https://api.github.com".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default = "default_python_version")]
    pub python_version: String,
    #[serde(default)]
    pub seed: bool,
    /// Default directory where virtual environments are created.
    /// When empty/None, defaults to `<repo>/.venv`.
    #[serde(default)]
    pub default_venv_dir: Option<String>,
    #[serde(default)]
    pub global_env_vars: HashMap<String, String>,
    #[serde(default)]
    pub uv_extra_args: Vec<String>,
}

fn default_python_version() -> String {
    "3.12".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UvConfig {
    /// Private package index URL (PEP 503 simple API). Unset: public PyPI only.
    #[serde(default)]
    pub default_index: Option<String>,
    /// Keep public PyPI as a fallback behind the private index. Off: the
    /// private index replaces PyPI (avoids dependency-confusion attacks).
    #[serde(default)]
    pub use_public_index: bool,
    /// Logical name for the index (used to build UV_INDEX_{NAME}_USERNAME/PASSWORD).
    /// Defaults to "private".
    #[serde(default = "default_index_name")]
    pub index_name: String,
    /// Username for private index authentication.
    #[serde(default)]
    pub username: Option<String>,
    /// Password / token for private index authentication.
    #[serde(default)]
    pub password: Option<String>,
}

fn default_index_name() -> String {
    "private".to_string()
}

impl Default for UvConfig {
    fn default() -> Self {
        Self {
            default_index: None,
            use_public_index: false,
            index_name: default_index_name(),
            username: None,
            password: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedEnvConfig {
    pub id: String,
    pub name: String,
    pub repo_path: String,
    pub venv_path: String,
    pub python_version: String,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                terminal_command: None,
                pycharm_command: None,
                theme: ThemeChoice::Dark,
            },
            github: GitHubConfig {
                pat: None,
                api_base_url: default_api_base_url(),
                default_org: None,
            },
            defaults: DefaultsConfig {
                python_version: default_python_version(),
                seed: false,
                default_venv_dir: None,
                global_env_vars: HashMap::new(),
                uv_extra_args: Vec::new(),
            },
            uv: UvConfig::default(),
            environments: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_config_serde_roundtrip() {
        let uv = UvConfig {
            default_index: Some("https://pypi.example.com/simple".to_string()),
            use_public_index: true,
            index_name: "corp".to_string(),
            username: Some("deploy-bot".to_string()),
            password: Some("secret-token".to_string()),
        };
        let json = serde_json::to_string(&uv).unwrap();
        let de: UvConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            de.default_index.as_deref(),
            Some("https://pypi.example.com/simple")
        );
        assert_eq!(de.index_name, "corp");
        assert!(de.use_public_index);
        assert_eq!(de.username.as_deref(), Some("deploy-bot"));
        assert_eq!(de.password.as_deref(), Some("secret-token"));
    }

    #[test]
    fn uv_config_deserialize_empty_defaults() {
        let json = "{}";
        let uv: UvConfig = serde_json::from_str(json).unwrap();
        assert!(uv.default_index.is_none());
        assert!(!uv.use_public_index);
        assert_eq!(uv.index_name, "private");
        assert!(uv.username.is_none());
        assert!(uv.password.is_none());
    }

    // ── PersistedEnvConfig serde roundtrip ──────────────────────────────

    #[test]
    fn persisted_env_config_json_roundtrip() {
        let env = PersistedEnvConfig {
            id: "env-abc".to_string(),
            name: "my-project".to_string(),
            repo_path: "/home/user/project".to_string(),
            venv_path: "/home/user/project/.venv".to_string(),
            python_version: "3.11".to_string(),
            env_vars: {
                let mut m = HashMap::new();
                m.insert(
                    "DATABASE_URL".to_string(),
                    "postgres://localhost/db".to_string(),
                );
                m
            },
        };

        let json = serde_json::to_string(&env).unwrap();
        let de: PersistedEnvConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(de.id, "env-abc");
        assert_eq!(de.name, "my-project");
        assert_eq!(de.repo_path, "/home/user/project");
        assert_eq!(de.venv_path, "/home/user/project/.venv");
        assert_eq!(de.python_version, "3.11");
        assert_eq!(
            de.env_vars.get("DATABASE_URL").unwrap(),
            "postgres://localhost/db"
        );
    }

    #[test]
    fn persisted_env_config_toml_roundtrip() {
        let env = PersistedEnvConfig {
            id: "env-1".to_string(),
            name: "test".to_string(),
            repo_path: "/tmp/repo".to_string(),
            venv_path: "/tmp/repo/.venv".to_string(),
            python_version: "3.12".to_string(),
            env_vars: HashMap::new(),
        };

        let toml_str = toml::to_string(&env).unwrap();
        let de: PersistedEnvConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(de.id, "env-1");
        assert_eq!(de.name, "test");
        assert!(de.env_vars.is_empty());
    }

    #[test]
    fn persisted_env_config_defaults_for_optional_fields() {
        // JSON without env_vars should deserialize with defaults
        let json = r#"{
            "id": "x",
            "name": "y",
            "repo_path": "/a",
            "venv_path": "/b",
            "python_version": "3.10"
        }"#;
        let de: PersistedEnvConfig = serde_json::from_str(json).unwrap();

        assert!(de.env_vars.is_empty());
    }
}
