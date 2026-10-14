use std::path::{Path, PathBuf};

use crate::error::OrthancError;
use crate::models::environment::PythonEnvironment;
use crate::services::terminal::get_python_bin;

/// Open PyCharm on the environment's repo, pointing the project interpreter at
/// the environment's venv.
///
/// Returns `Ok(Some(warning))` when PyCharm was launched but the interpreter
/// could not be configured automatically.
pub fn open_pycharm(
    env: &PythonEnvironment,
    custom_command: Option<&str>,
) -> Result<Option<String>, OrthancError> {
    let launcher = find_launcher(custom_command).ok_or_else(|| {
        OrthancError::Other(
            "PyCharm not found. Set general.pycharm_command in the config.".to_string(),
        )
    })?;

    let warning = configure_interpreter(env).err();
    if let Some(w) = &warning {
        log::warn!(
            "PyCharm interpreter not configured for '{}': {}",
            env.name,
            w
        );
    }

    let mut cmd = launcher;
    cmd.arg(&env.repo_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    no_console_window(&mut cmd);
    cmd.spawn()
        .map_err(|e| OrthancError::Other(format!("Failed to launch PyCharm: {}", e)))?;

    log::info!(
        "Opened PyCharm for '{}' at {}",
        env.name,
        env.repo_path.display()
    );
    Ok(warning)
}

/// Register the venv as a PyCharm SDK and set it as the project interpreter.
/// The error string is user-facing.
fn configure_interpreter(env: &PythonEnvironment) -> Result<(), String> {
    let python = get_python_bin(&env.venv_path);
    let manual = format!("set the interpreter manually to {}", python.display());

    let config_dir = dirs::config_dir()
        .and_then(|d| latest_pycharm_config_dir(&d.join("JetBrains")))
        .ok_or_else(|| format!("PyCharm config folder not found; {}", manual))?;
    let table = config_dir.join("options").join("jdk.table.xml");
    let home = dirs::home_dir().unwrap_or_default();

    let sdk_name = if pycharm_running() {
        // PyCharm rewrites jdk.table.xml on exit, so only reuse an existing SDK.
        let content = std::fs::read_to_string(&table).unwrap_or_default();
        find_sdk(&content, &python, &home)
            .ok_or_else(|| format!("PyCharm is running; {}", manual))?
    } else {
        ensure_sdk(&table, &format!("Orthanc ({})", env.name), &python, &home)
            .map_err(|e| format!("{}; {}", e, manual))?
    };

    set_project_sdk(&env.repo_path.join(".idea").join("misc.xml"), &sdk_name)
        .map_err(|e| format!("{}; {}", e, manual))
}

// ── Launcher ────────────────────────────────────────────────────────────

fn find_launcher(custom_command: Option<&str>) -> Option<std::process::Command> {
    if let Some(c) = custom_command.filter(|c| !c.trim().is_empty()) {
        return Some(std::process::Command::new(c));
    }

    // JetBrains Toolbox shell scripts (Linux/macOS: ~/.local/share/JetBrains/Toolbox/scripts,
    // Windows: %LOCALAPPDATA%\JetBrains\Toolbox\scripts).
    if let Some(data) = dirs::data_local_dir() {
        for name in ["pycharm", "pycharm.cmd"] {
            let script = data
                .join("JetBrains")
                .join("Toolbox")
                .join("scripts")
                .join(name);
            if script.exists() {
                return Some(std::process::Command::new(script));
            }
        }
    }

    for name in ["pycharm", "pycharm.sh", "charm", "pycharm64.exe"] {
        if super::on_path(name) {
            return Some(std::process::Command::new(name));
        }
    }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        let apps = [PathBuf::from("/Applications"), home.join("Applications")];
        for app in [
            "PyCharm.app",
            "PyCharm Professional Edition.app",
            "PyCharm CE.app",
        ] {
            if let Some(path) = apps.iter().map(|d| d.join(app)).find(|p| p.exists()) {
                let mut c = std::process::Command::new("open");
                c.arg("-na").arg(path).arg("--args");
                return Some(c);
            }
        }
    }

    None
}

// ponytail: matches the process name only; a java-launched PyCharm (old pycharm.sh) isn't detected.
fn pycharm_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("tasklist");
        cmd.args(["/FI", "IMAGENAME eq pycharm64.exe", "/NH"]);
        no_console_window(&mut cmd);
        cmd.output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("pycharm64.exe"))
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("pgrep")
            .args(["-x", "pycharm"])
            .stdout(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Stop Windows from flashing a console window for child processes.
fn no_console_window(_cmd: &mut std::process::Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        _cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Newest `PyCharm<version>` / `PyCharmCE<version>` folder under the JetBrains config dir.
fn latest_pycharm_config_dir(jetbrains: &Path) -> Option<PathBuf> {
    std::fs::read_dir(jetbrains)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let ver = name
                .strip_prefix("PyCharm")?
                .trim_start_matches(|c: char| c.is_alphabetic());
            let parts: Vec<u32> = ver
                .split('.')
                .map(|p| p.parse().ok())
                .collect::<Option<_>>()?;
            Some((parts, e.path()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, p)| p)
}

// ── jdk.table.xml ───────────────────────────────────────────────────────

const EMPTY_TABLE: &str =
    "<application>\n  <component name=\"ProjectJdkTable\">\n  </component>\n</application>\n";

/// Value of the first `<tag value="..." />` in `block`.
fn tag_value<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{} value=\"", tag);
    let start = block.find(&open)? + open.len();
    let len = block[start..].find('"')?;
    Some(&block[start..start + len])
}

fn jdk_blocks(content: &str) -> impl Iterator<Item = &str> {
    content
        .split("<jdk ")
        .skip(1)
        .map(|b| b.split("</jdk>").next().unwrap_or(b))
}

/// Name of an existing SDK whose interpreter is `python`.
fn find_sdk(content: &str, python: &Path, home: &Path) -> Option<String> {
    // PyCharm stores paths with '/' even on Windows.
    let norm = |p: &str| p.replace('\\', "/");
    let python = norm(&python.to_string_lossy());
    let home = norm(&home.to_string_lossy());
    jdk_blocks(content).find_map(|b| {
        let path = norm(&xml_unescape(tag_value(b, "homePath")?)).replace("$USER_HOME$", &home);
        if path == python {
            tag_value(b, "name").map(xml_unescape)
        } else {
            None
        }
    })
}

/// Return the SDK name for `python`, adding an entry to the table if needed.
fn ensure_sdk(table: &Path, base_name: &str, python: &Path, home: &Path) -> Result<String, String> {
    let content = match std::fs::read_to_string(table) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => EMPTY_TABLE.to_string(),
        Err(e) => return Err(format!("cannot read {}: {}", table.display(), e)),
    };
    if let Some(name) = find_sdk(&content, python, home) {
        return Ok(name);
    }

    // SDK names must be unique.
    let taken: Vec<String> = jdk_blocks(&content)
        .filter_map(|b| tag_value(b, "name").map(xml_unescape))
        .collect();
    let mut name = base_name.to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{} {}", base_name, n);
        n += 1;
    }

    let component = content
        .find("name=\"ProjectJdkTable\"")
        .ok_or_else(|| format!("unrecognised format in {}", table.display()))?;
    let close = content[component..]
        .find("</component>")
        .map(|i| component + i)
        .ok_or_else(|| format!("unrecognised format in {}", table.display()))?;
    // Insert at the start of the closing tag's line to keep indentation tidy.
    let at = content[..close].rfind('\n').map(|i| i + 1).unwrap_or(close);

    let entry = format!(
        r#"    <jdk version="2">
      <name value="{name}" />
      <type value="Python SDK" />
      <homePath value="{path}" />
      <roots>
        <classPath>
          <root type="composite" />
        </classPath>
        <sourcePath>
          <root type="composite" />
        </sourcePath>
      </roots>
      <additional>
        <setting name="FLAVOR_ID" value="VirtualEnvSdkFlavor" />
        <setting name="FLAVOR_DATA" value="{{}}" />
      </additional>
    </jdk>
"#,
        name = xml_escape(&name),
        path = xml_escape(&python.to_string_lossy()),
    );

    let mut updated = content.clone();
    updated.insert_str(at, &entry);
    if let Some(dir) = table.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(table, updated)
        .map_err(|e| format!("cannot write {}: {}", table.display(), e))?;
    Ok(name)
}

// ── .idea/misc.xml ──────────────────────────────────────────────────────

/// Create or patch `misc.xml` so the project uses the named Python SDK.
fn set_project_sdk(misc: &Path, sdk_name: &str) -> Result<(), String> {
    let line = format!(
        r#"<component name="ProjectRootManager" version="2" project-jdk-name="{}" project-jdk-type="Python SDK" />"#,
        xml_escape(sdk_name)
    );

    let updated = match std::fs::read_to_string(misc) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<project version=\"4\">\n  {}\n</project>\n",
            line
        ),
        Err(e) => return Err(format!("cannot read {}: {}", misc.display(), e)),
        Ok(content) => {
            let bad = || format!("unrecognised format in {}", misc.display());
            if let Some(start) = content.find("<component name=\"ProjectRootManager\"") {
                let gt = start + content[start..].find('>').ok_or_else(bad)?;
                let end = if content[..gt].ends_with('/') {
                    gt + 1
                } else {
                    let close = content[gt..].find("</component>").ok_or_else(bad)?;
                    gt + close + "</component>".len()
                };
                format!("{}{}{}", &content[..start], line, &content[end..])
            } else {
                let close = content.rfind("</project>").ok_or_else(bad)?;
                format!("{}  {}\n{}", &content[..close], line, &content[close..])
            }
        }
    };

    if let Some(dir) = misc.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(misc, updated).map_err(|e| format!("cannot write {}: {}", misc.display(), e))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/u";

    #[test]
    fn ensure_sdk_creates_table_and_reuses_entry() {
        let dir = tempfile::tempdir().unwrap();
        let table = dir.path().join("options").join("jdk.table.xml");
        let py = Path::new("/venvs/a & b/bin/python");

        let name = ensure_sdk(&table, "Orthanc (a)", py, Path::new(HOME)).unwrap();
        assert_eq!(name, "Orthanc (a)");
        let content = std::fs::read_to_string(&table).unwrap();
        assert!(content.contains(r#"<homePath value="/venvs/a &amp; b/bin/python" />"#));
        assert!(content.trim_end().ends_with("</component>\n</application>"));

        // Second call finds the existing SDK without writing a duplicate.
        assert_eq!(
            ensure_sdk(&table, "other", py, Path::new(HOME)).unwrap(),
            "Orthanc (a)"
        );
        assert_eq!(std::fs::read_to_string(&table).unwrap(), content);
    }

    #[test]
    fn ensure_sdk_matches_user_home_macro_and_dedupes_names() {
        let dir = tempfile::tempdir().unwrap();
        let table = dir.path().join("jdk.table.xml");
        std::fs::write(
            &table,
            r#"<application>
  <component name="ProjectJdkTable">
    <jdk version="2">
      <name value="uv (myproject)" />
      <homePath value="$USER_HOME$/myproject/.venv/bin/python" />
    </jdk>
    <jdk version="2">
      <name value="Orthanc (x)" />
      <homePath value="/elsewhere/bin/python" />
    </jdk>
  </component>
</application>
"#,
        )
        .unwrap();

        let existing = Path::new("/home/u/myproject/.venv/bin/python");
        assert_eq!(
            ensure_sdk(&table, "n", existing, Path::new(HOME)).unwrap(),
            "uv (myproject)"
        );

        let new = Path::new("/venvs/x/bin/python");
        assert_eq!(
            ensure_sdk(&table, "Orthanc (x)", new, Path::new(HOME)).unwrap(),
            "Orthanc (x) 2"
        );
        let content = std::fs::read_to_string(&table).unwrap();
        assert_eq!(content.matches("<jdk ").count(), 3);
        assert!(content.contains("uv (myproject)"));
    }

    #[test]
    fn find_sdk_ignores_path_separator_style() {
        let table = r#"<jdk version="2"><name value="w" /><homePath value="$USER_HOME$/proj/.venv/Scripts/python.exe" /></jdk>"#;
        let py = Path::new(r"C:\Users\u\proj\.venv\Scripts\python.exe");
        assert_eq!(
            find_sdk(table, py, Path::new(r"C:\Users\u")).as_deref(),
            Some("w")
        );
    }

    #[test]
    fn ensure_sdk_rejects_unknown_format() {
        let dir = tempfile::tempdir().unwrap();
        let table = dir.path().join("jdk.table.xml");
        std::fs::write(&table, "garbage").unwrap();
        assert!(ensure_sdk(&table, "n", Path::new("/p"), Path::new(HOME)).is_err());
        assert_eq!(std::fs::read_to_string(&table).unwrap(), "garbage");
    }

    #[test]
    fn set_project_sdk_creates_misc() {
        let dir = tempfile::tempdir().unwrap();
        let misc = dir.path().join(".idea").join("misc.xml");
        set_project_sdk(&misc, "Orthanc (a)").unwrap();
        let content = std::fs::read_to_string(&misc).unwrap();
        assert!(content.contains(r#"project-jdk-name="Orthanc (a)" project-jdk-type="Python SDK""#));
        assert!(content.ends_with("</project>\n"));
    }

    #[test]
    fn set_project_sdk_replaces_existing_component() {
        let dir = tempfile::tempdir().unwrap();
        let misc = dir.path().join("misc.xml");
        std::fs::write(
            &misc,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="Black">
    <option name="sdkName" value="old" />
  </component>
  <component name="ProjectRootManager" version="2" project-jdk-name="old" project-jdk-type="Python SDK" />
</project>
"#,
        )
        .unwrap();
        set_project_sdk(&misc, "new").unwrap();
        let content = std::fs::read_to_string(&misc).unwrap();
        assert!(content.contains(r#"project-jdk-name="new""#));
        assert!(!content.contains(r#"project-jdk-name="old""#));
        assert!(content.contains(r#"<component name="Black">"#));
        assert_eq!(content.matches("ProjectRootManager").count(), 1);
    }

    #[test]
    fn set_project_sdk_inserts_when_component_missing() {
        let dir = tempfile::tempdir().unwrap();
        let misc = dir.path().join("misc.xml");
        std::fs::write(&misc, "<project version=\"4\">\n</project>\n").unwrap();
        set_project_sdk(&misc, "new").unwrap();
        let content = std::fs::read_to_string(&misc).unwrap();
        assert!(content.contains("ProjectRootManager"));
        assert!(content.ends_with("</project>\n"));
    }

    #[test]
    fn latest_config_dir_picks_newest_version() {
        let dir = tempfile::tempdir().unwrap();
        for d in [
            "PyCharm2025.3",
            "PyCharm2026.2",
            "PyCharmCE2026.1",
            "PyCharm2026.10x",
            "IntelliJIdea2027.1",
        ] {
            std::fs::create_dir(dir.path().join(d)).unwrap();
        }
        let got = latest_pycharm_config_dir(dir.path()).unwrap();
        assert_eq!(got.file_name().unwrap(), "PyCharm2026.2");
    }
}
