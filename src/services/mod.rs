pub mod git;
pub mod github_client;
pub mod pycharm;
pub mod staleness;
pub mod terminal;
pub mod updater;
pub mod uv;

/// Whether an executable named `cmd` (with extension on Windows) is on PATH.
pub(crate) fn on_path(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(cmd).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod github_client_tests;
#[cfg(test)]
mod staleness_tests;
