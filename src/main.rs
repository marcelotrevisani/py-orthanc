// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

mod app;
mod icon;
mod tray;
mod ui;

use app::Orthanc;
use iced::daemon;

fn main() -> iced::Result {
    // Work around iced 0.13 defaulting to LowPower GPU, which causes
    // "Timeout when presenting surface" on many Linux systems.
    // Must stay the first thing in main: set_var is only sound while no other
    // thread exists (it becomes `unsafe` in edition 2024 for that reason).
    if std::env::var("WGPU_POWER_PREF").is_err() {
        std::env::set_var("WGPU_POWER_PREF", "high");
    }

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(
            "info,zbus=warn,iced_wgpu=warn,iced_winit=warn,wgpu_hal=error,wgpu_hal::gles::egl=off,wgpu_core=warn,tracing=warn,sctk_adwaita=error",
        ),
    )
    .init();

    log::info!("Orthanc application starting up");

    #[cfg(target_os = "linux")]
    icon::install_desktop_entry();

    daemon(Orthanc::title, Orthanc::update, Orthanc::view)
        .subscription(Orthanc::subscription)
        .theme(Orthanc::theme)
        .run_with(Orthanc::new)
}
