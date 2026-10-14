#[cfg(test)]
mod tests {
    use crate::config::loader::{load_config, save_config};
    use crate::models::config::AppConfig;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_default_config_serialization_roundtrip() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.github.api_base_url, config.github.api_base_url);
        assert_eq!(
            parsed.defaults.python_version,
            config.defaults.python_version
        );
    }

    #[test]
    fn test_config_with_all_fields() {
        let mut config = AppConfig::default();
        config.github.pat = Some("ghp_test_token_12345".to_string());
        config.github.default_org = Some("TestOrg".to_string());
        config.defaults.python_version = "3.11".to_string();
        config.defaults.seed = true;
        config
            .defaults
            .global_env_vars
            .insert("MY_VAR".to_string(), "my_value".to_string());
        config.defaults.uv_extra_args = vec!["--no-cache".to_string()];
        config.general.terminal_command = Some("alacritty -e {cmd}".to_string());

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.github.pat.unwrap(), "ghp_test_token_12345");
        assert_eq!(parsed.github.default_org.unwrap(), "TestOrg");
        assert_eq!(parsed.defaults.python_version, "3.11");
        assert!(parsed.defaults.seed);
        assert_eq!(
            parsed.defaults.global_env_vars.get("MY_VAR").unwrap(),
            "my_value"
        );
        assert_eq!(parsed.defaults.uv_extra_args, vec!["--no-cache"]);
        assert_eq!(
            parsed.general.terminal_command.unwrap(),
            "alacritty -e {cmd}"
        );
    }

    #[test]
    fn test_save_and_load_config() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("orthanc").join("orthanc.toml");

        let mut config = AppConfig::default();
        config.github.pat = Some("test-token".to_string());
        config.defaults.python_version = "3.13".to_string();
        config
            .defaults
            .global_env_vars
            .insert("KEY".to_string(), "VALUE".to_string());

        save_config(&config_path, &config).unwrap();

        // Verify the file exists
        assert!(config_path.exists());

        // Load it back
        let loaded = load_config(&config_path).unwrap();
        assert_eq!(loaded.github.pat.unwrap(), "test-token");
        assert_eq!(loaded.defaults.python_version, "3.13");
        assert_eq!(loaded.defaults.global_env_vars.get("KEY").unwrap(), "VALUE");
    }

    #[test]
    fn test_load_nonexistent_config_returns_default() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("nonexistent.toml");

        let config = load_config(&config_path).unwrap();
        assert_eq!(config.defaults.python_version, "3.12");
    }

    #[test]
    fn test_config_with_environments() {
        let mut config = AppConfig::default();
        config
            .environments
            .push(crate::models::config::PersistedEnvConfig {
                id: "env-1".to_string(),
                name: "my-env".to_string(),
                repo_path: "/home/user/projects/myrepo".to_string(),
                venv_path: "/home/user/projects/myrepo/.venv".to_string(),
                python_version: "3.12".to_string(),
                env_vars: {
                    let mut m = HashMap::new();
                    m.insert("DEBUG".to_string(), "1".to_string());
                    m
                },
            });

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.environments.len(), 1);
        assert_eq!(parsed.environments[0].name, "my-env");
        assert_eq!(parsed.environments[0].env_vars.get("DEBUG").unwrap(), "1");
    }

    #[test]
    fn test_config_missing_optional_fields() {
        let toml_str = r#"
[general]
config_version = 1

[github]
api_base_url = "https://api.github.com"

[defaults]
python_version = "3.12"

[notifications]
enabled = true
on_pipeline_complete = true
on_staleness_detected = true
sound = true
"#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.github.pat.is_none());
        assert!(config.general.terminal_command.is_none());
        assert!(config.github.default_org.is_none());
        assert!(config.defaults.global_env_vars.is_empty());
        assert!(config.defaults.uv_extra_args.is_empty());
        assert!(config.environments.is_empty());
    }

    #[test]
    fn test_config_partial_with_serde_defaults() {
        // Minimal config that relies on serde defaults
        let toml_str = r#"
[general]
config_version = 1

[github]

[defaults]

[notifications]
"#;

        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.github.api_base_url, "https://api.github.com");
        assert_eq!(config.defaults.python_version, "3.12");
        assert!(!config.defaults.seed);
    }
}
