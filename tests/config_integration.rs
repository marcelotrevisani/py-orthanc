use std::collections::HashMap;
use tempfile::TempDir;

// Integration tests for config persistence.
// These test the full flow of saving and loading config through the loader module.

#[test]
fn test_full_config_lifecycle() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("orthanc").join("orthanc.toml");

    // Step 1: Create config with all fields populated
    let mut config = orthanc_lib::models::config::AppConfig::default();
    config.github.pat = Some("ghp_abcdefg123456".to_string());
    config.github.default_org = Some("MyOrg".to_string());
    config.defaults.python_version = "3.11.7".to_string();
    config.defaults.seed = true;
    config.defaults.global_env_vars = {
        let mut m = HashMap::new();
        m.insert("PYTHONHASHSEED".to_string(), "0".to_string());
        m.insert("OMP_NUM_THREADS".to_string(), "4".to_string());
        m
    };
    config.general.terminal_command = Some("wezterm start -- {cmd}".to_string());

    // Add environments
    config
        .environments
        .push(orthanc_lib::models::config::PersistedEnvConfig {
            id: "env-001".to_string(),
            name: "production-env".to_string(),
            repo_path: "/home/user/project".to_string(),
            venv_path: "/home/user/project/.venv".to_string(),
            python_version: "3.11.7".to_string(),
            env_vars: {
                let mut m = HashMap::new();
                m.insert(
                    "DATABASE_URL".to_string(),
                    "postgres://localhost/db".to_string(),
                );
                m
            },
        });

    // Step 2: Save
    orthanc_lib::config::loader::save_config(&config_path, &config).unwrap();
    assert!(config_path.exists());

    // Step 3: Load
    let loaded = orthanc_lib::config::loader::load_config(&config_path).unwrap();

    // Step 4: Verify all fields
    assert_eq!(loaded.github.pat.unwrap(), "ghp_abcdefg123456");
    assert_eq!(loaded.github.default_org.unwrap(), "MyOrg");
    assert_eq!(loaded.defaults.python_version, "3.11.7");
    assert!(loaded.defaults.seed);
    assert_eq!(loaded.defaults.global_env_vars.len(), 2);
    assert_eq!(
        loaded
            .defaults
            .global_env_vars
            .get("PYTHONHASHSEED")
            .unwrap(),
        "0"
    );
    assert_eq!(
        loaded.general.terminal_command.unwrap(),
        "wezterm start -- {cmd}"
    );
    assert_eq!(loaded.environments.len(), 1);
    assert_eq!(loaded.environments[0].name, "production-env");
}

#[test]
fn test_config_survives_multiple_saves() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("orthanc.toml");

    let mut config = orthanc_lib::models::config::AppConfig::default();
    config.defaults.python_version = "3.10".to_string();
    orthanc_lib::config::loader::save_config(&config_path, &config).unwrap();

    // Modify and save again
    config.defaults.python_version = "3.11".to_string();
    config.github.pat = Some("token".to_string());
    orthanc_lib::config::loader::save_config(&config_path, &config).unwrap();

    // Modify and save a third time
    config.defaults.python_version = "3.12".to_string();
    orthanc_lib::config::loader::save_config(&config_path, &config).unwrap();

    // Load and verify final state
    let loaded = orthanc_lib::config::loader::load_config(&config_path).unwrap();
    assert_eq!(loaded.defaults.python_version, "3.12");
    assert_eq!(loaded.github.pat.unwrap(), "token");
}
