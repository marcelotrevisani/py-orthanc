use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use iced::widget::{column, container, row};
use iced::{window, Element, Length, Size, Subscription, Task, Theme};
use uuid::Uuid;

use orthanc_lib::config::loader;
use orthanc_lib::models::config::UvConfig;
use orthanc_lib::models::config::{AppConfig, DefaultsConfig, ThemeChoice};
use orthanc_lib::models::environment::{EnvStatus, PythonEnvironment, StalenessInfo};
use orthanc_lib::models::github::{PullRequest, WorkflowRun};
use orthanc_lib::services::git::GitStatus;
use orthanc_lib::services::github_client::GitHubClient;
use orthanc_lib::services::{git, pycharm, staleness, terminal, updater, uv};

use crate::icon;
use crate::tray;
use crate::ui;

// ── Pages ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Environments,
    PullRequests,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsTab {
    Info,
    EnvVars,
    Dependencies,
    Pipelines,
}

/// Where the Dependencies tab installs a package from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstallSource {
    #[default]
    PyPi,
    Local,
    GitHub,
}

/// State of the Dependencies tab's install form.
#[derive(Debug, Clone, Default)]
pub struct InstallForm {
    pub source: InstallSource,
    pub name: String,
    pub path: String,
    pub editable: bool,
    pub git_url: String,
    pub git_ref: String,
}

impl InstallForm {
    /// The `uv pip install` spec and editable flag, or `None` if the form is incomplete.
    pub fn request(&self) -> Option<(String, bool)> {
        let non_empty = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
        match self.source {
            InstallSource::PyPi => non_empty(&self.name).map(|n| (n, false)),
            InstallSource::Local => non_empty(&self.path).map(|p| (p, self.editable)),
            InstallSource::GitHub => github_spec(&self.git_url, &self.git_ref).map(|s| (s, false)),
        }
    }

    /// Clear what was typed for the current source, keeping the source and editable choice.
    fn clear_input(&mut self) {
        match self.source {
            InstallSource::PyPi => self.name.clear(),
            InstallSource::Local => self.path.clear(),
            InstallSource::GitHub => {
                self.git_url.clear();
                self.git_ref.clear();
            }
        }
    }
}

/// Turn `org/repo`, `github.com/org/repo`, an https URL or `git@host:org/repo`
/// into a `git+<url>[@ref]` spec for uv.
fn github_spec(url: &str, git_ref: &str) -> Option<String> {
    let url = url.trim().trim_start_matches("git+").trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    let url = if url.contains("://") {
        url.to_string()
    } else if let Some(rest) = url.strip_prefix("git@") {
        // scp-style SSH: git@github.com:org/repo -> ssh://git@github.com/org/repo
        format!("ssh://git@{}", rest.replacen(':', "/", 1))
    } else if url.starts_with("github.com/") {
        format!("https://{}", url)
    } else if url.matches('/').count() == 1 {
        format!("https://github.com/{}", url)
    } else {
        return None;
    };
    let git_ref = git_ref.trim();
    Some(if git_ref.is_empty() {
        format!("git+{}", url)
    } else {
        format!("git+{}@{}", url, git_ref)
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum DepSource {
    Direct,
    Dev,
    Transitive,
    Editable,
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub source: DepSource,
}

#[derive(Debug, Clone)]
pub struct DepInfo {
    pub name: String,
    pub version: String,
    pub summary: String,
    pub location: String,
    pub requires: Vec<String>,
    pub required_by: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DepTreeNode {
    pub name: String,
    pub version: String,
    pub children: Vec<DepTreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastVariant {
    Info,
    Success,
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: usize,
    pub message: String,
    pub variant: ToastVariant,
    pub created_at: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct EnvVarRow {
    pub key: String,
    pub value: String,
    pub source: EnvVarSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EnvVarSource {
    Local,
    Global,
    Override,
}

// ── Message ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    // Navigation
    NavigateTo(Page),

    // Environment table
    SelectEnvironment(Option<String>),
    RefreshEnvironments,

    // Create environment
    ShowCreateDialog,
    HideCreateDialog,
    CreateRepoPathChanged(String),
    CreateEnvNameChanged(String),
    CreateVenvPathChanged(String),
    CreatePythonVersionChanged(String),
    PickFolder,
    FolderPicked(Option<String>),
    PickVenvFolder,
    VenvFolderPicked(Option<String>),
    SubmitCreateEnvironment,
    EnvironmentCreated(Result<(PythonEnvironment, Option<String>), String>),

    // Sync
    SyncEnvironment(String),
    EnvironmentSynced(Result<(String, StalenessInfo), String>),

    // Delete
    ShowDeleteConfirm(String),
    HideConfirmDialog,
    ConfirmDelete,
    EnvironmentDeleted(Result<String, String>),

    // Change repo path
    PickRepoPath(String),
    RepoPathPicked(Option<(String, String)>),

    // Terminal
    OpenTerminal(String),

    // PyCharm
    OpenPycharm(String),

    // Staleness
    CheckAllStaleness,
    AllStalenessChecked(Result<Vec<(String, StalenessInfo)>, String>),

    // Environment details tabs
    SwitchDetailsTab(DetailsTab),
    SaveEnvVars,
    EnvVarKeyChanged(usize, String),
    EnvVarValueChanged(usize, String),
    AddEnvVar,
    RemoveEnvVar(usize),
    OverrideGlobalVar(String),

    // Dependencies
    ListDependencies,
    DependenciesLoaded(Result<Vec<Dependency>, String>),
    DepVersionEditing(usize, String), // start editing version at index
    DepVersionSubmit(usize),          // user pressed enter / confirmed
    DepVersionUpdated(Result<String, String>), // result of pip install pkg==ver
    DepInstallNameChanged(String),
    DepInstallSourceChanged(InstallSource),
    DepInstallPathChanged(String),
    DepInstallPickPath,
    DepInstallPathPicked(Option<String>),
    DepInstallEditableToggled(bool),
    DepInstallGitUrlChanged(String),
    DepInstallGitRefChanged(String),
    DepInstallSubmit,
    DepInstalled(Result<String, String>),
    DepUninstall(usize),
    DepUninstalled(Result<String, String>),
    DepClick(usize),
    DepInfoLoaded(Result<DepInfo, String>),
    DepCloseInfo,
    // Dependency tree (right-click context menu)
    EnvContextMenu(String),
    HideContextMenu,
    ShowDepTree(String),
    ShowDependencies(String),

    // Git status
    RefreshGitStatus,
    GitStatusLoaded(Vec<(String, Option<GitStatus>)>),
    DepTreeLoaded(Result<Vec<DepTreeNode>, String>),

    // Windows
    MainWindowOpened(window::Id),
    GraphWindowClosed(window::Id),

    // Pipelines
    PipelinesLoaded(Result<Vec<WorkflowRun>, String>),
    RefreshPipelines,
    RerunWorkflow(u64),
    RerunWorkflowResult(Result<(), String>),

    // Settings
    SettingsPATChanged(String),
    SettingsApiBaseUrlChanged(String),
    SettingsDefaultOrgChanged(String),
    SettingsPythonVersionChanged(String),
    SettingsSeedChanged(bool),
    SettingsTerminalCommandChanged(String),
    SettingsThemeChanged(ThemeChoice),
    SaveSettings,
    SettingsSaved(Result<(), String>),
    // Global env vars in settings
    SettingsGlobalEnvVarKeyChanged(usize, String),
    SettingsGlobalEnvVarValueChanged(usize, String),
    SettingsAddGlobalEnvVar,
    SettingsRemoveGlobalEnvVar(usize),

    // Pull Requests page
    LoadOrgPullRequests,
    OrgPullRequestsLoaded(Result<Vec<PullRequest>, String>),
    PrFilterStateChanged(String),
    PrFilterAuthorChanged(String),
    OpenUrl(String),

    // Toasts
    DismissToast(usize),
    ToastTick,

    SettingsDefaultVenvDirChanged(String),
    SettingsUvExtraArgsChanged(String),

    // UV index settings
    SettingsUvDefaultIndexChanged(String),
    SettingsUvIndexNameChanged(String),
    SettingsUvUsePublicIndex(bool),
    SettingsUvUsernameChanged(String),
    SettingsUvPasswordChanged(String),

    // System tray
    /// Periodic tray poll with no menu action; also starts the tray.
    TrayTick,
    TrayShow,
    TrayQuit,
    TrayOpenTerminal(String),

    // Self-update
    CheckForUpdate,
    UpdateCheckResult(Result<Option<updater::ReleaseInfo>, String>),
    DownloadUpdate(String), // download_url
    UpdateDownloaded(Result<(), String>),

    // Startup
    UvAvailabilityChecked(bool),
}

// ── Application State ────────────────────────────────────────────────────

pub struct Orthanc {
    // Core data
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub environments: Vec<PythonEnvironment>,
    pub github_client: Option<GitHubClient>,

    // Windows
    pub main_window_id: Option<window::Id>,
    pub graph_window_id: Option<window::Id>,

    // UI state
    pub active_page: Page,
    pub selected_env_id: Option<String>,
    pub uv_available: Option<bool>,

    // Create dialog
    pub show_create_dialog: bool,
    pub create_repo_path: String,
    pub create_env_name: String,
    pub create_env_name_touched: bool,
    pub create_venv_path: String,
    pub create_python_version: String,
    pub creating: bool,

    // Confirm dialog
    pub confirm_delete_env_id: Option<String>,

    // Details panel
    pub active_details_tab: DetailsTab,
    pub env_var_rows: Vec<EnvVarRow>,
    pub global_env_vars_cache: HashMap<String, String>,
    pub dependencies: Vec<Dependency>,
    pub loading_dependencies: bool,
    pub dep_editing_index: Option<usize>,
    pub dep_editing_version: String,
    pub dep_install: InstallForm,
    /// Git status per environment id; absent when the repo is not a git repository.
    pub git_status: HashMap<String, GitStatus>,
    pub dep_last_click_index: Option<usize>,
    pub dep_last_click_time: Option<std::time::Instant>,
    pub dep_info: Option<DepInfo>,
    pub loading_dep_info: bool,
    pub context_menu_env_id: Option<String>,
    pub dep_tree: Option<Vec<DepTreeNode>>,
    pub loading_dep_tree: bool,
    pub pipeline_runs: Vec<WorkflowRun>,
    pub loading_pipelines: bool,

    // Pull Requests page
    pub org_pull_requests: Vec<PullRequest>,
    pub loading_org_prs: bool,
    pub pr_filter_state: String,
    pub pr_filter_author: String,

    // Settings form
    /// Draft of the config edited by the Settings page; copied into
    /// `config` on save.
    pub settings: AppConfig,
    pub settings_uv_extra_args: String,
    pub settings_global_env_var_rows: Vec<(String, String)>,

    // Self-update
    pub update_available: Option<updater::ReleaseInfo>,
    pub update_downloading: bool,

    // Toasts
    pub toasts: Vec<Toast>,
    pub next_toast_id: usize,
}

impl Orthanc {
    pub fn new() -> (Self, Task<Message>) {
        let config_path = loader::default_config_path();
        let config = loader::load_config(&config_path).unwrap_or_default();

        let github_client = config.github.pat.as_ref().and_then(|token| {
            GitHubClient::new(token.clone(), config.github.api_base_url.clone())
                .map_err(|e| log::warn!("GitHub client disabled: {}", e))
                .ok()
        });

        // Apply theme from config on startup
        ui::theme::set_light_theme(config.general.theme == ThemeChoice::Light);

        let environments = load_persisted_environments(&config);

        let settings_global_env_var_rows: Vec<(String, String)> = config
            .defaults
            .global_env_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Open the main window (daemon doesn't open one by default)
        let (main_id, open_main) = window::open(window::Settings {
            size: Size::new(1200.0, 800.0),
            min_size: Some(Size::new(800.0, 500.0)),
            icon: icon::window_icon(),
            platform_specific: icon::platform_specific(),
            ..Default::default()
        });

        let app = Self {
            main_window_id: Some(main_id),
            graph_window_id: None,

            settings: config.clone(),
            settings_uv_extra_args: config.defaults.uv_extra_args.join(" "),
            settings_global_env_var_rows,

            update_available: None,
            update_downloading: false,

            config,
            config_path,
            environments,
            github_client,

            active_page: Page::Environments,
            selected_env_id: None,
            uv_available: None,

            show_create_dialog: false,
            create_repo_path: String::new(),
            create_env_name: String::new(),
            create_env_name_touched: false,
            create_venv_path: String::new(),
            create_python_version: String::new(),
            creating: false,

            confirm_delete_env_id: None,

            active_details_tab: DetailsTab::Info,
            env_var_rows: Vec::new(),
            global_env_vars_cache: HashMap::new(),
            dependencies: Vec::new(),
            loading_dependencies: false,
            dep_editing_index: None,
            dep_editing_version: String::new(),
            dep_install: InstallForm::default(),
            git_status: HashMap::new(),
            dep_last_click_index: None,
            dep_last_click_time: None,
            dep_info: None,
            loading_dep_info: false,
            context_menu_env_id: None,
            dep_tree: None,
            loading_dep_tree: false,
            pipeline_runs: Vec::new(),
            loading_pipelines: false,

            org_pull_requests: Vec::new(),
            loading_org_prs: false,
            pr_filter_state: "open".to_string(),
            pr_filter_author: String::new(),

            toasts: Vec::new(),
            next_toast_id: 0,
        };

        // Sync tray with initial environment list
        app.sync_tray_envs();

        let uv_check = Task::perform(uv::is_uv_available(), |result| {
            Message::UvAvailabilityChecked(result.unwrap_or(false))
        });

        let startup = Task::batch([
            open_main.map(Message::MainWindowOpened),
            uv_check,
            Task::done(Message::CheckForUpdate),
            Task::done(Message::RefreshGitStatus),
        ]);

        (app, startup)
    }

    /// Push the current environment list to the system tray.
    fn sync_tray_envs(&self) {
        let entries: Vec<tray::TrayEnvEntry> = self
            .environments
            .iter()
            .map(|env| tray::TrayEnvEntry {
                id: env.id.clone(),
                name: env.name.clone(),
            })
            .collect();
        tray::update_tray_envs(entries);
    }

    pub fn title(&self, _window: window::Id) -> String {
        if Some(_window) == self.graph_window_id {
            "Dependency Graph".to_string()
        } else {
            "Orthanc — Python Environment Manager".to_string()
        }
    }

    pub fn theme(&self, _window: window::Id) -> Theme {
        ui::theme::orthanc_theme()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();

        if !self.toasts.is_empty() {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(1)).map(|_| Message::ToastTick),
            );
        }

        // Keep git branch/status current while the window is visible
        if self.main_window_id.is_some() {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(15))
                    .map(|_| Message::RefreshGitStatus),
            );
        }

        // Always listen for window close events so we can minimize to tray
        subs.push(window::close_events().map(Message::GraphWindowClosed));

        // Poll system tray menu events (~200ms)
        subs.push(
            iced::time::every(std::time::Duration::from_millis(200)).map(|_| {
                match tray::poll_tray_action() {
                    Some(tray::TrayAction::Show) => Message::TrayShow,
                    Some(tray::TrayAction::Quit) => Message::TrayQuit,
                    Some(tray::TrayAction::OpenTerminal(id)) => Message::TrayOpenTerminal(id),
                    None => Message::TrayTick,
                }
            }),
        );

        Subscription::batch(subs)
    }

    // ── Update ───────────────────────────────────────────────────────────

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Windows ──
            Message::MainWindowOpened(_id) => Task::none(),
            Message::GraphWindowClosed(id) => {
                if self.graph_window_id == Some(id) {
                    self.graph_window_id = None;
                    self.dep_tree = None;
                }
                // If main window closed, minimize to tray (don't exit)
                if self.main_window_id == Some(id) {
                    self.main_window_id = None;
                    if !tray::tray_available() {
                        log::info!("Main window closed and no system tray — exiting");
                        return iced::exit();
                    }
                    log::info!("Main window closed — running in system tray");
                }
                Task::none()
            }

            // ── System tray ──
            Message::TrayTick => {
                if tray::init() {
                    self.sync_tray_envs();
                }
                Task::none()
            }
            Message::TrayShow => {
                if let Some(id) = self.main_window_id {
                    // Already open — just focus it
                    return window::gain_focus(id);
                }
                let (id, open) = window::open(window::Settings {
                    size: Size::new(1200.0, 800.0),
                    min_size: Some(Size::new(800.0, 500.0)),
                    icon: icon::window_icon(),
                    platform_specific: icon::platform_specific(),
                    ..Default::default()
                });
                self.main_window_id = Some(id);
                open.discard()
            }
            Message::TrayQuit => {
                log::info!("Quit from system tray");
                iced::exit()
            }
            Message::TrayOpenTerminal(env_id) => {
                // Reuse the same terminal spawning logic as OpenTerminal
                self.update(Message::OpenTerminal(env_id))
            }

            // ── Navigation ──
            Message::NavigateTo(page) => {
                self.active_page = page;
                if page == Page::Settings {
                    self.sync_settings_form();
                }
                if page == Page::PullRequests && self.org_pull_requests.is_empty() {
                    return self.load_org_prs_task();
                }
                Task::none()
            }

            // ── Environment Table ──
            Message::SelectEnvironment(id) => {
                self.selected_env_id = id;
                self.active_details_tab = DetailsTab::Info;
                self.env_var_rows.clear();
                self.pipeline_runs.clear();
                Task::none()
            }
            Message::RefreshEnvironments => Task::batch([
                Task::done(Message::CheckAllStaleness),
                Task::done(Message::RefreshGitStatus),
            ]),
            Message::RefreshGitStatus => {
                let repos: Vec<(String, PathBuf)> = self
                    .environments
                    .iter()
                    .map(|e| (e.id.clone(), e.repo_path.clone()))
                    .collect();
                Task::perform(
                    async move {
                        // git2 is blocking; keep it off the async worker threads.
                        tokio::task::spawn_blocking(move || {
                            repos
                                .into_iter()
                                .map(|(id, path)| {
                                    let status = git::get_status(&path).unwrap_or_else(|e| {
                                        log::warn!(
                                            "git status failed for {}: {}",
                                            path.display(),
                                            e
                                        );
                                        None
                                    });
                                    (id, status)
                                })
                                .collect()
                        })
                        .await
                        .unwrap_or_default()
                    },
                    Message::GitStatusLoaded,
                )
            }
            Message::GitStatusLoaded(results) => {
                self.git_status = results
                    .into_iter()
                    .filter_map(|(id, status)| status.map(|s| (id, s)))
                    .collect();
                Task::none()
            }

            // ── Create Dialog ──
            Message::ShowCreateDialog => {
                self.show_create_dialog = true;
                self.create_repo_path.clear();
                self.create_env_name.clear();
                self.create_env_name_touched = false;
                self.create_venv_path.clear();
                self.create_python_version = self.config.defaults.python_version.clone();
                self.creating = false;
                Task::none()
            }
            Message::HideCreateDialog => {
                self.show_create_dialog = false;
                Task::none()
            }
            Message::CreateRepoPathChanged(path) => {
                self.create_repo_path = path;
                if !self.create_env_name_touched {
                    self.create_env_name = std::path::Path::new(&self.create_repo_path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                }
                Task::none()
            }
            Message::CreateEnvNameChanged(name) => {
                self.create_env_name = name;
                self.create_env_name_touched = true;
                Task::none()
            }
            Message::CreateVenvPathChanged(path) => {
                self.create_venv_path = path;
                Task::none()
            }
            Message::CreatePythonVersionChanged(ver) => {
                self.create_python_version = ver;
                Task::none()
            }
            Message::PickFolder => Task::perform(
                pick_folder("Select Repository Folder"),
                Message::FolderPicked,
            ),
            Message::FolderPicked(path) => {
                if let Some(p) = path {
                    self.create_repo_path = p;
                    if !self.create_env_name_touched {
                        self.create_env_name = std::path::Path::new(&self.create_repo_path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                    }
                }
                Task::none()
            }
            Message::PickVenvFolder => Task::perform(
                pick_folder("Select Virtual Environment Folder"),
                Message::VenvFolderPicked,
            ),
            Message::VenvFolderPicked(path) => {
                if let Some(p) = path {
                    self.create_venv_path = p;
                }
                Task::none()
            }
            Message::SubmitCreateEnvironment => {
                if self.create_repo_path.is_empty() || self.creating {
                    return Task::none();
                }
                self.creating = true;
                let repo_path = self.create_repo_path.clone();
                let env_name = self.create_env_name.clone();
                let venv_path = self.create_venv_path.clone();
                let python_version = self.create_python_version.clone();
                let defaults = self.config.defaults.clone();
                let uv_cfg = self.config.uv.clone();

                Task::perform(
                    async move {
                        create_environment_async(
                            repo_path,
                            env_name,
                            venv_path,
                            python_version,
                            defaults,
                            uv_cfg,
                        )
                        .await
                    },
                    Message::EnvironmentCreated,
                )
            }
            Message::EnvironmentCreated(result) => {
                self.creating = false;
                match result {
                    Ok((env, warning)) => {
                        self.show_create_dialog = false;
                        let name = env.name.clone();
                        self.environments.push(env);
                        self.persist_environments();
                        self.sync_tray_envs();
                        self.show_toast(
                            format!("Environment '{}' created", name),
                            ToastVariant::Success,
                        );
                        if let Some(warn) = warning {
                            self.show_toast(warn, ToastVariant::Warning);
                        }
                    }
                    Err(e) => {
                        self.show_toast(
                            format!("Failed to create environment: {}", e),
                            ToastVariant::Error,
                        );
                    }
                }
                Task::done(Message::RefreshGitStatus)
            }

            // ── Sync ──
            Message::SyncEnvironment(env_id) => {
                if let Some(env) = self.environments.iter_mut().find(|e| e.id == env_id) {
                    env.status = EnvStatus::Syncing;
                    let venv_path = env.venv_path.clone();
                    let repo_path = env.repo_path.clone();
                    let id = env_id.clone();
                    let uv_cfg = self.config.uv.clone();

                    Task::perform(
                        async move {
                            uv::sync_environment(&venv_path, &repo_path, &uv_cfg)
                                .await
                                .map_err(|e| e.to_string())?;
                            let info = staleness::compute_deps_hash(&repo_path)
                                .map(|hash| StalenessInfo {
                                    is_stale: false,
                                    reason: None,
                                    deps_hash: Some(hash),
                                    last_checked: Some(Utc::now()),
                                })
                                .unwrap_or_default();
                            Ok((id, info))
                        },
                        Message::EnvironmentSynced,
                    )
                } else {
                    Task::none()
                }
            }
            Message::EnvironmentSynced(result) => {
                match result {
                    Ok((id, info)) => {
                        if let Some(env) = self.environments.iter_mut().find(|e| e.id == id) {
                            env.status = EnvStatus::Ready;
                            env.last_synced_at = Some(Utc::now());
                            env.staleness = info;
                        }
                        self.persist_environments();
                        self.show_toast("Environment synced".to_string(), ToastVariant::Success);
                    }
                    Err(e) => {
                        self.show_toast(format!("Sync failed: {}", e), ToastVariant::Error);
                    }
                }
                Task::none()
            }

            // ── Delete ──
            Message::ShowDeleteConfirm(env_id) => {
                self.confirm_delete_env_id = Some(env_id);
                Task::none()
            }
            Message::HideConfirmDialog => {
                self.confirm_delete_env_id = None;
                Task::none()
            }
            Message::ConfirmDelete => {
                if let Some(env_id) = self.confirm_delete_env_id.take() {
                    let venv_path = self
                        .environments
                        .iter()
                        .find(|e| e.id == env_id)
                        .map(|e| e.venv_path.clone());

                    if let Some(venv_path) = venv_path {
                        let id = env_id.clone();
                        return Task::perform(
                            async move {
                                uv::delete_venv(&venv_path)
                                    .await
                                    .map_err(|e| e.to_string())?;
                                Ok(id)
                            },
                            Message::EnvironmentDeleted,
                        );
                    }
                }
                Task::none()
            }
            Message::EnvironmentDeleted(result) => {
                match result {
                    Ok(id) => {
                        if self.selected_env_id.as_deref() == Some(&id) {
                            self.selected_env_id = None;
                        }
                        self.environments.retain(|e| e.id != id);
                        self.persist_environments();
                        self.sync_tray_envs();
                        self.show_toast("Environment deleted".to_string(), ToastVariant::Success);
                    }
                    Err(e) => {
                        self.show_toast(format!("Delete failed: {}", e), ToastVariant::Error);
                    }
                }
                Task::none()
            }

            // ── Terminal ──
            Message::PickRepoPath(env_id) => Task::perform(
                async move {
                    let path = pick_folder("Select Repository Folder").await;
                    path.map(|p| (env_id, p))
                },
                Message::RepoPathPicked,
            ),
            Message::RepoPathPicked(result) => {
                if let Some((env_id, new_path)) = result {
                    if let Some(env) = self.environments.iter_mut().find(|e| e.id == env_id) {
                        env.repo_path = PathBuf::from(&new_path);
                        let name = env.name.clone();
                        self.persist_environments();
                        self.show_toast(
                            format!("Repository path updated for '{}'", name),
                            ToastVariant::Success,
                        );
                    }
                }
                Task::done(Message::RefreshGitStatus)
            }

            Message::OpenTerminal(env_id) => {
                if let Some(env) = self.environments.iter().find(|e| e.id == env_id) {
                    let global_vars = self.config.defaults.global_env_vars.clone();
                    let custom_terminal = self.config.general.terminal_command.clone();
                    match terminal::spawn_terminal(env, &global_vars, custom_terminal.as_deref()) {
                        Ok(()) => {
                            self.show_toast("Terminal opened".to_string(), ToastVariant::Success);
                        }
                        Err(e) => {
                            self.show_toast(
                                format!("Failed to open terminal: {}", e),
                                ToastVariant::Error,
                            );
                        }
                    }
                }
                Task::none()
            }

            Message::OpenPycharm(env_id) => {
                if let Some(env) = self.environments.iter().find(|e| e.id == env_id) {
                    let custom = self.config.general.pycharm_command.clone();
                    match pycharm::open_pycharm(env, custom.as_deref()) {
                        Ok(None) => {
                            self.show_toast("PyCharm opened".to_string(), ToastVariant::Success);
                        }
                        Ok(Some(warning)) => {
                            self.show_toast(
                                format!("PyCharm opened, but {}", warning),
                                ToastVariant::Warning,
                            );
                        }
                        Err(e) => {
                            self.show_toast(
                                format!("Failed to open PyCharm: {}", e),
                                ToastVariant::Error,
                            );
                        }
                    }
                }
                Task::none()
            }

            // ── Staleness ──
            Message::CheckAllStaleness => {
                let envs: Vec<PythonEnvironment> = self.environments.clone();
                Task::perform(
                    async move {
                        let mut results = Vec::new();
                        for env in &envs {
                            match staleness::check_staleness(env) {
                                Ok(info) => results.push((env.id.clone(), info)),
                                Err(e) => {
                                    log::warn!("Staleness check failed for '{}': {}", env.name, e);
                                }
                            }
                        }
                        Ok(results)
                    },
                    Message::AllStalenessChecked,
                )
            }
            Message::AllStalenessChecked(result) => {
                if let Ok(results) = result {
                    for (id, info) in results {
                        if let Some(env) = self.environments.iter_mut().find(|e| e.id == id) {
                            if info.is_stale {
                                env.status = EnvStatus::Stale;
                            }
                            env.staleness = info;
                        }
                    }
                }
                Task::none()
            }

            // ── Details Tabs ──
            Message::SwitchDetailsTab(tab) => {
                self.active_details_tab = tab;
                match tab {
                    DetailsTab::EnvVars => {
                        if let Some(id) = &self.selected_env_id {
                            let local = self
                                .environments
                                .iter()
                                .find(|e| &e.id == id)
                                .map(|e| e.env_vars.clone())
                                .unwrap_or_default();
                            let global = self.config.defaults.global_env_vars.clone();
                            self.build_env_var_rows(&local, &global);
                            self.global_env_vars_cache = global;
                        }
                        Task::none()
                    }
                    DetailsTab::Dependencies => {
                        self.loading_dependencies = true;
                        self.dependencies.clear();
                        self.load_dependencies_task()
                    }
                    DetailsTab::Pipelines => {
                        self.loading_pipelines = true;
                        self.load_pipelines_task()
                    }
                    DetailsTab::Info => Task::none(),
                }
            }
            Message::EnvVarKeyChanged(idx, key) => {
                if let Some(row) = self.env_var_rows.get_mut(idx) {
                    row.key = key;
                }
                Task::none()
            }
            Message::EnvVarValueChanged(idx, val) => {
                if let Some(row) = self.env_var_rows.get_mut(idx) {
                    row.value = val;
                }
                Task::none()
            }
            Message::AddEnvVar => {
                self.env_var_rows.push(EnvVarRow {
                    key: String::new(),
                    value: String::new(),
                    source: EnvVarSource::Local,
                });
                Task::none()
            }
            Message::RemoveEnvVar(idx) => {
                if idx < self.env_var_rows.len() {
                    self.env_var_rows.remove(idx);
                }
                Task::none()
            }
            Message::OverrideGlobalVar(key) => {
                // Find the global var and convert it to an override
                if let Some(row) = self
                    .env_var_rows
                    .iter_mut()
                    .find(|r| r.key == key && r.source == EnvVarSource::Global)
                {
                    row.source = EnvVarSource::Override;
                }
                Task::none()
            }
            Message::SaveEnvVars => {
                if let Some(id) = &self.selected_env_id {
                    let vars: HashMap<String, String> = self
                        .env_var_rows
                        .iter()
                        .filter(|r| r.source != EnvVarSource::Global && !r.key.is_empty())
                        .map(|r| (r.key.clone(), r.value.clone()))
                        .collect();

                    if let Some(env) = self.environments.iter_mut().find(|e| &e.id == id) {
                        env.env_vars = vars;
                    }
                    self.persist_environments();
                    self.show_toast(
                        "Environment variables saved".to_string(),
                        ToastVariant::Success,
                    );
                }
                Task::none()
            }

            // ── Dependencies ──
            Message::ListDependencies => {
                self.loading_dependencies = true;
                self.dependencies.clear();
                self.load_dependencies_task()
            }
            Message::DependenciesLoaded(result) => {
                self.loading_dependencies = false;
                self.dep_editing_index = None;
                self.dep_editing_version.clear();
                match result {
                    Ok(deps) => self.dependencies = deps,
                    Err(e) => {
                        self.show_toast(
                            format!("Failed to list dependencies: {}", e),
                            ToastVariant::Error,
                        );
                    }
                }
                Task::none()
            }
            Message::DepVersionEditing(idx, version) => {
                self.dep_editing_index = Some(idx);
                self.dep_editing_version = version;
                Task::none()
            }
            Message::DepVersionSubmit(idx) => {
                if let Some(dep) = self.dependencies.get(idx) {
                    let new_ver = self.dep_editing_version.trim().to_string();
                    if new_ver.is_empty() || new_ver == dep.version {
                        self.dep_editing_index = None;
                        self.dep_editing_version.clear();
                        return Task::none();
                    }
                    let pkg_name = dep.name.clone();
                    let spec = format!("{}=={}", pkg_name, new_ver);
                    if let Some(env) = self
                        .selected_env_id
                        .as_ref()
                        .and_then(|id| self.environments.iter().find(|e| &e.id == id))
                    {
                        let venv_path = env.venv_path.clone();
                        let uv_cfg = self.config.uv.clone();
                        self.loading_dependencies = true;
                        return Task::perform(
                            async move {
                                uv::pip_install(&venv_path, &spec, false, &uv_cfg)
                                    .await
                                    .map(|_| pkg_name)
                                    .map_err(|e| e.to_string())
                            },
                            Message::DepVersionUpdated,
                        );
                    }
                }
                Task::none()
            }
            Message::DepVersionUpdated(result) => match result {
                Ok(pkg) => {
                    self.show_toast(format!("Updated {}", pkg), ToastVariant::Success);
                    self.loading_dependencies = true;
                    self.dependencies.clear();
                    self.load_dependencies_task()
                }
                Err(e) => {
                    self.loading_dependencies = false;
                    self.dep_editing_index = None;
                    self.dep_editing_version.clear();
                    self.show_toast(format!("Update failed: {}", e), ToastVariant::Error);
                    Task::none()
                }
            },
            Message::DepInstallNameChanged(v) => {
                self.dep_install.name = v;
                Task::none()
            }
            Message::DepInstallSourceChanged(source) => {
                self.dep_install.source = source;
                Task::none()
            }
            Message::DepInstallPathChanged(v) => {
                self.dep_install.path = v;
                Task::none()
            }
            Message::DepInstallPickPath => Task::perform(
                pick_folder("Select Package Folder"),
                Message::DepInstallPathPicked,
            ),
            Message::DepInstallPathPicked(path) => {
                if let Some(p) = path {
                    self.dep_install.path = p;
                }
                Task::none()
            }
            Message::DepInstallEditableToggled(v) => {
                self.dep_install.editable = v;
                Task::none()
            }
            Message::DepInstallGitUrlChanged(v) => {
                self.dep_install.git_url = v;
                Task::none()
            }
            Message::DepInstallGitRefChanged(v) => {
                self.dep_install.git_ref = v;
                Task::none()
            }
            Message::DepInstallSubmit => {
                let Some((spec, editable)) = self.dep_install.request() else {
                    return Task::none();
                };
                if let Some(env) = self
                    .selected_env_id
                    .as_ref()
                    .and_then(|id| self.environments.iter().find(|e| &e.id == id))
                {
                    let venv_path = env.venv_path.clone();
                    let uv_cfg = self.config.uv.clone();
                    self.loading_dependencies = true;
                    self.dep_install.clear_input();
                    Task::perform(
                        async move {
                            uv::pip_install(&venv_path, &spec, editable, &uv_cfg)
                                .await
                                .map(|_| spec)
                                .map_err(|e| e.to_string())
                        },
                        Message::DepInstalled,
                    )
                } else {
                    Task::none()
                }
            }
            Message::DepInstalled(result) => match result {
                Ok(spec) => {
                    self.show_toast(format!("Installed {}", spec), ToastVariant::Success);
                    self.loading_dependencies = true;
                    self.dependencies.clear();
                    self.load_dependencies_task()
                }
                Err(e) => {
                    self.loading_dependencies = false;
                    self.show_toast(format!("Install failed: {}", e), ToastVariant::Error);
                    Task::none()
                }
            },
            Message::DepUninstall(idx) => {
                if let Some(dep) = self.dependencies.get(idx) {
                    let pkg_name = dep.name.clone();
                    if let Some(env) = self
                        .selected_env_id
                        .as_ref()
                        .and_then(|id| self.environments.iter().find(|e| &e.id == id))
                    {
                        let venv_path = env.venv_path.clone();
                        let uv_cfg = self.config.uv.clone();
                        self.loading_dependencies = true;
                        return Task::perform(
                            async move {
                                uv::pip_uninstall(&venv_path, &pkg_name, &uv_cfg)
                                    .await
                                    .map(|_| pkg_name)
                                    .map_err(|e| e.to_string())
                            },
                            Message::DepUninstalled,
                        );
                    }
                }
                Task::none()
            }
            Message::DepUninstalled(result) => match result {
                Ok(pkg) => {
                    self.show_toast(format!("Uninstalled {}", pkg), ToastVariant::Success);
                    self.loading_dependencies = true;
                    self.dependencies.clear();
                    self.load_dependencies_task()
                }
                Err(e) => {
                    self.loading_dependencies = false;
                    self.show_toast(format!("Uninstall failed: {}", e), ToastVariant::Error);
                    Task::none()
                }
            },
            Message::DepClick(idx) => {
                let now = std::time::Instant::now();
                let is_double = self.dep_last_click_index == Some(idx)
                    && self
                        .dep_last_click_time
                        .is_some_and(|t| now.duration_since(t).as_millis() < 400);
                self.dep_last_click_index = Some(idx);
                self.dep_last_click_time = Some(now);

                if is_double {
                    if let Some(dep) = self.dependencies.get(idx) {
                        let pkg_name = dep.name.clone();
                        if let Some(env) = self
                            .selected_env_id
                            .as_ref()
                            .and_then(|id| self.environments.iter().find(|e| &e.id == id))
                        {
                            let venv_path = env.venv_path.clone();
                            let uv_cfg = self.config.uv.clone();
                            self.loading_dep_info = true;
                            self.dep_info = None;
                            return Task::perform(
                                async move { load_dep_info_async(&venv_path, &pkg_name, &uv_cfg).await },
                                Message::DepInfoLoaded,
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::DepInfoLoaded(result) => {
                self.loading_dep_info = false;
                match result {
                    Ok(info) => {
                        self.dep_info = Some(info);
                    }
                    Err(e) => {
                        self.show_toast(
                            format!("Failed to load package info: {}", e),
                            ToastVariant::Error,
                        );
                    }
                }
                Task::none()
            }
            Message::DepCloseInfo => {
                self.dep_info = None;
                Task::none()
            }
            Message::EnvContextMenu(env_id) => {
                self.context_menu_env_id = Some(env_id);
                Task::none()
            }
            Message::HideContextMenu => {
                self.context_menu_env_id = None;
                Task::none()
            }
            Message::ShowDependencies(env_id) => {
                self.context_menu_env_id = None;
                let _ = self.update(Message::SelectEnvironment(Some(env_id)));
                self.update(Message::SwitchDetailsTab(DetailsTab::Dependencies))
            }
            Message::ShowDepTree(env_id) => {
                self.context_menu_env_id = None;
                if let Some(env) = self.environments.iter().find(|e| e.id == env_id) {
                    let venv_path = env.venv_path.clone();
                    let repo_path = env.repo_path.clone();
                    let uv_cfg = self.config.uv.clone();
                    self.loading_dep_tree = true;
                    self.dep_tree = None;
                    Task::perform(
                        async move { build_dep_tree_async(venv_path, repo_path, uv_cfg).await },
                        Message::DepTreeLoaded,
                    )
                } else {
                    Task::none()
                }
            }
            Message::DepTreeLoaded(result) => {
                self.loading_dep_tree = false;
                match result {
                    Ok(tree) => {
                        self.dep_tree = Some(tree);
                        // Open graph window if not already open
                        if self.graph_window_id.is_none() {
                            let (id, open) = window::open(window::Settings {
                                size: Size::new(1000.0, 700.0),
                                min_size: Some(Size::new(600.0, 400.0)),
                                icon: icon::window_icon(),
                                platform_specific: icon::platform_specific(),
                                ..Default::default()
                            });
                            self.graph_window_id = Some(id);
                            return open.discard();
                        }
                    }
                    Err(e) => {
                        self.show_toast(
                            format!("Failed to build dependency tree: {}", e),
                            ToastVariant::Error,
                        );
                    }
                }
                Task::none()
            }

            // ── Pipelines ──
            Message::PipelinesLoaded(result) => {
                self.loading_pipelines = false;
                match result {
                    Ok(runs) => self.pipeline_runs = runs,
                    Err(e) => {
                        self.show_toast(
                            format!("Failed to load pipelines: {}", e),
                            ToastVariant::Error,
                        );
                    }
                }
                Task::none()
            }
            Message::RefreshPipelines => {
                self.loading_pipelines = true;
                self.load_pipelines_task()
            }
            Message::RerunWorkflow(run_id) => {
                if let Some(client) = &self.github_client {
                    if let Some(env) = self
                        .selected_env_id
                        .as_deref()
                        .and_then(|id| self.environments.iter().find(|e| e.id == id))
                    {
                        if let Ok(repo) = git::get_repo_from_path(&env.repo_path) {
                            let client = client.clone();
                            let owner = repo.owner.clone();
                            let name = repo.name.clone();
                            return Task::perform(
                                async move {
                                    client
                                        .rerun_workflow(&owner, &name, run_id)
                                        .await
                                        .map_err(|e| e.to_string())
                                },
                                Message::RerunWorkflowResult,
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::RerunWorkflowResult(result) => {
                match result {
                    Ok(()) => self.show_toast(
                        "Workflow rerun triggered".to_string(),
                        ToastVariant::Success,
                    ),
                    Err(e) => self.show_toast(format!("Rerun failed: {}", e), ToastVariant::Error),
                }
                Task::none()
            }

            // ── Pull Requests ──
            Message::LoadOrgPullRequests => self.load_org_prs_task(),
            Message::OrgPullRequestsLoaded(result) => {
                self.loading_org_prs = false;
                match result {
                    Ok(prs) => self.org_pull_requests = prs,
                    Err(e) => {
                        self.show_toast(format!("Failed to load PRs: {}", e), ToastVariant::Error);
                    }
                }
                Task::none()
            }
            Message::PrFilterStateChanged(state) => {
                self.pr_filter_state = state;
                self.load_org_prs_task()
            }
            Message::PrFilterAuthorChanged(author) => {
                self.pr_filter_author = author;
                Task::none()
            }
            Message::OpenUrl(url) => {
                let _ = open::that(&url);
                Task::none()
            }

            // ── Settings ──
            Message::SettingsPATChanged(v) => {
                self.settings.github.pat = non_empty(v);
                Task::none()
            }
            Message::SettingsApiBaseUrlChanged(v) => {
                self.settings.github.api_base_url = v;
                Task::none()
            }
            Message::SettingsDefaultOrgChanged(v) => {
                self.settings.github.default_org = non_empty(v);
                Task::none()
            }
            Message::SettingsPythonVersionChanged(v) => {
                self.settings.defaults.python_version = v;
                Task::none()
            }
            Message::SettingsSeedChanged(v) => {
                self.settings.defaults.seed = v;
                Task::none()
            }
            Message::SettingsTerminalCommandChanged(v) => {
                self.settings.general.terminal_command = non_empty(v);
                Task::none()
            }
            Message::SettingsThemeChanged(choice) => {
                self.settings.general.theme = choice;
                ui::theme::set_light_theme(choice == ThemeChoice::Light);
                Task::none()
            }
            Message::SettingsGlobalEnvVarKeyChanged(idx, key) => {
                if let Some(row) = self.settings_global_env_var_rows.get_mut(idx) {
                    row.0 = key;
                }
                Task::none()
            }
            Message::SettingsGlobalEnvVarValueChanged(idx, val) => {
                if let Some(row) = self.settings_global_env_var_rows.get_mut(idx) {
                    row.1 = val;
                }
                Task::none()
            }
            Message::SettingsAddGlobalEnvVar => {
                self.settings_global_env_var_rows
                    .push((String::new(), String::new()));
                Task::none()
            }
            Message::SettingsRemoveGlobalEnvVar(idx) => {
                if idx < self.settings_global_env_var_rows.len() {
                    self.settings_global_env_var_rows.remove(idx);
                }
                Task::none()
            }
            Message::SettingsDefaultVenvDirChanged(v) => {
                self.settings.defaults.default_venv_dir = non_empty(v);
                Task::none()
            }
            Message::SettingsUvExtraArgsChanged(v) => {
                self.settings_uv_extra_args = v;
                Task::none()
            }

            // ── UV index settings ──
            Message::SettingsUvDefaultIndexChanged(v) => {
                self.settings.uv.default_index = non_empty(v);
                Task::none()
            }
            Message::SettingsUvIndexNameChanged(v) => {
                self.settings.uv.index_name = v;
                Task::none()
            }
            Message::SettingsUvUsePublicIndex(v) => {
                self.settings.uv.use_public_index = v;
                Task::none()
            }
            Message::SettingsUvUsernameChanged(v) => {
                self.settings.uv.username = non_empty(v);
                Task::none()
            }
            Message::SettingsUvPasswordChanged(v) => {
                self.settings.uv.password = non_empty(v);
                Task::none()
            }

            Message::SaveSettings => {
                self.apply_settings_to_config();

                let config = self.config.clone();
                let config_path = self.config_path.clone();

                // Update GitHub client
                self.github_client = match &self.config.github.pat {
                    Some(token) => {
                        match GitHubClient::new(
                            token.clone(),
                            self.config.github.api_base_url.clone(),
                        ) {
                            Ok(client) => Some(client),
                            Err(e) => {
                                self.show_toast(e.to_string(), ToastVariant::Error);
                                None
                            }
                        }
                    }
                    None => None,
                };

                Task::perform(
                    async move { loader::save_config(&config_path, &config).map_err(|e| e.to_string()) },
                    Message::SettingsSaved,
                )
            }
            Message::SettingsSaved(result) => {
                match result {
                    Ok(()) => self.show_toast("Settings saved".to_string(), ToastVariant::Success),
                    Err(e) => self.show_toast(
                        format!("Failed to save settings: {}", e),
                        ToastVariant::Error,
                    ),
                }
                Task::none()
            }

            // ── Toasts ──
            Message::DismissToast(id) => {
                self.toasts.retain(|t| t.id != id);
                Task::none()
            }
            Message::ToastTick => {
                let now = std::time::Instant::now();
                self.toasts
                    .retain(|t| now.duration_since(t.created_at).as_secs() < 5);
                Task::none()
            }

            // ── Self-update ──
            Message::CheckForUpdate => {
                let pat = self.config.github.pat.clone();
                let current = env!("CARGO_PKG_VERSION").to_string();
                Task::perform(
                    async move {
                        updater::check_for_update(&current, &pat)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::UpdateCheckResult,
                )
            }
            Message::UpdateCheckResult(result) => {
                match result {
                    Ok(Some(info)) => {
                        self.show_toast(
                            format!("Update available: v{}", info.version),
                            ToastVariant::Info,
                        );
                        self.update_available = Some(info);
                    }
                    Ok(None) => {
                        log::info!("App is up to date (v{})", env!("CARGO_PKG_VERSION"));
                    }
                    Err(e) => {
                        log::warn!("Update check failed: {}", e);
                    }
                }
                Task::none()
            }
            Message::DownloadUpdate(url) => {
                if self.update_downloading {
                    return Task::none();
                }
                self.update_downloading = true;
                let pat = self.config.github.pat.clone();
                Task::perform(
                    async move {
                        updater::download_and_install(&url, &pat)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::UpdateDownloaded,
                )
            }
            Message::UpdateDownloaded(result) => {
                self.update_downloading = false;
                match result {
                    Ok(()) => {
                        self.show_toast(
                            "Update installed! Restart Orthanc to apply.".to_string(),
                            ToastVariant::Success,
                        );
                        self.update_available = None;
                    }
                    Err(e) => {
                        self.show_toast(format!("Update failed: {}", e), ToastVariant::Error);
                    }
                }
                Task::none()
            }

            // ── Startup ──
            Message::UvAvailabilityChecked(available) => {
                self.uv_available = Some(available);
                if !available {
                    self.show_toast(
                        "uv is not available on PATH. Environment creation will fail.".to_string(),
                        ToastVariant::Warning,
                    );
                }
                // Check staleness on startup
                Task::done(Message::CheckAllStaleness)
            }
        }
    }

    // ── View ─────────────────────────────────────────────────────────────

    pub fn view(&self, window_id: window::Id) -> Element<'_, Message> {
        // Graph window
        if Some(window_id) == self.graph_window_id {
            return ui::dependency_tree::view_graph(
                self.dep_tree.as_deref(),
                self.loading_dep_tree,
            );
        }

        // Main window
        let sidebar = ui::sidebar::view(self.active_page);

        let main_content: Element<Message> = match self.active_page {
            Page::Environments => {
                let table = ui::environment_table::view(
                    &self.environments,
                    &self.selected_env_id,
                    self.context_menu_env_id.as_deref(),
                    &self.git_status,
                );

                let details: Option<Element<Message>> =
                    self.selected_env_id.as_ref().and_then(|id| {
                        self.environments
                            .iter()
                            .find(|e| &e.id == id)
                            .map(|env| ui::environment_details::view(env, self))
                    });

                let mut content = column![table].spacing(0).width(Length::Fill);
                if let Some(details_el) = details {
                    content = content.push(details_el);
                }
                content.into()
            }
            Page::PullRequests => ui::pull_requests_page::view(
                &self.org_pull_requests,
                self.loading_org_prs,
                &self.pr_filter_state,
                &self.pr_filter_author,
            ),
            Page::Settings => ui::settings_page::view(self),
        };

        let layout = row![
            sidebar,
            container(main_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme| container::Style {
                    background: Some(iced::Background::Color(ui::theme::BG_DARKEST())),
                    ..Default::default()
                })
        ]
        .height(Length::Fill);

        // Overlay stack: create dialog, confirm dialog, toasts
        let mut stack_children: Vec<Element<Message>> = vec![layout.into()];

        if self.show_create_dialog {
            stack_children.push(ui::create_dialog::view(
                &self.create_repo_path,
                &self.create_env_name,
                &self.create_venv_path,
                self.config.defaults.default_venv_dir.as_deref(),
                &self.create_python_version,
                self.creating,
            ));
        }

        if let Some(env_id) = &self.confirm_delete_env_id {
            let env_name = self
                .environments
                .iter()
                .find(|e| &e.id == env_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let confirm_msg = format!(
                "Are you sure you want to delete '{}'? This will remove the virtual environment.",
                env_name
            );
            stack_children.push(ui::confirm_dialog::view_owned(
                "Delete Environment".to_string(),
                confirm_msg,
            ));
        }

        if !self.toasts.is_empty() {
            stack_children.push(ui::toast::view(&self.toasts));
        }

        iced::widget::stack(stack_children)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    fn show_toast(&mut self, message: String, variant: ToastVariant) {
        let id = self.next_toast_id;
        self.next_toast_id += 1;
        self.toasts.push(Toast {
            id,
            message,
            variant,
            created_at: std::time::Instant::now(),
        });
    }

    fn persist_environments(&self) {
        let persisted: Vec<orthanc_lib::models::PersistedEnvConfig> = self
            .environments
            .iter()
            .map(|env| orthanc_lib::models::PersistedEnvConfig {
                id: env.id.clone(),
                name: env.name.clone(),
                repo_path: env.repo_path.to_string_lossy().to_string(),
                venv_path: env.venv_path.to_string_lossy().to_string(),
                python_version: env.python_version.clone(),
                env_vars: env.env_vars.clone(),
            })
            .collect();

        let mut config = self.config.clone();
        config.environments = persisted;
        if let Err(e) = loader::save_config(&self.config_path, &config) {
            log::error!("Failed to persist environments: {}", e);
        }
    }

    fn build_env_var_rows(
        &mut self,
        local: &HashMap<String, String>,
        global: &HashMap<String, String>,
    ) {
        self.env_var_rows.clear();
        // Local vars
        for (k, v) in local {
            let source = if global.contains_key(k) {
                EnvVarSource::Override
            } else {
                EnvVarSource::Local
            };
            self.env_var_rows.push(EnvVarRow {
                key: k.clone(),
                value: v.clone(),
                source,
            });
        }
        // Global vars not overridden
        for (k, v) in global {
            if !local.contains_key(k) {
                self.env_var_rows.push(EnvVarRow {
                    key: k.clone(),
                    value: v.clone(),
                    source: EnvVarSource::Global,
                });
            }
        }
    }

    fn load_org_prs_task(&mut self) -> Task<Message> {
        if let Some(client) = &self.github_client {
            if let Some(org) = &self.config.github.default_org {
                if !org.is_empty() {
                    self.loading_org_prs = true;
                    let client = client.clone();
                    let org = org.clone();
                    let state = self.pr_filter_state.clone();
                    let author = if self.pr_filter_author.is_empty() {
                        None
                    } else {
                        Some(self.pr_filter_author.clone())
                    };
                    return Task::perform(
                        async move {
                            client
                                .search_org_pull_requests(&org, Some(&state), author.as_deref())
                                .await
                                .map_err(|e| e.to_string())
                        },
                        Message::OrgPullRequestsLoaded,
                    );
                }
            }
        }
        Task::done(Message::OrgPullRequestsLoaded(Ok(Vec::new())))
    }

    fn load_dependencies_task(&self) -> Task<Message> {
        if let Some(env) = self
            .selected_env_id
            .as_ref()
            .and_then(|id| self.environments.iter().find(|e| &e.id == id))
        {
            let venv_path = env.venv_path.clone();
            let repo_path = env.repo_path.clone();
            let uv_cfg = self.config.uv.clone();
            Task::perform(
                async move { load_dependencies_async(venv_path, repo_path, uv_cfg).await },
                Message::DependenciesLoaded,
            )
        } else {
            Task::done(Message::DependenciesLoaded(Ok(Vec::new())))
        }
    }

    fn load_pipelines_task(&self) -> Task<Message> {
        if let (Some(client), Some(env)) = (
            &self.github_client,
            self.selected_env_id
                .as_ref()
                .and_then(|id| self.environments.iter().find(|e| &e.id == id)),
        ) {
            let repo_path = env.repo_path.clone();
            let client = client.clone();
            Task::perform(
                async move {
                    let repo = git::get_repo_from_path(&repo_path).map_err(|e| e.to_string())?;
                    let branch = git::get_current_branch(&repo_path).ok();
                    client
                        .list_workflow_runs(&repo.owner, &repo.name, branch.as_deref())
                        .await
                        .map_err(|e| e.to_string())
                },
                Message::PipelinesLoaded,
            )
        } else {
            Task::done(Message::PipelinesLoaded(Ok(Vec::new())))
        }
    }

    fn sync_settings_form(&mut self) {
        self.settings = self.config.clone();
        self.settings_uv_extra_args = self.config.defaults.uv_extra_args.join(" ");
        self.settings_global_env_var_rows = self
            .config
            .defaults
            .global_env_vars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
    }

    fn apply_settings_to_config(&mut self) {
        let mut config = self.settings.clone();
        // Environments change outside the Settings page; keep the live list.
        config.environments = std::mem::take(&mut self.config.environments);
        config.defaults.global_env_vars = self
            .settings_global_env_var_rows
            .iter()
            .filter(|(k, _)| !k.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        config.defaults.uv_extra_args = self
            .settings_uv_extra_args
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if config.uv.index_name.is_empty() {
            config.uv.index_name = "private".to_string();
        }
        self.config = config;
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Settings text box value: empty means "not set".
fn non_empty(v: String) -> Option<String> {
    (!v.is_empty()).then_some(v)
}

/// Where a new venv goes when no path is given: `<default_venv_dir>/<name>`
/// if one is set in Settings, otherwise `<repo>/.venv`.
pub fn default_venv_path(repo: &Path, name: &str, default_dir: Option<&str>) -> PathBuf {
    match default_dir.filter(|d| !d.is_empty()) {
        Some(dir) => Path::new(dir).join(name),
        None => repo.join(".venv"),
    }
}

fn load_persisted_environments(config: &AppConfig) -> Vec<PythonEnvironment> {
    config
        .environments
        .iter()
        .map(|p| PythonEnvironment {
            id: p.id.clone(),
            name: p.name.clone(),
            repo_path: PathBuf::from(&p.repo_path),
            venv_path: PathBuf::from(&p.venv_path),
            python_version: p.python_version.clone(),
            status: EnvStatus::Ready,
            created_at: Utc::now(),
            last_synced_at: None,
            staleness: StalenessInfo::default(),
            env_vars: p.env_vars.clone(),
        })
        .collect()
}

async fn pick_folder(title: &'static str) -> Option<String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_folder()
        .await?;
    Some(handle.path().to_string_lossy().to_string())
}

async fn create_environment_async(
    repo_path: String,
    env_name: String,
    custom_venv_path: String,
    python_version: String,
    defaults: DefaultsConfig,
    uv_config: UvConfig,
) -> Result<(PythonEnvironment, Option<String>), String> {
    let repo_path_buf = PathBuf::from(&repo_path);
    if !repo_path_buf.exists() {
        return Err(format!("Repository path does not exist: {}", repo_path));
    }

    let name = if env_name.is_empty() {
        repo_path_buf
            .file_name()
            .ok_or_else(|| "Invalid repository path".to_string())?
            .to_string_lossy()
            .to_string()
    } else {
        env_name
    };

    let venv_path = if !custom_venv_path.is_empty() {
        PathBuf::from(&custom_venv_path)
    } else {
        default_venv_path(&repo_path_buf, &name, defaults.default_venv_dir.as_deref())
    };

    if let Some(parent) = venv_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory: {}", e))?;
    }

    uv::create_venv(
        &venv_path,
        &python_version,
        defaults.seed,
        &defaults.uv_extra_args,
        &uv_config,
    )
    .await
    .map_err(|e| e.to_string())?;

    let install_warning = uv::install_dev(&venv_path, &repo_path_buf, &uv_config).await;

    let deps_hash = staleness::compute_deps_hash(&repo_path_buf).ok();

    Ok((
        PythonEnvironment {
            id: Uuid::new_v4().to_string(),
            name,
            repo_path: repo_path_buf,
            venv_path,
            python_version,
            status: EnvStatus::Ready,
            created_at: Utc::now(),
            last_synced_at: None,
            staleness: StalenessInfo {
                is_stale: false,
                reason: None,
                deps_hash,
                last_checked: Some(Utc::now()),
            },
            env_vars: HashMap::new(),
        },
        install_warning,
    ))
}

async fn load_dependencies_async(
    venv_path: PathBuf,
    repo_path: PathBuf,
    uv_config: UvConfig,
) -> Result<Vec<Dependency>, String> {
    let packages = uv::pip_list(&venv_path, &uv_config)
        .await
        .map_err(|e| e.to_string())?;

    // Parse pyproject.toml to classify direct vs dev vs transitive
    let (direct_deps, dev_deps) = parse_project_deps(&repo_path);

    let deps = packages
        .into_iter()
        .map(|(name, version, is_editable)| {
            let name_lower = name.to_lowercase().replace('-', "_");
            let source = if is_editable {
                DepSource::Editable
            } else if dev_deps.contains(&name_lower) {
                DepSource::Dev
            } else if direct_deps.contains(&name_lower) {
                DepSource::Direct
            } else {
                DepSource::Transitive
            };
            Dependency {
                name,
                version,
                source,
            }
        })
        .collect();

    Ok(deps)
}

async fn load_dep_info_async(
    venv_path: &std::path::Path,
    package: &str,
    uv_config: &UvConfig,
) -> Result<DepInfo, String> {
    let fields = uv::pip_show(venv_path, package, uv_config)
        .await
        .map_err(|e| e.to_string())?;

    let requires_str = fields.get("Requires").cloned().unwrap_or_default();
    let required_by_str = fields.get("Required-by").cloned().unwrap_or_default();

    let parse_list = |s: &str| -> Vec<String> {
        s.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    };

    Ok(DepInfo {
        name: fields
            .get("Name")
            .cloned()
            .unwrap_or_else(|| package.to_string()),
        version: fields.get("Version").cloned().unwrap_or_default(),
        summary: fields.get("Summary").cloned().unwrap_or_default(),
        location: fields.get("Location").cloned().unwrap_or_default(),
        requires: parse_list(&requires_str),
        required_by: parse_list(&required_by_str),
    })
}

async fn build_dep_tree_async(
    venv_path: PathBuf,
    repo_path: PathBuf,
    uv_config: UvConfig,
) -> Result<Vec<DepTreeNode>, String> {
    // Get all installed packages
    let packages = uv::pip_list(&venv_path, &uv_config)
        .await
        .map_err(|e| e.to_string())?;
    let pkg_names: Vec<String> = packages.iter().map(|(n, _, _)| n.clone()).collect();

    if pkg_names.is_empty() {
        return Ok(Vec::new());
    }

    // Get dependency info for all packages
    let show_results = uv::pip_show_all(&venv_path, &pkg_names, &uv_config)
        .await
        .map_err(|e| e.to_string())?;

    // Build lookup maps
    use std::collections::{HashMap, HashSet};
    let mut version_map: HashMap<String, String> = HashMap::new();
    let mut requires_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_required: HashSet<String> = HashSet::new();

    for (name, version, requires, _required_by) in &show_results {
        let key = name.to_lowercase().replace('-', "_");
        version_map.insert(key.clone(), version.clone());
        let req_normalized: Vec<String> = requires
            .iter()
            .map(|r| r.to_lowercase().replace('-', "_"))
            .collect();
        for r in &req_normalized {
            all_required.insert(r.clone());
        }
        requires_map.insert(key, req_normalized);
    }

    // Also add packages from pip_list that might not have show results
    for (name, version, _) in &packages {
        let key = name.to_lowercase().replace('-', "_");
        version_map
            .entry(key.clone())
            .or_insert_with(|| version.clone());
        requires_map.entry(key).or_default();
    }

    // Determine root packages: those not required by any other package
    let (direct_deps, dev_deps) = parse_project_deps(&repo_path);

    // Roots are: direct + dev deps that are installed, plus editable packages
    // If no direct/dev deps are known, roots = packages not required by anyone
    let all_keys: Vec<String> = version_map.keys().cloned().collect();
    let roots: Vec<String> = if direct_deps.is_empty() && dev_deps.is_empty() {
        all_keys
            .iter()
            .filter(|k| !all_required.contains(k.as_str()))
            .cloned()
            .collect()
    } else {
        let mut roots: Vec<String> = direct_deps
            .iter()
            .chain(dev_deps.iter())
            .filter(|d| version_map.contains_key(d.as_str()))
            .cloned()
            .collect();
        // Also add editable packages
        for (name, _, is_editable) in &packages {
            if *is_editable {
                let key = name.to_lowercase().replace('-', "_");
                if !roots.contains(&key) {
                    roots.push(key);
                }
            }
        }
        // And any other packages not required by anyone (orphan roots)
        for k in &all_keys {
            if !all_required.contains(k.as_str()) && !roots.contains(k) {
                roots.push(k.clone());
            }
        }
        roots
    };

    // Build tree recursively with cycle detection
    fn build_node(
        name: &str,
        version_map: &HashMap<String, String>,
        requires_map: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
    ) -> DepTreeNode {
        let version = version_map.get(name).cloned().unwrap_or_default();
        let children = if visited.contains(name) {
            Vec::new() // cycle detected, stop
        } else {
            visited.insert(name.to_string());
            let deps = requires_map.get(name).cloned().unwrap_or_default();
            let mut children: Vec<DepTreeNode> = deps
                .iter()
                .map(|dep| build_node(dep, version_map, requires_map, visited))
                .collect();
            children.sort_by(|a, b| a.name.cmp(&b.name));
            visited.remove(name);
            children
        };

        DepTreeNode {
            name: name.to_string(),
            version,
            children,
        }
    }

    let mut visited = HashSet::new();
    let mut tree: Vec<DepTreeNode> = roots
        .iter()
        .map(|root| build_node(root, &version_map, &requires_map, &mut visited))
        .collect();
    tree.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(tree)
}

/// Parse pyproject.toml and requirements.txt to find direct and dev dependency names.
fn parse_project_deps(repo_path: &std::path::Path) -> (Vec<String>, Vec<String>) {
    let mut direct = Vec::new();
    let mut dev = Vec::new();

    let pyproject_path = repo_path.join("pyproject.toml");
    if let Ok(content) = std::fs::read_to_string(&pyproject_path) {
        if let Ok(doc) = content.parse::<toml::Value>() {
            // [project.dependencies]
            if let Some(deps) = doc
                .get("project")
                .and_then(|p| p.get("dependencies"))
                .and_then(|d| d.as_array())
            {
                for dep in deps {
                    if let Some(s) = dep.as_str() {
                        direct.push(normalize_dep_name(s));
                    }
                }
            }
            // [project.optional-dependencies.dev]
            if let Some(opt) = doc
                .get("project")
                .and_then(|p| p.get("optional-dependencies"))
                .and_then(|o| o.as_table())
            {
                if let Some(dev_deps) = opt.get("dev").and_then(|d| d.as_array()) {
                    for dep in dev_deps {
                        if let Some(s) = dep.as_str() {
                            dev.push(normalize_dep_name(s));
                        }
                    }
                }
            }
            // [tool.uv.dev-dependencies] or [dependency-groups.dev]
            if let Some(uv_dev) = doc
                .get("tool")
                .and_then(|t| t.get("uv"))
                .and_then(|u| u.get("dev-dependencies"))
                .and_then(|d| d.as_array())
            {
                for dep in uv_dev {
                    if let Some(s) = dep.as_str() {
                        dev.push(normalize_dep_name(s));
                    }
                }
            }
            if let Some(groups) = doc.get("dependency-groups").and_then(|g| g.as_table()) {
                if let Some(dev_group) = groups.get("dev").and_then(|d| d.as_array()) {
                    for dep in dev_group {
                        if let Some(s) = dep.as_str() {
                            dev.push(normalize_dep_name(s));
                        }
                    }
                }
            }
        }
    }

    // Also check requirements.txt
    let req_path = repo_path.join("requirements.txt");
    if let Ok(content) = std::fs::read_to_string(&req_path) {
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') && !line.starts_with('-') {
                direct.push(normalize_dep_name(line));
            }
        }
    }

    (direct, dev)
}

/// Normalize a dependency specifier to just the lowercase package name.
/// e.g. "requests>=2.28" -> "requests", "my-package[extra]" -> "my_package"
fn normalize_dep_name(spec: &str) -> String {
    let name = spec
        .split(['>', '<', '=', '!', '~', ';', '['])
        .next()
        .unwrap_or(spec)
        .trim();
    name.to_lowercase().replace('-', "_")
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_app_has_correct_defaults() {
        // We can't fully test new() without an iced runtime, but we can test helpers
        let config = AppConfig::default();
        let envs = load_persisted_environments(&config);
        assert!(envs.is_empty());
    }

    #[test]
    fn test_load_persisted_environments() {
        let mut config = AppConfig::default();
        config
            .environments
            .push(orthanc_lib::models::PersistedEnvConfig {
                id: "env-1".to_string(),
                name: "test".to_string(),
                repo_path: "/tmp/repo".to_string(),
                venv_path: "/tmp/VirtualEnvironments/test".to_string(),
                python_version: "3.12".to_string(),
                env_vars: HashMap::new(),
            });
        let envs = load_persisted_environments(&config);
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].name, "test");
        assert_eq!(envs[0].status, EnvStatus::Ready);
    }

    #[test]
    fn test_build_env_var_rows_local_only() {
        let mut app = create_test_app();
        let local: HashMap<String, String> = [("KEY1".to_string(), "val1".to_string())].into();
        let global: HashMap<String, String> = HashMap::new();
        app.build_env_var_rows(&local, &global);
        assert_eq!(app.env_var_rows.len(), 1);
        assert_eq!(app.env_var_rows[0].source, EnvVarSource::Local);
    }

    #[test]
    fn test_build_env_var_rows_global_only() {
        let mut app = create_test_app();
        let local: HashMap<String, String> = HashMap::new();
        let global: HashMap<String, String> = [("GKEY".to_string(), "gval".to_string())].into();
        app.build_env_var_rows(&local, &global);
        assert_eq!(app.env_var_rows.len(), 1);
        assert_eq!(app.env_var_rows[0].source, EnvVarSource::Global);
    }

    #[test]
    fn test_build_env_var_rows_override() {
        let mut app = create_test_app();
        let local: HashMap<String, String> =
            [("SHARED".to_string(), "local_val".to_string())].into();
        let global: HashMap<String, String> =
            [("SHARED".to_string(), "global_val".to_string())].into();
        app.build_env_var_rows(&local, &global);
        assert_eq!(app.env_var_rows.len(), 1);
        assert_eq!(app.env_var_rows[0].source, EnvVarSource::Override);
        assert_eq!(app.env_var_rows[0].value, "local_val");
    }

    #[test]
    fn test_apply_settings_empty_pat_becomes_none() {
        let mut app = create_test_app();
        app.config.github.pat = Some("old".to_string());
        app.sync_settings_form();
        let _ = app.update(Message::SettingsPATChanged(String::new()));
        app.apply_settings_to_config();
        assert!(app.config.github.pat.is_none());
    }

    #[test]
    fn test_apply_settings_keeps_environments_added_meanwhile() {
        let mut app = create_test_app();
        app.sync_settings_form();
        app.config
            .environments
            .push(orthanc_lib::models::PersistedEnvConfig {
                id: "e1".to_string(),
                name: "added-while-editing".to_string(),
                repo_path: "/r".to_string(),
                venv_path: "/r/.venv".to_string(),
                python_version: "3.12".to_string(),
                env_vars: HashMap::new(),
            });
        app.settings.defaults.seed = true;
        app.apply_settings_to_config();
        assert!(app.config.defaults.seed);
        assert_eq!(app.config.environments.len(), 1);
    }

    #[test]
    fn test_apply_settings_with_pat() {
        let mut app = create_test_app();
        app.settings.github.pat = Some("ghp_test123".to_string());
        app.apply_settings_to_config();
        assert_eq!(app.config.github.pat.as_deref(), Some("ghp_test123"));
    }

    #[test]
    fn test_apply_settings_global_env_vars() {
        let mut app = create_test_app();
        app.settings_global_env_var_rows = vec![
            ("KEY1".to_string(), "val1".to_string()),
            ("".to_string(), "ignored".to_string()),
            ("KEY2".to_string(), "val2".to_string()),
        ];
        app.apply_settings_to_config();
        assert_eq!(app.config.defaults.global_env_vars.len(), 2);
        assert_eq!(
            app.config.defaults.global_env_vars.get("KEY1").unwrap(),
            "val1"
        );
    }

    #[test]
    fn test_show_toast() {
        let mut app = create_test_app();
        app.show_toast("hello".to_string(), ToastVariant::Info);
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].message, "hello");
        app.show_toast("world".to_string(), ToastVariant::Error);
        assert_eq!(app.toasts.len(), 2);
        assert_eq!(app.next_toast_id, 2);
    }

    // ── Message handling tests ─────────────────────────────────────────

    #[test]
    fn test_navigate_to_settings() {
        let mut app = create_test_app();
        let _ = app.update(Message::NavigateTo(Page::Settings));
        assert_eq!(app.active_page, Page::Settings);
    }

    #[test]
    fn test_navigate_to_environments() {
        let mut app = create_test_app();
        app.active_page = Page::Settings;
        let _ = app.update(Message::NavigateTo(Page::Environments));
        assert_eq!(app.active_page, Page::Environments);
    }

    #[test]
    fn test_navigate_to_settings_syncs_form() {
        let mut app = create_test_app();
        app.config.github.pat = Some("my-token".to_string());
        app.config.defaults.python_version = "3.11".to_string();
        let _ = app.update(Message::NavigateTo(Page::Settings));
        assert_eq!(app.settings.github.pat.as_deref(), Some("my-token"));
        assert_eq!(app.settings.defaults.python_version, "3.11");
    }

    #[test]
    fn test_select_environment() {
        let mut app = create_test_app();
        let _ = app.update(Message::SelectEnvironment(Some("env-1".to_string())));
        assert_eq!(app.selected_env_id, Some("env-1".to_string()));
        // Resets details tab
        assert_eq!(app.active_details_tab, DetailsTab::Info);
    }

    #[test]
    fn test_deselect_environment() {
        let mut app = create_test_app();
        app.selected_env_id = Some("env-1".to_string());
        let _ = app.update(Message::SelectEnvironment(None));
        assert_eq!(app.selected_env_id, None);
    }

    #[test]
    fn test_show_create_dialog() {
        let mut app = create_test_app();
        let _ = app.update(Message::ShowCreateDialog);
        assert!(app.show_create_dialog);
        assert!(app.create_repo_path.is_empty());
        assert!(app.create_env_name.is_empty());
        assert!(!app.create_env_name_touched);
        assert!(app.create_venv_path.is_empty());
        assert_eq!(app.create_python_version, "3.12");
        assert!(!app.creating);
    }

    #[test]
    fn test_hide_create_dialog() {
        let mut app = create_test_app();
        app.show_create_dialog = true;
        let _ = app.update(Message::HideCreateDialog);
        assert!(!app.show_create_dialog);
    }

    #[test]
    fn test_create_repo_path_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::CreateRepoPathChanged(
            "/home/user/project".to_string(),
        ));
        assert_eq!(app.create_repo_path, "/home/user/project");
        // Name auto-derived from path when not touched
        assert_eq!(app.create_env_name, "project");
    }

    #[test]
    fn test_create_env_name_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::CreateEnvNameChanged("my-custom-env".to_string()));
        assert_eq!(app.create_env_name, "my-custom-env");
        assert!(app.create_env_name_touched);
    }

    #[test]
    fn test_create_env_name_not_overwritten_when_touched() {
        let mut app = create_test_app();
        let _ = app.update(Message::CreateEnvNameChanged("custom".to_string()));
        assert!(app.create_env_name_touched);
        // Changing repo path should NOT override the custom name
        let _ = app.update(Message::CreateRepoPathChanged(
            "/home/user/other-project".to_string(),
        ));
        assert_eq!(app.create_env_name, "custom");
    }

    #[test]
    fn test_create_env_name_auto_derived_until_touched() {
        let mut app = create_test_app();
        let _ = app.update(Message::CreateRepoPathChanged(
            "/home/user/alpha".to_string(),
        ));
        assert_eq!(app.create_env_name, "alpha");
        let _ = app.update(Message::CreateRepoPathChanged(
            "/home/user/beta".to_string(),
        ));
        assert_eq!(app.create_env_name, "beta");
        // Now touch it
        let _ = app.update(Message::CreateEnvNameChanged("fixed".to_string()));
        let _ = app.update(Message::CreateRepoPathChanged(
            "/home/user/gamma".to_string(),
        ));
        assert_eq!(app.create_env_name, "fixed");
    }

    #[test]
    fn test_default_venv_path() {
        let repo = Path::new("/work/proj");
        assert_eq!(default_venv_path(repo, "proj", None), repo.join(".venv"));
        assert_eq!(
            default_venv_path(repo, "proj", Some("")),
            repo.join(".venv")
        );
        assert_eq!(
            default_venv_path(repo, "proj", Some("/venvs")),
            Path::new("/venvs").join("proj")
        );
    }

    #[test]
    fn test_create_venv_path_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::CreateVenvPathChanged(
            "/custom/venvs/my-env".to_string(),
        ));
        assert_eq!(app.create_venv_path, "/custom/venvs/my-env");
    }

    #[test]
    fn test_venv_folder_picked_some() {
        let mut app = create_test_app();
        let _ = app.update(Message::VenvFolderPicked(Some("/custom/venvs".to_string())));
        assert_eq!(app.create_venv_path, "/custom/venvs");
    }

    #[test]
    fn test_venv_folder_picked_none() {
        let mut app = create_test_app();
        app.create_venv_path = "original".to_string();
        let _ = app.update(Message::VenvFolderPicked(None));
        assert_eq!(app.create_venv_path, "original");
    }

    #[test]
    fn test_create_python_version_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::CreatePythonVersionChanged("3.11".to_string()));
        assert_eq!(app.create_python_version, "3.11");
    }

    #[test]
    fn test_folder_picked_some() {
        let mut app = create_test_app();
        let _ = app.update(Message::FolderPicked(Some("/home/user/repo".to_string())));
        assert_eq!(app.create_repo_path, "/home/user/repo");
        assert_eq!(app.create_env_name, "repo");
    }

    #[test]
    fn test_folder_picked_does_not_override_custom_name() {
        let mut app = create_test_app();
        let _ = app.update(Message::CreateEnvNameChanged("my-env".to_string()));
        let _ = app.update(Message::FolderPicked(Some("/home/user/repo".to_string())));
        assert_eq!(app.create_repo_path, "/home/user/repo");
        assert_eq!(app.create_env_name, "my-env");
    }

    #[test]
    fn test_folder_picked_none() {
        let mut app = create_test_app();
        app.create_repo_path = "original".to_string();
        let _ = app.update(Message::FolderPicked(None));
        assert_eq!(app.create_repo_path, "original");
    }

    #[test]
    fn test_submit_create_environment_empty_path() {
        let mut app = create_test_app();
        app.create_repo_path = String::new();
        let _ = app.update(Message::SubmitCreateEnvironment);
        assert!(!app.creating); // Should not start creating
    }

    #[test]
    fn test_submit_create_environment_already_creating() {
        let mut app = create_test_app();
        app.creating = true;
        app.create_repo_path = "/some/path".to_string();
        let _ = app.update(Message::SubmitCreateEnvironment);
        // Should remain creating (no double submit)
        assert!(app.creating);
    }

    #[test]
    fn test_environment_created_success() {
        let mut app = create_test_app();
        app.creating = true;
        app.show_create_dialog = true;
        let env = PythonEnvironment {
            id: "new-env".to_string(),
            name: "my-project".to_string(),
            repo_path: PathBuf::from("/tmp/repo"),
            venv_path: PathBuf::from("/tmp/VirtualEnvironments/my-project"),
            python_version: "3.12".to_string(),
            status: EnvStatus::Ready,
            created_at: Utc::now(),
            last_synced_at: None,
            staleness: StalenessInfo::default(),
            env_vars: HashMap::new(),
        };
        let _ = app.update(Message::EnvironmentCreated(Ok((env, None))));
        assert!(!app.creating);
        assert!(!app.show_create_dialog);
        assert_eq!(app.environments.len(), 1);
        assert_eq!(app.environments[0].name, "my-project");
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
    }

    #[test]
    fn test_environment_created_with_install_warning() {
        let mut app = create_test_app();
        app.creating = true;
        app.show_create_dialog = true;
        let env = sample_env("warned-env");
        let _ = app.update(Message::EnvironmentCreated(Ok((
            env,
            Some("Dependencies could not be installed".to_string()),
        ))));
        assert!(!app.creating);
        assert!(!app.show_create_dialog);
        assert_eq!(app.environments.len(), 1);
        assert_eq!(app.toasts.len(), 2);
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
        assert_eq!(app.toasts[1].variant, ToastVariant::Warning);
    }

    #[test]
    fn test_environment_created_failure() {
        let mut app = create_test_app();
        app.creating = true;
        app.show_create_dialog = true;
        let _ = app.update(Message::EnvironmentCreated(Err("uv not found".to_string())));
        assert!(!app.creating);
        assert!(app.show_create_dialog); // Dialog stays open on error
        assert_eq!(app.environments.len(), 0);
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_sync_environment_sets_syncing_status() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        let _ = app.update(Message::SyncEnvironment("env-1".to_string()));
        assert_eq!(app.environments[0].status, EnvStatus::Syncing);
    }

    #[test]
    fn test_environment_synced_success() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        let info = StalenessInfo {
            is_stale: false,
            reason: None,
            deps_hash: Some("abc123".to_string()),
            last_checked: Some(Utc::now()),
        };
        let _ = app.update(Message::EnvironmentSynced(Ok(("env-1".to_string(), info))));
        assert_eq!(app.environments[0].status, EnvStatus::Ready);
        assert!(app.environments[0].last_synced_at.is_some());
        assert!(!app.environments[0].staleness.is_stale);
    }

    #[test]
    fn test_environment_synced_failure() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        let _ = app.update(Message::EnvironmentSynced(Err("sync failed".to_string())));
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_show_delete_confirm() {
        let mut app = create_test_app();
        let _ = app.update(Message::ShowDeleteConfirm("env-1".to_string()));
        assert_eq!(app.confirm_delete_env_id, Some("env-1".to_string()));
    }

    #[test]
    fn test_hide_confirm_dialog() {
        let mut app = create_test_app();
        app.confirm_delete_env_id = Some("env-1".to_string());
        let _ = app.update(Message::HideConfirmDialog);
        assert!(app.confirm_delete_env_id.is_none());
    }

    #[test]
    fn test_environment_deleted_success() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        let _ = app.update(Message::EnvironmentDeleted(Ok("env-1".to_string())));
        assert!(app.environments.is_empty());
        assert!(app.selected_env_id.is_none());
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
    }

    #[test]
    fn test_environment_deleted_preserves_other_envs() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.environments.push(sample_env("env-2"));
        let _ = app.update(Message::EnvironmentDeleted(Ok("env-1".to_string())));
        assert_eq!(app.environments.len(), 1);
        assert_eq!(app.environments[0].id, "env-2");
    }

    #[test]
    fn test_switch_details_tab_to_env_vars() {
        let mut app = create_test_app();
        let mut env = sample_env("env-1");
        env.env_vars
            .insert("LOCAL_KEY".to_string(), "local_val".to_string());
        app.environments.push(env);
        app.selected_env_id = Some("env-1".to_string());
        app.config
            .defaults
            .global_env_vars
            .insert("GLOBAL_KEY".to_string(), "gval".to_string());

        let _ = app.update(Message::SwitchDetailsTab(DetailsTab::EnvVars));
        assert_eq!(app.active_details_tab, DetailsTab::EnvVars);
        assert_eq!(app.env_var_rows.len(), 2);
    }

    #[test]
    fn test_env_var_key_changed() {
        let mut app = create_test_app();
        app.env_var_rows.push(EnvVarRow {
            key: "OLD".to_string(),
            value: "val".to_string(),
            source: EnvVarSource::Local,
        });
        let _ = app.update(Message::EnvVarKeyChanged(0, "NEW".to_string()));
        assert_eq!(app.env_var_rows[0].key, "NEW");
    }

    #[test]
    fn test_env_var_value_changed() {
        let mut app = create_test_app();
        app.env_var_rows.push(EnvVarRow {
            key: "K".to_string(),
            value: "old".to_string(),
            source: EnvVarSource::Local,
        });
        let _ = app.update(Message::EnvVarValueChanged(0, "new".to_string()));
        assert_eq!(app.env_var_rows[0].value, "new");
    }

    #[test]
    fn test_add_env_var() {
        let mut app = create_test_app();
        let _ = app.update(Message::AddEnvVar);
        assert_eq!(app.env_var_rows.len(), 1);
        assert_eq!(app.env_var_rows[0].source, EnvVarSource::Local);
        assert!(app.env_var_rows[0].key.is_empty());
    }

    #[test]
    fn test_remove_env_var() {
        let mut app = create_test_app();
        app.env_var_rows.push(EnvVarRow {
            key: "A".to_string(),
            value: "1".to_string(),
            source: EnvVarSource::Local,
        });
        app.env_var_rows.push(EnvVarRow {
            key: "B".to_string(),
            value: "2".to_string(),
            source: EnvVarSource::Local,
        });
        let _ = app.update(Message::RemoveEnvVar(0));
        assert_eq!(app.env_var_rows.len(), 1);
        assert_eq!(app.env_var_rows[0].key, "B");
    }

    #[test]
    fn test_override_global_var() {
        let mut app = create_test_app();
        app.env_var_rows.push(EnvVarRow {
            key: "GK".to_string(),
            value: "gv".to_string(),
            source: EnvVarSource::Global,
        });
        let _ = app.update(Message::OverrideGlobalVar("GK".to_string()));
        assert_eq!(app.env_var_rows[0].source, EnvVarSource::Override);
    }

    #[test]
    fn test_save_env_vars_filters_globals_and_empty_keys() {
        let mut app = create_test_app();
        let env = sample_env("env-1");
        app.environments.push(env);
        app.selected_env_id = Some("env-1".to_string());

        app.env_var_rows = vec![
            EnvVarRow {
                key: "LOCAL".to_string(),
                value: "v1".to_string(),
                source: EnvVarSource::Local,
            },
            EnvVarRow {
                key: "GLOBAL".to_string(),
                value: "v2".to_string(),
                source: EnvVarSource::Global,
            },
            EnvVarRow {
                key: "".to_string(),
                value: "empty".to_string(),
                source: EnvVarSource::Local,
            },
            EnvVarRow {
                key: "OVER".to_string(),
                value: "v3".to_string(),
                source: EnvVarSource::Override,
            },
        ];

        let _ = app.update(Message::SaveEnvVars);
        let saved = &app.environments[0].env_vars;
        assert_eq!(saved.len(), 2); // LOCAL and OVER (not GLOBAL, not empty)
        assert_eq!(saved.get("LOCAL").unwrap(), "v1");
        assert_eq!(saved.get("OVER").unwrap(), "v3");
    }

    #[test]
    fn test_settings_field_changes() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsPATChanged("new-token".to_string()));
        assert_eq!(app.settings.github.pat.as_deref(), Some("new-token"));

        let _ = app.update(Message::SettingsApiBaseUrlChanged(
            "https://custom.api".to_string(),
        ));
        assert_eq!(app.settings.github.api_base_url, "https://custom.api");

        let _ = app.update(Message::SettingsDefaultOrgChanged("myorg".to_string()));
        assert_eq!(app.settings.github.default_org.as_deref(), Some("myorg"));

        let _ = app.update(Message::SettingsPythonVersionChanged("3.11".to_string()));
        assert_eq!(app.settings.defaults.python_version, "3.11");

        let _ = app.update(Message::SettingsSeedChanged(true));
        assert!(app.settings.defaults.seed);

        let _ = app.update(Message::SettingsTerminalCommandChanged(
            "alacritty".to_string(),
        ));
        assert_eq!(
            app.settings.general.terminal_command.as_deref(),
            Some("alacritty")
        );
    }

    #[test]
    fn test_settings_global_env_var_operations() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsAddGlobalEnvVar);
        assert_eq!(app.settings_global_env_var_rows.len(), 1);

        let _ = app.update(Message::SettingsGlobalEnvVarKeyChanged(
            0,
            "MY_KEY".to_string(),
        ));
        assert_eq!(app.settings_global_env_var_rows[0].0, "MY_KEY");

        let _ = app.update(Message::SettingsGlobalEnvVarValueChanged(
            0,
            "my_val".to_string(),
        ));
        assert_eq!(app.settings_global_env_var_rows[0].1, "my_val");

        let _ = app.update(Message::SettingsAddGlobalEnvVar);
        assert_eq!(app.settings_global_env_var_rows.len(), 2);

        let _ = app.update(Message::SettingsRemoveGlobalEnvVar(0));
        assert_eq!(app.settings_global_env_var_rows.len(), 1);
    }

    #[test]
    fn test_dismiss_toast() {
        let mut app = create_test_app();
        app.show_toast("msg1".to_string(), ToastVariant::Info);
        app.show_toast("msg2".to_string(), ToastVariant::Error);
        assert_eq!(app.toasts.len(), 2);

        let id = app.toasts[0].id;
        let _ = app.update(Message::DismissToast(id));
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].message, "msg2");
    }

    #[test]
    fn test_toast_tick_removes_expired() {
        let mut app = create_test_app();
        app.toasts.push(Toast {
            id: 0,
            message: "old".to_string(),
            variant: ToastVariant::Info,
            created_at: std::time::Instant::now() - std::time::Duration::from_secs(10),
        });
        app.toasts.push(Toast {
            id: 1,
            message: "new".to_string(),
            variant: ToastVariant::Info,
            created_at: std::time::Instant::now(),
        });
        let _ = app.update(Message::ToastTick);
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].message, "new");
    }

    #[test]
    fn test_uv_availability_checked_false_shows_warning() {
        let mut app = create_test_app();
        let _ = app.update(Message::UvAvailabilityChecked(false));
        assert_eq!(app.uv_available, Some(false));
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].variant, ToastVariant::Warning);
    }

    #[test]
    fn test_uv_availability_checked_true_no_warning() {
        let mut app = create_test_app();
        let _ = app.update(Message::UvAvailabilityChecked(true));
        assert_eq!(app.uv_available, Some(true));
        assert!(app.toasts.is_empty());
    }

    fn sample_pr(number: u64, author: &str) -> PullRequest {
        PullRequest {
            number,
            title: format!("PR #{}", number),
            state: orthanc_lib::models::github::PRState::Open,
            head_branch: "feature".to_string(),
            base_branch: "main".to_string(),
            html_url: format!("https://github.com/org/repo/pull/{}", number),
            created_at: Utc::now(),
            author: author.to_string(),
            repo_full_name: "org/repo".to_string(),
        }
    }

    #[test]
    fn test_org_pull_requests_loaded_success() {
        let mut app = create_test_app();
        app.loading_org_prs = true;
        let prs = vec![sample_pr(1, "alice"), sample_pr(2, "bob")];
        let _ = app.update(Message::OrgPullRequestsLoaded(Ok(prs)));
        assert!(!app.loading_org_prs);
        assert_eq!(app.org_pull_requests.len(), 2);
        assert_eq!(app.org_pull_requests[0].author, "alice");
    }

    #[test]
    fn test_org_pull_requests_loaded_error() {
        let mut app = create_test_app();
        app.loading_org_prs = true;
        let _ = app.update(Message::OrgPullRequestsLoaded(Err(
            "auth failed".to_string()
        )));
        assert!(!app.loading_org_prs);
        assert!(app.org_pull_requests.is_empty());
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_pr_filter_state_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::PrFilterStateChanged("closed".to_string()));
        assert_eq!(app.pr_filter_state, "closed");
    }

    #[test]
    fn test_pr_filter_author_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::PrFilterAuthorChanged("alice".to_string()));
        assert_eq!(app.pr_filter_author, "alice");
    }

    #[test]
    fn test_navigate_to_pull_requests() {
        let mut app = create_test_app();
        let _ = app.update(Message::NavigateTo(Page::PullRequests));
        assert_eq!(app.active_page, Page::PullRequests);
    }

    #[test]
    fn test_repo_path_picked_updates_environment() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        let _ = app.update(Message::RepoPathPicked(Some((
            "env-1".to_string(),
            "/new/path".to_string(),
        ))));
        assert_eq!(app.environments[0].repo_path, PathBuf::from("/new/path"));
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
    }

    #[test]
    fn test_repo_path_picked_none_does_nothing() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        let _ = app.update(Message::RepoPathPicked(None));
        assert_eq!(app.environments[0].repo_path, PathBuf::from("/tmp/repo"));
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn test_list_dependencies_sets_loading() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        let _ = app.update(Message::ListDependencies);
        assert!(app.loading_dependencies);
        assert!(app.dependencies.is_empty());
    }

    #[test]
    fn test_dependencies_loaded_success() {
        let mut app = create_test_app();
        app.loading_dependencies = true;
        let deps = vec![
            Dependency {
                name: "requests".to_string(),
                version: "2.31.0".to_string(),
                source: DepSource::Direct,
            },
            Dependency {
                name: "urllib3".to_string(),
                version: "2.0.4".to_string(),
                source: DepSource::Transitive,
            },
            Dependency {
                name: "pytest".to_string(),
                version: "7.4.0".to_string(),
                source: DepSource::Dev,
            },
        ];
        let _ = app.update(Message::DependenciesLoaded(Ok(deps)));
        assert!(!app.loading_dependencies);
        assert_eq!(app.dependencies.len(), 3);
        assert_eq!(app.dependencies[0].source, DepSource::Direct);
        assert_eq!(app.dependencies[1].source, DepSource::Transitive);
        assert_eq!(app.dependencies[2].source, DepSource::Dev);
    }

    #[test]
    fn test_dependencies_loaded_error() {
        let mut app = create_test_app();
        app.loading_dependencies = true;
        let _ = app.update(Message::DependenciesLoaded(Err("uv not found".to_string())));
        assert!(!app.loading_dependencies);
        assert!(app.dependencies.is_empty());
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_switch_to_dependencies_tab() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        let _ = app.update(Message::SwitchDetailsTab(DetailsTab::Dependencies));
        assert_eq!(app.active_details_tab, DetailsTab::Dependencies);
        assert!(app.loading_dependencies);
    }

    #[test]
    fn test_dep_version_editing() {
        let mut app = create_test_app();
        let _ = app.update(Message::DepVersionEditing(2, "1.5.0".to_string()));
        assert_eq!(app.dep_editing_index, Some(2));
        assert_eq!(app.dep_editing_version, "1.5.0");
    }

    #[test]
    fn test_dep_version_submit_same_version_cancels() {
        let mut app = create_test_app();
        app.dependencies = vec![Dependency {
            name: "requests".to_string(),
            version: "2.31.0".to_string(),
            source: DepSource::Direct,
        }];
        app.dep_editing_index = Some(0);
        app.dep_editing_version = "2.31.0".to_string();
        let _ = app.update(Message::DepVersionSubmit(0));
        assert!(app.dep_editing_index.is_none());
        assert!(!app.loading_dependencies);
    }

    #[test]
    fn test_dep_version_submit_empty_cancels() {
        let mut app = create_test_app();
        app.dependencies = vec![Dependency {
            name: "requests".to_string(),
            version: "2.31.0".to_string(),
            source: DepSource::Direct,
        }];
        app.dep_editing_index = Some(0);
        app.dep_editing_version = "  ".to_string();
        let _ = app.update(Message::DepVersionSubmit(0));
        assert!(app.dep_editing_index.is_none());
        assert!(!app.loading_dependencies);
    }

    #[test]
    fn test_dep_version_updated_error() {
        let mut app = create_test_app();
        app.loading_dependencies = true;
        app.dep_editing_index = Some(0);
        app.dep_editing_version = "99.0.0".to_string();
        let _ = app.update(Message::DepVersionUpdated(Err(
            "no such version".to_string()
        )));
        assert!(!app.loading_dependencies);
        assert!(app.dep_editing_index.is_none());
        assert!(app.dep_editing_version.is_empty());
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_dep_version_updated_success() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        app.loading_dependencies = true;
        let _ = app.update(Message::DepVersionUpdated(Ok("requests".to_string())));
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
        assert!(app.loading_dependencies);
    }

    #[test]
    fn test_dep_install_name_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::DepInstallNameChanged("flask".to_string()));
        assert_eq!(app.dep_install.name, "flask");
    }

    #[test]
    fn test_install_form_request_per_source() {
        let mut form = InstallForm {
            name: " requests==2.31 ".to_string(),
            path: "/src/my lib".to_string(),
            editable: true,
            git_url: "org/repo".to_string(),
            git_ref: "v1.2".to_string(),
            ..Default::default()
        };
        assert_eq!(form.request(), Some(("requests==2.31".to_string(), false)));
        form.source = InstallSource::Local;
        assert_eq!(form.request(), Some(("/src/my lib".to_string(), true)));
        form.editable = false;
        assert_eq!(form.request(), Some(("/src/my lib".to_string(), false)));
        form.source = InstallSource::GitHub;
        assert_eq!(
            form.request(),
            Some(("git+https://github.com/org/repo@v1.2".to_string(), false))
        );
        form.git_url = "  ".to_string();
        assert_eq!(form.request(), None);
    }

    #[test]
    fn test_github_spec_normalises_urls() {
        let cases = [
            ("org/repo", "", Some("git+https://github.com/org/repo")),
            (
                "github.com/org/repo/",
                "",
                Some("git+https://github.com/org/repo"),
            ),
            (
                "https://github.com/org/repo.git",
                "main",
                Some("git+https://github.com/org/repo@main"),
            ),
            (
                "git+https://gitlab.com/a/b",
                "",
                Some("git+https://gitlab.com/a/b"),
            ),
            (
                "git@github.com:org/repo.git",
                "abc123",
                Some("git+ssh://git@github.com/org/repo@abc123"),
            ),
            ("just-a-name", "", None),
            ("org/repo with space", "", None),
            ("", "", None),
        ];
        for (url, git_ref, want) in cases {
            assert_eq!(github_spec(url, git_ref).as_deref(), want, "{url}");
        }
    }

    #[test]
    fn test_dep_install_submit_clears_only_current_source() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        app.dep_install.source = InstallSource::Local;
        app.dep_install.path = "/src/lib".to_string();
        app.dep_install.name = "keep-me".to_string();
        let _ = app.update(Message::DepInstallSubmit);
        assert!(app.loading_dependencies);
        assert!(app.dep_install.path.is_empty());
        assert_eq!(app.dep_install.name, "keep-me");
    }

    #[test]
    fn test_dep_install_path_picked_cancel_keeps_path() {
        let mut app = create_test_app();
        app.dep_install.path = "/typed".to_string();
        let _ = app.update(Message::DepInstallPathPicked(None));
        assert_eq!(app.dep_install.path, "/typed");
        let _ = app.update(Message::DepInstallPathPicked(Some("/picked".to_string())));
        assert_eq!(app.dep_install.path, "/picked");
    }

    #[test]
    fn test_git_status_loaded_keeps_only_repos() {
        let mut app = create_test_app();
        app.git_status
            .insert("stale".to_string(), GitStatus::default());
        let main = GitStatus {
            branch: "main".to_string(),
            ..Default::default()
        };
        let _ = app.update(Message::GitStatusLoaded(vec![
            ("env-1".to_string(), Some(main.clone())),
            ("env-2".to_string(), None),
        ]));
        assert_eq!(app.git_status.len(), 1);
        assert_eq!(app.git_status.get("env-1"), Some(&main));
    }

    #[test]
    fn test_show_dependencies_opens_tab_for_env() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.context_menu_env_id = Some("env-1".to_string());
        let _ = app.update(Message::ShowDependencies("env-1".to_string()));
        assert_eq!(app.selected_env_id.as_deref(), Some("env-1"));
        assert_eq!(app.active_details_tab, DetailsTab::Dependencies);
        assert!(app.loading_dependencies);
        assert!(app.context_menu_env_id.is_none());
    }

    #[test]
    fn test_dep_install_submit_empty_noop() {
        let mut app = create_test_app();
        app.dep_install.name = "  ".to_string();
        let _ = app.update(Message::DepInstallSubmit);
        assert!(!app.loading_dependencies);
    }

    #[test]
    fn test_dep_installed_success() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        app.loading_dependencies = true;
        let _ = app.update(Message::DepInstalled(Ok("flask".to_string())));
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
        assert!(app.loading_dependencies);
    }

    #[test]
    fn test_dep_installed_error() {
        let mut app = create_test_app();
        app.loading_dependencies = true;
        let _ = app.update(Message::DepInstalled(Err("not found".to_string())));
        assert!(!app.loading_dependencies);
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_dep_uninstalled_success() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        app.loading_dependencies = true;
        let _ = app.update(Message::DepUninstalled(Ok("requests".to_string())));
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
        assert!(app.loading_dependencies);
    }

    #[test]
    fn test_dep_uninstalled_error() {
        let mut app = create_test_app();
        app.loading_dependencies = true;
        let _ = app.update(Message::DepUninstalled(Err("failed".to_string())));
        assert!(!app.loading_dependencies);
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_dep_click_single_does_not_load_info() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.selected_env_id = Some("env-1".to_string());
        app.dependencies = vec![Dependency {
            name: "requests".to_string(),
            version: "2.31.0".to_string(),
            source: DepSource::Direct,
        }];
        let _ = app.update(Message::DepClick(0));
        assert!(app.dep_info.is_none());
        assert!(!app.loading_dep_info);
        assert_eq!(app.dep_last_click_index, Some(0));
        assert!(app.dep_last_click_time.is_some());
    }

    #[test]
    fn test_dep_click_different_index_resets() {
        let mut app = create_test_app();
        app.dependencies = vec![
            Dependency {
                name: "a".to_string(),
                version: "1.0".to_string(),
                source: DepSource::Direct,
            },
            Dependency {
                name: "b".to_string(),
                version: "2.0".to_string(),
                source: DepSource::Direct,
            },
        ];
        let _ = app.update(Message::DepClick(0));
        let _ = app.update(Message::DepClick(1));
        // Different index, so no double-click triggered
        assert!(!app.loading_dep_info);
        assert_eq!(app.dep_last_click_index, Some(1));
    }

    #[test]
    fn test_dep_info_loaded_success() {
        let mut app = create_test_app();
        app.loading_dep_info = true;
        let info = DepInfo {
            name: "requests".to_string(),
            version: "2.31.0".to_string(),
            summary: "HTTP library".to_string(),
            location: "/tmp/venv/lib".to_string(),
            requires: vec!["urllib3".to_string(), "certifi".to_string()],
            required_by: vec!["httpx".to_string()],
        };
        let _ = app.update(Message::DepInfoLoaded(Ok(info)));
        assert!(!app.loading_dep_info);
        assert!(app.dep_info.is_some());
        let info = app.dep_info.as_ref().unwrap();
        assert_eq!(info.name, "requests");
        assert_eq!(info.requires.len(), 2);
        assert_eq!(info.required_by.len(), 1);
    }

    #[test]
    fn test_dep_info_loaded_error() {
        let mut app = create_test_app();
        app.loading_dep_info = true;
        let _ = app.update(Message::DepInfoLoaded(Err("not found".to_string())));
        assert!(!app.loading_dep_info);
        assert!(app.dep_info.is_none());
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_env_context_menu() {
        let mut app = create_test_app();
        let _ = app.update(Message::EnvContextMenu("env-1".to_string()));
        assert_eq!(app.context_menu_env_id.as_deref(), Some("env-1"));
    }

    #[test]
    fn test_hide_context_menu() {
        let mut app = create_test_app();
        app.context_menu_env_id = Some("env-1".to_string());
        let _ = app.update(Message::HideContextMenu);
        assert!(app.context_menu_env_id.is_none());
    }

    #[test]
    fn test_show_dep_tree_clears_context_menu() {
        let mut app = create_test_app();
        app.environments.push(sample_env("env-1"));
        app.context_menu_env_id = Some("env-1".to_string());
        let _ = app.update(Message::ShowDepTree("env-1".to_string()));
        assert!(app.context_menu_env_id.is_none());
        assert!(app.loading_dep_tree);
        assert!(app.dep_tree.is_none());
    }

    #[test]
    fn test_show_dep_tree_nonexistent_env() {
        let mut app = create_test_app();
        let _ = app.update(Message::ShowDepTree("nonexistent".to_string()));
        assert!(!app.loading_dep_tree);
    }

    #[test]
    fn test_dep_tree_loaded_success() {
        let mut app = create_test_app();
        app.loading_dep_tree = true;
        let tree = vec![DepTreeNode {
            name: "requests".to_string(),
            version: "2.31.0".to_string(),
            children: vec![DepTreeNode {
                name: "urllib3".to_string(),
                version: "2.0.4".to_string(),
                children: Vec::new(),
            }],
        }];
        let _ = app.update(Message::DepTreeLoaded(Ok(tree)));
        assert!(!app.loading_dep_tree);
        assert!(app.dep_tree.is_some());
        let tree = app.dep_tree.as_ref().unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 1);
    }

    #[test]
    fn test_dep_tree_loaded_error() {
        let mut app = create_test_app();
        app.loading_dep_tree = true;
        let _ = app.update(Message::DepTreeLoaded(Err("uv failed".to_string())));
        assert!(!app.loading_dep_tree);
        assert!(app.dep_tree.is_none());
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_dep_close_info() {
        let mut app = create_test_app();
        app.dep_info = Some(DepInfo {
            name: "requests".to_string(),
            version: "2.31.0".to_string(),
            summary: String::new(),
            location: String::new(),
            requires: Vec::new(),
            required_by: Vec::new(),
        });
        let _ = app.update(Message::DepCloseInfo);
        assert!(app.dep_info.is_none());
    }

    #[test]
    fn test_dependencies_loaded_clears_editing_state() {
        let mut app = create_test_app();
        app.dep_editing_index = Some(2);
        app.dep_editing_version = "1.0.0".to_string();
        app.loading_dependencies = true;
        let _ = app.update(Message::DependenciesLoaded(Ok(vec![])));
        assert!(app.dep_editing_index.is_none());
        assert!(app.dep_editing_version.is_empty());
    }

    #[test]
    fn test_normalize_dep_name() {
        assert_eq!(normalize_dep_name("requests>=2.28"), "requests");
        assert_eq!(normalize_dep_name("my-package[extra]"), "my_package");
        assert_eq!(normalize_dep_name("Flask"), "flask");
        assert_eq!(
            normalize_dep_name("python-dateutil~=2.8"),
            "python_dateutil"
        );
    }

    #[test]
    fn test_pipelines_loaded_success() {
        let mut app = create_test_app();
        app.loading_pipelines = true;
        let runs = vec![WorkflowRun {
            id: 1,
            name: "CI".to_string(),
            status: orthanc_lib::models::github::WorkflowStatus::Completed,
            conclusion: Some(orthanc_lib::models::github::WorkflowConclusion::Success),
            html_url: "https://example.com".to_string(),
            head_branch: "main".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }];
        let _ = app.update(Message::PipelinesLoaded(Ok(runs)));
        assert!(!app.loading_pipelines);
        assert_eq!(app.pipeline_runs.len(), 1);
    }

    #[test]
    fn test_pipelines_loaded_failure() {
        let mut app = create_test_app();
        app.loading_pipelines = true;
        let _ = app.update(Message::PipelinesLoaded(Err("auth failed".to_string())));
        assert!(!app.loading_pipelines);
        assert!(app.pipeline_runs.is_empty());
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_rerun_workflow_result_success() {
        let mut app = create_test_app();
        let _ = app.update(Message::RerunWorkflowResult(Ok(())));
        assert_eq!(app.toasts[0].variant, ToastVariant::Success);
    }

    #[test]
    fn test_rerun_workflow_result_failure() {
        let mut app = create_test_app();
        let _ = app.update(Message::RerunWorkflowResult(Err("forbidden".to_string())));
        assert_eq!(app.toasts[0].variant, ToastVariant::Error);
    }

    #[test]
    fn test_apply_settings_terminal_command_empty_is_none() {
        let mut app = create_test_app();
        app.settings.general.terminal_command = None;
        app.apply_settings_to_config();
        assert!(app.config.general.terminal_command.is_none());
    }

    #[test]
    fn test_apply_settings_terminal_command_set() {
        let mut app = create_test_app();
        app.settings.general.terminal_command = Some("wezterm start -- {cmd}".to_string());
        app.apply_settings_to_config();
        assert_eq!(
            app.config.general.terminal_command.as_deref(),
            Some("wezterm start -- {cmd}")
        );
    }

    #[test]
    fn test_apply_settings_default_org_empty_is_none() {
        let mut app = create_test_app();
        app.settings.github.default_org = None;
        app.apply_settings_to_config();
        assert!(app.config.github.default_org.is_none());
    }

    #[test]
    fn test_sync_settings_form_roundtrip() {
        let mut app = create_test_app();
        app.config.github.pat = Some("the-token".to_string());
        app.config.github.default_org = Some("TheOrg".to_string());
        app.config.defaults.python_version = "3.10".to_string();
        app.config.defaults.seed = true;
        app.config.general.terminal_command = Some("kitty".to_string());
        app.config
            .defaults
            .global_env_vars
            .insert("X".to_string(), "Y".to_string());

        app.sync_settings_form();

        assert_eq!(app.settings.github.pat.as_deref(), Some("the-token"));
        assert_eq!(app.settings.github.default_org.as_deref(), Some("TheOrg"));
        assert_eq!(app.settings.defaults.python_version, "3.10");
        assert!(app.settings.defaults.seed);
        assert_eq!(
            app.settings.general.terminal_command.as_deref(),
            Some("kitty")
        );
        assert_eq!(app.settings_global_env_var_rows.len(), 1);
    }

    // ── Default venv dir & uv extra args tests ──

    #[test]
    fn test_settings_default_venv_dir_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsDefaultVenvDirChanged(
            "/home/user/.venvs".to_string(),
        ));
        assert_eq!(
            app.settings.defaults.default_venv_dir.as_deref(),
            Some("/home/user/.venvs")
        );
    }

    #[test]
    fn test_settings_uv_extra_args_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsUvExtraArgsChanged(
            "--no-cache --verbose".to_string(),
        ));
        assert_eq!(app.settings_uv_extra_args, "--no-cache --verbose");
    }

    #[test]
    fn test_apply_settings_default_venv_dir() {
        let mut app = create_test_app();
        app.settings.defaults.default_venv_dir = Some("/opt/venvs".to_string());
        app.apply_settings_to_config();
        assert_eq!(
            app.config.defaults.default_venv_dir.as_deref(),
            Some("/opt/venvs")
        );
    }

    #[test]
    fn test_apply_settings_default_venv_dir_empty_clears() {
        let mut app = create_test_app();
        app.config.defaults.default_venv_dir = Some("/old/path".to_string());
        app.settings.defaults.default_venv_dir = None;
        app.apply_settings_to_config();
        assert!(app.config.defaults.default_venv_dir.is_none());
    }

    #[test]
    fn test_apply_settings_uv_extra_args_parsed() {
        let mut app = create_test_app();
        app.settings_uv_extra_args = "--no-cache  --verbose".to_string();
        app.apply_settings_to_config();
        assert_eq!(
            app.config.defaults.uv_extra_args,
            vec!["--no-cache", "--verbose"]
        );
    }

    #[test]
    fn test_apply_settings_uv_extra_args_empty() {
        let mut app = create_test_app();
        app.settings_uv_extra_args = String::new();
        app.apply_settings_to_config();
        assert!(app.config.defaults.uv_extra_args.is_empty());
    }

    #[test]
    fn test_sync_settings_form_default_venv_dir() {
        let mut app = create_test_app();
        app.config.defaults.default_venv_dir = Some("/custom/venvs".to_string());
        app.config.defaults.uv_extra_args = vec!["--no-cache".to_string(), "--verbose".to_string()];
        app.sync_settings_form();
        assert_eq!(
            app.settings.defaults.default_venv_dir.as_deref(),
            Some("/custom/venvs")
        );
        assert_eq!(app.settings_uv_extra_args, "--no-cache --verbose");
    }

    // ── UV config tests ──

    #[test]
    fn test_settings_uv_default_index_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsUvDefaultIndexChanged(
            "https://pypi.example.com/simple".to_string(),
        ));
        assert_eq!(
            app.settings.uv.default_index.as_deref(),
            Some("https://pypi.example.com/simple")
        );
    }

    #[test]
    fn test_settings_uv_index_name_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsUvIndexNameChanged("corp".to_string()));
        assert_eq!(app.settings.uv.index_name, "corp");
    }

    #[test]
    fn test_settings_uv_username_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsUvUsernameChanged("deploy-bot".to_string()));
        assert_eq!(app.settings.uv.username.as_deref(), Some("deploy-bot"));
    }

    #[test]
    fn test_settings_uv_password_changed() {
        let mut app = create_test_app();
        let _ = app.update(Message::SettingsUvPasswordChanged("s3cret".to_string()));
        assert_eq!(app.settings.uv.password.as_deref(), Some("s3cret"));
    }

    #[test]
    fn test_apply_settings_uv_config() {
        let mut app = create_test_app();
        app.settings.uv.default_index = Some("https://pypi.example.com/simple".to_string());
        app.settings.uv.index_name = "corp".to_string();
        app.settings.uv.username = Some("user1".to_string());
        app.settings.uv.password = Some("token123".to_string());

        app.apply_settings_to_config();

        assert_eq!(
            app.config.uv.default_index.as_deref(),
            Some("https://pypi.example.com/simple")
        );
        assert_eq!(app.config.uv.index_name, "corp");
        assert_eq!(app.config.uv.username.as_deref(), Some("user1"));
        assert_eq!(app.config.uv.password.as_deref(), Some("token123"));
    }

    #[test]
    fn test_apply_settings_uv_config_empty_clears() {
        let mut app = create_test_app();
        app.config.uv.default_index = Some("https://old.example.com".to_string());
        app.config.uv.username = Some("old-user".to_string());
        app.config.uv.password = Some("old-pass".to_string());

        // Empty form fields should clear config
        app.settings.uv.default_index = None;
        app.settings.uv.index_name = String::new();
        app.settings.uv.username = None;
        app.settings.uv.password = None;

        app.apply_settings_to_config();

        assert!(app.config.uv.default_index.is_none());
        assert_eq!(app.config.uv.index_name, "private"); // defaults back
        assert!(app.config.uv.username.is_none());
        assert!(app.config.uv.password.is_none());
    }

    #[test]
    fn test_sync_settings_form_uv_config() {
        let mut app = create_test_app();
        app.config.uv.default_index = Some("https://pypi.example.com/simple".to_string());
        app.config.uv.index_name = "mirror".to_string();
        app.config.uv.username = Some("ci-bot".to_string());
        app.config.uv.password = Some("jwt-token".to_string());

        app.sync_settings_form();

        assert_eq!(
            app.settings.uv.default_index.as_deref(),
            Some("https://pypi.example.com/simple")
        );
        assert_eq!(app.settings.uv.index_name, "mirror");
        assert_eq!(app.settings.uv.username.as_deref(), Some("ci-bot"));
        assert_eq!(app.settings.uv.password.as_deref(), Some("jwt-token"));
    }

    // ── Self-update tests ──

    #[test]
    fn test_update_check_result_none_no_update() {
        let mut app = create_test_app();
        let _ = app.update(Message::UpdateCheckResult(Ok(None)));
        assert!(app.update_available.is_none());
    }

    #[test]
    fn test_update_check_result_some_sets_available() {
        let mut app = create_test_app();
        let info = updater::ReleaseInfo {
            tag: "v0.2.0".to_string(),
            version: "0.2.0".to_string(),
            download_url: Some("https://example.com/download".to_string()),
            asset_name: Some("orthanc-linux-x86_64".to_string()),
            body: "release notes".to_string(),
        };
        let _ = app.update(Message::UpdateCheckResult(Ok(Some(info))));
        assert!(app.update_available.is_some());
        assert_eq!(app.update_available.as_ref().unwrap().version, "0.2.0");
        // Should show a toast
        assert!(!app.toasts.is_empty());
    }

    #[test]
    fn test_update_check_result_error_logs_but_no_crash() {
        let mut app = create_test_app();
        let _ = app.update(Message::UpdateCheckResult(Err("network error".to_string())));
        assert!(app.update_available.is_none());
    }

    #[test]
    fn test_download_update_sets_downloading() {
        let mut app = create_test_app();
        let _ = app.update(Message::DownloadUpdate(
            "https://example.com/orthanc".to_string(),
        ));
        assert!(app.update_downloading);
    }

    #[test]
    fn test_download_update_ignores_double_click() {
        let mut app = create_test_app();
        app.update_downloading = true;
        let _ = app.update(Message::DownloadUpdate(
            "https://example.com/orthanc".to_string(),
        ));
        assert!(app.update_downloading); // still true, no double download
    }

    #[test]
    fn test_update_downloaded_success() {
        let mut app = create_test_app();
        app.update_downloading = true;
        app.update_available = Some(updater::ReleaseInfo {
            tag: "v0.2.0".to_string(),
            version: "0.2.0".to_string(),
            download_url: Some("https://example.com".to_string()),
            asset_name: None,
            body: String::new(),
        });

        let _ = app.update(Message::UpdateDownloaded(Ok(())));
        assert!(!app.update_downloading);
        assert!(app.update_available.is_none());
        assert!(app.toasts.iter().any(|t| t.message.contains("Restart")));
    }

    #[test]
    fn test_update_downloaded_failure() {
        let mut app = create_test_app();
        app.update_downloading = true;

        let _ = app.update(Message::UpdateDownloaded(Err(
            "permission denied".to_string()
        )));
        assert!(!app.update_downloading);
        assert!(app
            .toasts
            .iter()
            .any(|t| t.message.contains("Update failed")));
    }

    // ── Helper ──────────────────────────────────────────────────────────

    fn sample_env(id: &str) -> PythonEnvironment {
        PythonEnvironment {
            id: id.to_string(),
            name: format!("env-{}", id),
            repo_path: PathBuf::from("/tmp/repo"),
            venv_path: PathBuf::from("/tmp/VirtualEnvironments/repo"),
            python_version: "3.12".to_string(),
            status: EnvStatus::Ready,
            created_at: Utc::now(),
            last_synced_at: None,
            staleness: StalenessInfo::default(),
            env_vars: HashMap::new(),
        }
    }

    #[test]
    fn test_tray_show_opens_window() {
        let mut app = create_test_app();
        app.main_window_id = None;
        let _ = app.update(Message::TrayShow);
        assert!(app.main_window_id.is_some());
    }

    #[test]
    fn test_tray_show_when_already_open() {
        let mut app = create_test_app();
        app.main_window_id = Some(window::Id::unique());
        let old_id = app.main_window_id;
        let _ = app.update(Message::TrayShow);
        // Window ID should remain the same (focus, not new window)
        assert_eq!(app.main_window_id, old_id);
    }

    #[test]
    fn test_tray_open_terminal_delegates_to_open_terminal() {
        let mut app = create_test_app();
        let env = sample_env("env1");
        app.environments.push(env);
        // TrayOpenTerminal should behave like OpenTerminal
        // (won't actually open a terminal in test, but should not panic)
        let _ = app.update(Message::TrayOpenTerminal("env1".to_string()));
    }

    #[test]
    fn test_open_pycharm_unknown_env_is_noop() {
        let mut app = create_test_app();
        let toasts = app.toasts.len();
        let _ = app.update(Message::OpenPycharm("missing".to_string()));
        assert_eq!(app.toasts.len(), toasts);
    }

    #[test]
    fn test_sync_tray_envs_builds_correct_entries() {
        let mut app = create_test_app();
        app.environments.push(sample_env("e1"));
        app.environments.push(sample_env("e2"));
        // sync_tray_envs should not panic even without a real tray
        app.sync_tray_envs();
    }

    #[test]
    fn test_window_close_minimizes_to_tray() {
        let mut app = create_test_app();
        let main_id = window::Id::unique();
        app.main_window_id = Some(main_id);
        let _ = app.update(Message::GraphWindowClosed(main_id));
        // Main window should be cleared (minimized to tray), not exiting
        assert!(app.main_window_id.is_none());
    }

    fn create_test_app() -> Orthanc {
        Orthanc {
            config: AppConfig::default(),
            config_path: PathBuf::from("/tmp/test-orthanc.toml"),
            environments: Vec::new(),
            github_client: None,
            main_window_id: None,
            graph_window_id: None,
            active_page: Page::Environments,
            selected_env_id: None,
            uv_available: None,
            show_create_dialog: false,
            create_repo_path: String::new(),
            create_env_name: String::new(),
            create_env_name_touched: false,
            create_venv_path: String::new(),
            create_python_version: String::new(),
            creating: false,
            confirm_delete_env_id: None,
            active_details_tab: DetailsTab::Info,
            env_var_rows: Vec::new(),
            global_env_vars_cache: HashMap::new(),
            dependencies: Vec::new(),
            loading_dependencies: false,
            dep_editing_index: None,
            dep_editing_version: String::new(),
            dep_install: InstallForm::default(),
            git_status: HashMap::new(),
            dep_last_click_index: None,
            dep_last_click_time: None,
            dep_info: None,
            loading_dep_info: false,
            context_menu_env_id: None,
            dep_tree: None,
            loading_dep_tree: false,
            pipeline_runs: Vec::new(),
            loading_pipelines: false,
            org_pull_requests: Vec::new(),
            loading_org_prs: false,
            pr_filter_state: "open".to_string(),
            pr_filter_author: String::new(),
            settings: AppConfig::default(),
            settings_uv_extra_args: String::new(),
            settings_global_env_var_rows: Vec::new(),
            update_available: None,
            update_downloading: false,
            toasts: Vec::new(),
            next_toast_id: 0,
        }
    }
}
