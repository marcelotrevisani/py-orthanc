use std::collections::HashMap;

use crate::error::OrthancError;
use crate::models::environment::PythonEnvironment;

/// Spawn a terminal session with the virtual environment activated and
/// environment variables exported.
pub fn spawn_terminal(
    env: &PythonEnvironment,
    global_env_vars: &HashMap<String, String>,
    custom_terminal: Option<&str>,
) -> Result<(), OrthancError> {
    let activate_script = get_activate_script(&env.venv_path);
    if !activate_script.exists() {
        return Err(OrthancError::Other(format!(
            "Activate script not found at {}",
            activate_script.display()
        )));
    }

    #[cfg(target_os = "windows")]
    {
        spawn_terminal_windows(env, global_env_vars, custom_terminal, &activate_script)
    }

    #[cfg(target_os = "macos")]
    {
        spawn_terminal_unix(
            env,
            global_env_vars,
            custom_terminal,
            &activate_script,
            true,
        )
    }

    #[cfg(target_os = "linux")]
    {
        spawn_terminal_unix(
            env,
            global_env_vars,
            custom_terminal,
            &activate_script,
            false,
        )
    }
}

/// Get the path to the activate script for the current platform.
fn get_activate_script(venv_path: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        venv_path.join("Scripts").join("activate.bat")
    }

    #[cfg(not(target_os = "windows"))]
    {
        venv_path.join("bin").join("activate")
    }
}

/// Get the path to the python binary for the current platform.
pub fn get_python_bin(venv_path: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        venv_path.join("Scripts").join("python.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        venv_path.join("bin").join("python")
    }
}

// ── Windows implementation ──────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn spawn_terminal_windows(
    env: &PythonEnvironment,
    global_env_vars: &HashMap<String, String>,
    custom_terminal: Option<&str>,
    activate_script: &std::path::Path,
) -> Result<(), OrthancError> {
    // Build a cmd.exe command that sets env vars and activates the venv
    let mut parts: Vec<String> = Vec::new();

    for (key, value) in global_env_vars {
        parts.push(format!("set \"{}={}\"", key, value));
    }
    for (key, value) in &env.env_vars {
        parts.push(format!("set \"{}={}\"", key, value));
    }
    parts.push(format!("call \"{}\"", activate_script.display()));

    let inner_command = parts.join(" && ");

    let mut cmd = match custom_terminal {
        Some(terminal_cmd) if terminal_cmd.contains("{cmd}") => {
            let full = terminal_cmd.replace("{cmd}", &inner_command);
            let mut c = std::process::Command::new("cmd.exe");
            c.arg("/C").arg(&full);
            c
        }
        Some(terminal_cmd) => {
            let mut c = std::process::Command::new(terminal_cmd);
            c.arg("/K").arg(&inner_command);
            c
        }
        None => {
            // Try Windows Terminal first, then fall back to cmd.exe
            if super::on_path("wt.exe") {
                let mut c = std::process::Command::new("wt.exe");
                c.arg("cmd.exe").arg("/K").arg(&inner_command);
                c
            } else {
                let mut c = std::process::Command::new("cmd.exe");
                c.arg("/K").arg(&inner_command);
                c
            }
        }
    };

    cmd.current_dir(&env.repo_path);
    cmd.spawn()
        .map_err(|e| OrthancError::Other(format!("Failed to spawn terminal: {}", e)))?;

    log::info!(
        "Spawned terminal for environment '{}' at {}",
        env.name,
        env.repo_path.display()
    );

    Ok(())
}

// ── Unix implementation (Linux & macOS) ─────────────────────────────────

#[cfg(not(target_os = "windows"))]
fn spawn_terminal_unix(
    env: &PythonEnvironment,
    global_env_vars: &HashMap<String, String>,
    custom_terminal: Option<&str>,
    activate_script: &std::path::Path,
    is_macos: bool,
) -> Result<(), OrthancError> {
    let mut shell_parts: Vec<String> = Vec::new();

    for (key, value) in global_env_vars {
        shell_parts.push(format!("export {}={}", key, shell_escape(value)));
    }
    for (key, value) in &env.env_vars {
        shell_parts.push(format!("export {}={}", key, shell_escape(value)));
    }

    shell_parts.push(format!(
        "source {}",
        shell_escape(&activate_script.to_string_lossy())
    ));
    shell_parts.push("exec $SHELL".to_string());

    let inner_command = shell_parts.join(" && ");

    // Use the user's default shell (zsh on macOS, bash on Linux typically)
    let default_shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if is_macos {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    });
    let shell_name = std::path::Path::new(&default_shell)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "bash".to_string());

    let shell_command = format!("{} -c {}", shell_name, shell_escape(&inner_command));

    let mut cmd = std::process::Command::new(&shell_name);

    match custom_terminal {
        Some(terminal_cmd) if terminal_cmd.contains("{cmd}") => {
            let full_command = terminal_cmd.replace("{cmd}", &shell_command);
            cmd = std::process::Command::new(&shell_name);
            cmd.arg("-c").arg(&full_command);
        }
        Some(terminal_cmd) => {
            cmd = std::process::Command::new(terminal_cmd);
            cmd.arg("-e").arg(&shell_name).arg("-c").arg(&inner_command);
        }
        None => {
            if is_macos {
                // Terminal.app ignores `open --args`, so ask it to run the
                // command via AppleScript (in a new window, at the repo).
                let script = format!(
                    "cd {} && {}",
                    shell_escape(&env.repo_path.to_string_lossy()),
                    inner_command
                );
                cmd = std::process::Command::new("osascript");
                cmd.arg("-e").arg(format!(
                    "tell application \"Terminal\" to do script \"{}\"",
                    applescript_escape(&script)
                ));
                cmd.arg("-e")
                    .arg("tell application \"Terminal\" to activate");
            } else {
                // Try common Linux terminal emulators
                let terminals = [
                    "x-terminal-emulator",
                    "gnome-terminal",
                    "konsole",
                    "xfce4-terminal",
                    "xterm",
                ];

                let mut found = false;
                for terminal in &terminals {
                    if super::on_path(terminal) {
                        if *terminal == "gnome-terminal" {
                            cmd = std::process::Command::new(terminal);
                            cmd.arg("--").arg(&shell_name).arg("-c").arg(&inner_command);
                        } else {
                            cmd = std::process::Command::new(terminal);
                            cmd.arg("-e").arg(&shell_name).arg("-c").arg(&inner_command);
                        }
                        found = true;
                        break;
                    }
                }

                if !found {
                    return Err(OrthancError::Other(
                        "No terminal emulator found. Please configure one in settings.".to_string(),
                    ));
                }
            }
        }
    }

    cmd.current_dir(&env.repo_path);
    cmd.spawn()
        .map_err(|e| OrthancError::Other(format!("Failed to spawn terminal: {}", e)))?;

    log::info!(
        "Spawned terminal for environment '{}' at {}",
        env.name,
        env.repo_path.display()
    );

    Ok(())
}

/// Escape a string for an AppleScript double-quoted literal.
#[cfg(not(target_os = "windows"))]
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Escape a string for safe use in a shell command (Unix).
#[cfg(not(target_os = "windows"))]
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── shell_escape ────────────────────────────────────────────────────

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_plain_string() {
        assert_eq!(shell_escape("hello"), "'hello'");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_string_with_spaces() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_string_with_single_quote() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_string_with_multiple_single_quotes() {
        assert_eq!(shell_escape("a'b'c"), "'a'\\''b'\\''c'");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_string_with_double_quotes() {
        assert_eq!(shell_escape(r#"say "hi""#), r#"'say "hi"'"#);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_string_with_special_chars() {
        assert_eq!(shell_escape("$HOME"), "'$HOME'");
        assert_eq!(shell_escape("foo;bar"), "'foo;bar'");
        assert_eq!(shell_escape("a & b"), "'a & b'");
        assert_eq!(shell_escape("$(whoami)"), "'$(whoami)'");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_empty_string() {
        assert_eq!(shell_escape(""), "''");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_string_with_newline() {
        assert_eq!(shell_escape("line1\nline2"), "'line1\nline2'");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_string_with_backslash() {
        assert_eq!(shell_escape("back\\slash"), "'back\\slash'");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn shell_escape_path_like_string() {
        assert_eq!(
            shell_escape("/home/user/my project/.venv/bin/activate"),
            "'/home/user/my project/.venv/bin/activate'"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn applescript_escape_quotes_and_backslashes() {
        assert_eq!(
            applescript_escape(r#"cd '/a b' && echo "hi\there""#),
            r#"cd '/a b' && echo \"hi\\there\""#
        );
    }

    // ── get_python_bin ──────────────────────────────────────────────────

    #[test]
    fn get_python_bin_returns_correct_path() {
        let venv = std::path::PathBuf::from("/some/venv");
        let bin = get_python_bin(&venv);
        #[cfg(target_os = "windows")]
        assert!(bin.ends_with("Scripts/python.exe") || bin.ends_with("Scripts\\python.exe"));
        #[cfg(not(target_os = "windows"))]
        assert!(bin.ends_with("bin/python"));
    }

    // ── get_activate_script ─────────────────────────────────────────────

    #[test]
    fn get_activate_script_returns_correct_path() {
        let venv = std::path::PathBuf::from("/some/venv");
        let script = get_activate_script(&venv);
        #[cfg(target_os = "windows")]
        assert!(
            script.ends_with("Scripts/activate.bat") || script.ends_with("Scripts\\activate.bat")
        );
        #[cfg(not(target_os = "windows"))]
        assert!(script.ends_with("bin/activate"));
    }
}
