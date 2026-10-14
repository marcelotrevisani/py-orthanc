use std::cell::RefCell;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Quit,
    OpenTerminal(String), // env_id
}

/// An environment entry for the tray submenu.
#[derive(Debug, Clone)]
pub struct TrayEnvEntry {
    pub id: String,
    pub name: String,
}

/// Channel receiver for tray actions, polled by the Iced subscription.
static TRAY_RECEIVER: OnceLock<Mutex<mpsc::Receiver<TrayAction>>> = OnceLock::new();

thread_local! {
    /// The tray icon lives on the main (UI) thread: the macOS/Windows handles
    /// are `!Send`, and every caller below runs from the iced update loop.
    /// Outer `None` = not started yet; inner `None` = no tray on this desktop.
    static TRAY: RefCell<Option<Option<TrayGuard>>> = const { RefCell::new(None) };
}

/// Poll for a tray action. Safe to call from any thread.
pub fn poll_tray_action() -> Option<TrayAction> {
    let lock = TRAY_RECEIVER.get()?.lock().ok()?;
    lock.try_recv().ok()
}

/// Update the tray's environment list. Called from the app whenever
/// environments are created, deleted, or loaded.
pub fn update_tray_envs(envs: Vec<TrayEnvEntry>) {
    TRAY.with_borrow(|t| {
        if let Some(Some(g)) = t {
            g.update_envs(envs);
        }
    });
}

/// Start the tray icon once (if the desktop supports one). Returns true on the
/// call that started it. Must run inside the event loop: macOS ignores a tray
/// created before the app has launched.
pub fn init() -> bool {
    TRAY.with_borrow_mut(|t| {
        if t.is_some() {
            return false;
        }
        *t = Some(platform::init_tray());
        true
    })
}

/// Whether a tray icon is showing (the app can then live without a window).
pub fn tray_available() -> bool {
    TRAY.with_borrow(|t| matches!(t, Some(Some(_))))
}

// ── Linux: ksni (pure D-Bus, no GTK needed) ────────────────────────────

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    struct OrthancTray {
        tx: mpsc::SyncSender<TrayAction>,
        envs: Vec<TrayEnvEntry>,
    }

    impl ksni::Tray for OrthancTray {
        fn id(&self) -> String {
            "orthanc".into()
        }

        fn title(&self) -> String {
            "Orthanc".into()
        }

        fn icon_name(&self) -> String {
            String::new() // empty so icon_pixmap is used instead
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            vec![create_ksni_icon()]
        }

        // Primary activation (double-click on GNOME's AppIndicator, click on KDE).
        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.tx.try_send(TrayAction::Show);
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: "Orthanc — Python Environment Manager".into(),
                ..Default::default()
            }
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;

            let mut items: Vec<ksni::MenuItem<Self>> = vec![StandardItem {
                label: "Show Orthanc".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.try_send(TrayAction::Show);
                }),
                ..Default::default()
            }
            .into()];

            // Build "Env Terminal" submenu from current environment list
            if !self.envs.is_empty() {
                let env_items: Vec<ksni::MenuItem<Self>> = self
                    .envs
                    .iter()
                    .map(|entry| {
                        let env_id = entry.id.clone();
                        StandardItem {
                            label: entry.name.clone(),
                            icon_name: "utilities-terminal".into(),
                            activate: Box::new(move |tray: &mut Self| {
                                let _ = tray.tx.try_send(TrayAction::OpenTerminal(env_id.clone()));
                            }),
                            ..Default::default()
                        }
                        .into()
                    })
                    .collect();

                items.push(ksni::MenuItem::Separator);
                items.push(
                    SubMenu {
                        label: "Env Terminal".into(),
                        icon_name: "utilities-terminal".into(),
                        submenu: env_items,
                        ..Default::default()
                    }
                    .into(),
                );
            }

            items.push(ksni::MenuItem::Separator);
            items.push(
                StandardItem {
                    label: "Quit".into(),
                    icon_name: "application-exit".into(),
                    activate: Box::new(|tray: &mut Self| {
                        let _ = tray.tx.try_send(TrayAction::Quit);
                    }),
                    ..Default::default()
                }
                .into(),
            );

            items
        }
    }

    fn create_ksni_icon() -> ksni::Icon {
        let size = 32;
        let data = crate::icon::orthanc_icon_argb(size);
        ksni::Icon {
            width: size as i32,
            height: size as i32,
            data,
        }
    }

    pub struct TrayGuard {
        handle: ksni::blocking::Handle<OrthancTray>,
    }

    impl TrayGuard {
        pub fn update_envs(&self, envs: Vec<TrayEnvEntry>) {
            self.handle.update(|tray| {
                tray.envs = envs;
            });
        }
    }

    pub fn init_tray() -> Option<TrayGuard> {
        let (tx, rx) = mpsc::sync_channel(16);
        TRAY_RECEIVER
            .set(Mutex::new(rx))
            .expect("Tray receiver already set");

        use ksni::blocking::TrayMethods;
        let handle = OrthancTray {
            tx,
            envs: Vec::new(),
        }
        .spawn()
        .map_err(|e| log::warn!("System tray unavailable: {}", e))
        .ok()?;

        Some(TrayGuard { handle })
    }
}

// ── Windows / macOS: tray-icon ─────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;
    use std::collections::HashMap;
    use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{TrayIconBuilder, TrayIconEvent};

    static SHOW_ID: OnceLock<MenuId> = OnceLock::new();
    static QUIT_ID: OnceLock<MenuId> = OnceLock::new();
    /// Maps MenuId -> env_id for dynamic environment menu items.
    static ENV_MENU_IDS: OnceLock<Mutex<HashMap<MenuId, String>>> = OnceLock::new();

    pub struct TrayGuard {
        _tray_icon: tray_icon::TrayIcon,
        env_submenu: Submenu,
    }

    impl TrayGuard {
        pub fn update_envs(&self, envs: Vec<TrayEnvEntry>) {
            // Clear and rebuild the submenu items
            // Remove old items by rebuilding
            let mut env_ids = ENV_MENU_IDS
                .get()
                .expect("ENV_MENU_IDS not initialized")
                .lock()
                .unwrap();
            env_ids.clear();

            // Remove all existing items from submenu
            while self.env_submenu.remove_at(0).is_some() {}

            // Add new items
            for entry in &envs {
                let item = MenuItem::new(&entry.name, true, None);
                env_ids.insert(item.id().clone(), entry.id.clone());
                let _ = self.env_submenu.append(&item);
            }

            if envs.is_empty() {
                let empty = MenuItem::new("(no environments)", false, None);
                let _ = self.env_submenu.append(&empty);
            }
        }
    }

    pub fn init_tray() -> Option<TrayGuard> {
        ENV_MENU_IDS
            .set(Mutex::new(HashMap::new()))
            .expect("ENV_MENU_IDS already set");

        let tray_menu = Menu::new();
        let show_item = MenuItem::new("Show Orthanc", true, None);
        let env_submenu = Submenu::new("Env Terminal", true);
        let empty_item = MenuItem::new("(no environments)", false, None);
        let _ = env_submenu.append(&empty_item);
        let quit_item = MenuItem::new("Quit", true, None);

        SHOW_ID
            .set(show_item.id().clone())
            .expect("SHOW_ID already set");
        QUIT_ID
            .set(quit_item.id().clone())
            .expect("QUIT_ID already set");

        tray_menu
            .append_items(&[
                &show_item,
                &PredefinedMenuItem::separator(),
                &env_submenu,
                &PredefinedMenuItem::separator(),
                &quit_item,
            ])
            .map_err(|e| log::warn!("System tray menu unavailable: {}", e))
            .ok()?;

        let icon = create_tray_icon_image();

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            // Windows: menu on right click only, so a double-click can open the window.
            .with_menu_on_left_click(cfg!(not(target_os = "windows")))
            .with_tooltip("Orthanc — Python Environment Manager")
            .with_icon(icon)
            .build()
            .map_err(|e| log::warn!("System tray unavailable: {}", e))
            .ok()?;

        // Spawn a thread to forward menu events into the shared channel
        let (tx, rx) = mpsc::sync_channel(16);
        TRAY_RECEIVER
            .set(Mutex::new(rx))
            .expect("Tray receiver already set");

        // Double-click opens the window (Windows only; macOS has no such event).
        let click_tx = tx.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if let TrayIconEvent::DoubleClick { .. } = event {
                let _ = click_tx.try_send(TrayAction::Show);
            }
        }));

        std::thread::spawn(move || {
            let menu_rx = MenuEvent::receiver();
            loop {
                if let Ok(event) = menu_rx.recv() {
                    let show_id = SHOW_ID.get().unwrap();
                    let quit_id = QUIT_ID.get().unwrap();

                    let action = if event.id == *show_id {
                        Some(TrayAction::Show)
                    } else if event.id == *quit_id {
                        Some(TrayAction::Quit)
                    } else {
                        // Check if it's an environment menu item
                        let env_ids = ENV_MENU_IDS.get().unwrap().lock().unwrap();
                        env_ids
                            .get(&event.id)
                            .map(|env_id| TrayAction::OpenTerminal(env_id.clone()))
                    };

                    if let Some(action) = action {
                        if tx.send(action).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Some(TrayGuard {
            _tray_icon: tray_icon,
            env_submenu,
        })
    }

    fn create_tray_icon_image() -> tray_icon::Icon {
        let size = 32u32;
        let rgba = crate::icon::orthanc_icon_rgba(size);
        tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create tray icon image")
    }
}

// Re-export the platform-specific public API
use platform::TrayGuard;
