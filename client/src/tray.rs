use crate::gui::ClipboardApp;
use crate::icon::{get_icon_bytes, ICON_WIDTH, ICON_HEIGHT};
use std::sync::{Arc, Mutex};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder, Icon,
};
use tracing::{debug, error, info};

/// System tray manager
pub struct SystemTray {
    _tray_icon: TrayIcon,
    menu_receiver: std::sync::mpsc::Receiver<MenuEvent>,
    pause_resume_item: MenuItem,
    is_paused: bool,
}

impl SystemTray {
    /// Create new system tray
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create tray icon (using a simple ASCII art icon for now)
        let icon = Self::create_icon()?;

        // Create menu items
        let pause_resume_item = MenuItem::new("⏸ Pause Sync", true, None);
        let change_url_item = MenuItem::new("🔗 Change Server URL", true, None);
        let settings_item = MenuItem::new("⚙️ Settings", true, None);
        let separator = MenuItem::separator();
        let exit_item = MenuItem::new("❌ Exit", true, None);

        // Create menu
        let menu = Menu::with_items(&[
            &pause_resume_item,
            &separator,
            &change_url_item,
            &settings_item,
            &separator,
            &exit_item,
        ])?;

        // Create tray icon
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Shared Clipboard")
            .with_icon(icon)
            .build()?;

        let menu_receiver = MenuEvent::receiver();

        info!("System tray created successfully");

        Ok(Self {
            _tray_icon: tray_icon,
            menu_receiver,
            pause_resume_item,
            is_paused: false,
        })
    }

    /// Create a simple icon for the tray
    fn create_icon() -> Result<Icon, Box<dyn std::error::Error + Send + Sync>> {
        let rgba = get_icon_bytes();
        Icon::from_rgba(rgba, ICON_WIDTH, ICON_HEIGHT)
            .map_err(|e| format!("Failed to create tray icon: {:?}", e).into())
    }

    /// Process tray events and return the action to take
    pub fn process_events(&mut self) -> Option<TrayAction> {
        while let Ok(event) = self.menu_receiver.try_recv() {
            debug!("Received tray menu event: {:?}", event);

            match event.id.0.as_str() {
                id if id == self.pause_resume_item.id().0 => {
                    self.is_paused = !self.is_paused;
                    self.update_pause_resume_text();
                    return Some(TrayAction::ToggleSync);
                }
                "🔗 Change Server URL" => {
                    return Some(TrayAction::ChangeServerUrl);
                }
                "⚙️ Settings" => {
                    return Some(TrayAction::ShowSettings);
                }
                "❌ Exit" => {
                    return Some(TrayAction::Exit);
                }
                _ => {}
            }
        }
        None
    }

    /// Update pause/resume menu item text
    fn update_pause_resume_text(&self) {
        let new_text = if self.is_paused {
            "▶ Resume Sync"
        } else {
            "⏸ Pause Sync"
        };
        
        if let Err(e) = self.pause_resume_item.set_text(new_text) {
            error!("Failed to update menu item text: {}", e);
        }
    }

    /// Update sync status
    pub fn set_sync_paused(&mut self, paused: bool) {
        if self.is_paused != paused {
            self.is_paused = paused;
            self.update_pause_resume_text();
        }
    }

    /// Update tray tooltip with server info
    pub fn update_tooltip(&self, server_url: &str, is_connected: bool) {
        let status = if is_connected { "Connected" } else { "Disconnected" };
        let tooltip = format!("Shared Clipboard - {} to {}", status, server_url);
        
        if let Err(e) = self._tray_icon.set_tooltip(Some(tooltip)) {
            error!("Failed to update tray tooltip: {}", e);
        }
    }
}

/// Actions that can be triggered from the system tray
#[derive(Debug, Clone, PartialEq)]
pub enum TrayAction {
    /// Toggle sync pause/resume
    ToggleSync,
    /// Change server URL
    ChangeServerUrl,
    /// Show settings dialog
    ShowSettings,
    /// Exit application
    Exit,
}

/// System tray event handler
pub struct TrayEventHandler {
    tray: SystemTray,
    app: Arc<Mutex<ClipboardApp>>,
}

impl TrayEventHandler {
    /// Create new tray event handler
    pub fn new(app: Arc<Mutex<ClipboardApp>>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let tray = SystemTray::new()?;
        
        // Initialize tray state from app config
        if let Ok(app_guard) = app.lock() {
            let config = app_guard.config();
            // Update tray tooltip with current server
            if !config.server_url.is_empty() {
                tray.update_tooltip(&config.server_url, false); // We don't know connection status yet
            }
        }

        Ok(Self { tray, app })
    }

    /// Process tray events
    pub fn process_events(&mut self) -> bool {
        if let Some(action) = self.tray.process_events() {
            match action {
                TrayAction::ToggleSync => {
                    if let Ok(mut app) = self.app.lock() {
                        match app.toggle_sync() {
                            Ok(paused) => {
                                self.tray.set_sync_paused(paused);
                                let status = if paused { "paused" } else { "resumed" };
                                info!("Sync {}", status);
                            }
                            Err(e) => {
                                error!("Failed to toggle sync: {}", e);
                            }
                        }
                    }
                }
                TrayAction::ChangeServerUrl => {
                    if let Ok(mut app) = self.app.lock() {
                        app.change_server_url();
                    }
                }
                TrayAction::ShowSettings => {
                    if let Ok(mut app) = self.app.lock() {
                        app.show();
                    }
                }
                TrayAction::Exit => {
                    info!("Exit requested from system tray");
                    return true; // Signal to exit
                }
            }
        }
        false // Continue running
    }

    /// Update tray state from app
    pub fn update_from_app(&mut self) {
        if let Ok(app) = self.app.lock() {
            let config = app.config();
            self.tray.set_sync_paused(config.sync_paused);
            
            if !config.server_url.is_empty() {
                self.tray.update_tooltip(&config.server_url, true); // Assume connected for now
            }
        }
    }
}
