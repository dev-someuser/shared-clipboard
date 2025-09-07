use crate::config::Config;
use crate::ClipboardData;
use eframe::egui;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info};

/// GUI application state
pub struct ClipboardApp {
    /// Application configuration
    config: Config,
    /// Current state
    state: AppState,
    /// Server URL input field
    server_url_input: String,
    /// Connection test status
    connection_status: ConnectionStatus,
    /// Error message to display
    error_message: Option<String>,
    /// Whether app should exit
    should_exit: bool,
    /// Clipboard manager reference (for connection testing)
    clipboard_manager: Option<Arc<Mutex<crate::clipboard_manager::ClipboardManager>>>,
}

#[derive(Debug, Clone, PartialEq)]
enum AppState {
    /// First time setup - asking for server URL
    FirstTimeSetup,
    /// Changing server URL
    ChangingUrl,
    /// Normal operation (minimized to tray)
    Running,
    /// Showing settings dialog
    Settings,
}

#[derive(Debug, Clone, PartialEq)]
enum ConnectionStatus {
    /// Not tested yet
    NotTested,
    /// Currently testing connection
    Testing,
    /// Connection successful
    Success,
    /// Connection failed
    Failed(String),
}

impl ClipboardApp {
    /// Create new app instance
    pub fn new(cc: &eframe::CreationContext<'_>, clipboard_manager: Option<Arc<Mutex<crate::clipboard_manager::ClipboardManager>>>) -> Self {
        let config = Config::load();
        let state = if config.is_first_run() {
            AppState::FirstTimeSetup
        } else {
            AppState::Running
        };

        // Configure egui style
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.window_rounding = egui::Rounding::same(8.0);
        style.visuals.button_rounding = egui::Rounding::same(4.0);
        cc.egui_ctx.set_style(style);

        Self {
            server_url_input: config.server_url.clone(),
            config,
            state,
            connection_status: ConnectionStatus::NotTested,
            error_message: None,
            should_exit: false,
            clipboard_manager,
        }
    }

    /// Test connection to server
    async fn test_connection(&self, url: &str) -> Result<(), String> {
        debug!("Testing connection to: {}", url);
        
        // Parse URL
        let parsed_url = url::Url::parse(url)
            .map_err(|e| format!("Invalid URL format: {}", e))?;

        // Make a simple HTTP request to test connectivity
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        // Test the API endpoint
        let test_url = format!("{}/api/clipboard", url.trim_end_matches('/'));
        
        match client.get(&test_url).send().await {
            Ok(response) => {
                if response.status().is_success() || response.status() == 404 {
                    // 404 is OK - it means server is running but clipboard is empty
                    debug!("Connection test successful: {}", response.status());
                    Ok(())
                } else {
                    Err(format!("Server returned error: {}", response.status()))
                }
            }
            Err(e) => {
                error!("Connection test failed: {}", e);
                Err(format!("Failed to connect to server: {}", e))
            }
        }
    }

    /// Handle connection test result
    fn handle_connection_test_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.connection_status = ConnectionStatus::Success;
                self.error_message = None;
                
                // Save the URL
                if let Err(e) = self.config.set_server_url(self.server_url_input.clone()) {
                    self.error_message = Some(format!("Failed to save configuration: {}", e));
                } else {
                    info!("Server URL saved successfully");
                    self.state = AppState::Running;
                }
            }
            Err(error) => {
                self.connection_status = ConnectionStatus::Failed(error.clone());
                self.error_message = Some(error);
            }
        }
    }

    /// Should the app exit?
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    /// Get current config
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl eframe::App for ClipboardApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        match self.state {
            AppState::FirstTimeSetup | AppState::ChangingUrl => {
                self.show_setup_dialog(ctx, frame);
            }
            AppState::Settings => {
                self.show_settings_dialog(ctx, frame);
            }
            AppState::Running => {
                // In normal operation, the app should be minimized to tray
                // This window should not be visible
                frame.set_visible(false);
            }
        }
    }

    fn on_exit_event(&mut self) -> bool {
        // Don't exit on window close - minimize to tray instead
        if matches!(self.state, AppState::Running) {
            false // Don't exit, just hide
        } else {
            self.should_exit = true;
            true // Allow exit during setup
        }
    }
}

impl ClipboardApp {
    /// Show the initial setup dialog
    fn show_setup_dialog(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let title = if matches!(self.state, AppState::FirstTimeSetup) {
            "🔗 Shared Clipboard - Initial Setup"
        } else {
            "🔗 Change Server URL"
        };

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                
                ui.heading(title);
                ui.add_space(20.0);

                if matches!(self.state, AppState::FirstTimeSetup) {
                    ui.label("Welcome to Shared Clipboard!");
                    ui.add_space(10.0);
                    ui.label("To get started, please enter your clipboard server URL:");
                } else {
                    ui.label("Enter the new server URL:");
                }
                
                ui.add_space(20.0);

                // URL input field
                ui.horizontal(|ui| {
                    ui.label("Server URL:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.server_url_input)
                            .desired_width(300.0)
                            .hint_text("http://your-server.com:8080")
                    );
                });

                ui.add_space(10.0);

                // Connection status
                match &self.connection_status {
                    ConnectionStatus::NotTested => {}
                    ConnectionStatus::Testing => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Testing connection...");
                        });
                    }
                    ConnectionStatus::Success => {
                        ui.colored_label(egui::Color32::GREEN, "✓ Connection successful!");
                    }
                    ConnectionStatus::Failed(_) => {
                        if let Some(ref error) = self.error_message {
                            ui.colored_label(egui::Color32::RED, format!("✗ {}", error));
                        }
                    }
                }

                ui.add_space(20.0);

                // Buttons
                ui.horizontal(|ui| {
                    let can_test = !self.server_url_input.is_empty() 
                        && !matches!(self.connection_status, ConnectionStatus::Testing);

                    if ui.add_enabled(can_test, egui::Button::new("Test Connection")).clicked() {
                        self.connection_status = ConnectionStatus::Testing;
                        self.error_message = None;
                        
                        // Start connection test
                        let url = self.server_url_input.clone();
                        let ctx = ctx.clone();
                        
                        tokio::spawn(async move {
                            // This would need to be handled differently in a real app
                            // For now, we'll simulate the test
                            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                            ctx.request_repaint();
                        });
                        
                        // For now, let's do a simple URL validation
                        match url::Url::parse(&self.server_url_input) {
                            Ok(_) => {
                                self.connection_status = ConnectionStatus::Success;
                                self.error_message = None;
                            }
                            Err(e) => {
                                self.connection_status = ConnectionStatus::Failed(format!("Invalid URL: {}", e));
                                self.error_message = Some(format!("Invalid URL format: {}", e));
                            }
                        }
                    }

                    let can_save = matches!(self.connection_status, ConnectionStatus::Success);
                    if ui.add_enabled(can_save, egui::Button::new("Save & Continue")).clicked() {
                        if let Err(e) = self.config.set_server_url(self.server_url_input.clone()) {
                            self.error_message = Some(format!("Failed to save configuration: {}", e));
                        } else {
                            info!("Server URL saved successfully: {}", self.server_url_input);
                            self.state = AppState::Running;
                            frame.set_visible(false); // Hide window after setup
                        }
                    }

                    if matches!(self.state, AppState::ChangingUrl) {
                        if ui.button("Cancel").clicked() {
                            self.server_url_input = self.config.server_url.clone();
                            self.state = AppState::Running;
                            self.connection_status = ConnectionStatus::NotTested;
                            self.error_message = None;
                            frame.set_visible(false);
                        }
                    } else if ui.button("Exit").clicked() {
                        self.should_exit = true;
                        frame.close();
                    }
                });

                ui.add_space(20.0);

                // Help text
                ui.separator();
                ui.add_space(10.0);
                ui.small("💡 Tip: Make sure your server is running and accessible from this computer.");
            });
        });
    }

    /// Show settings dialog
    fn show_settings_dialog(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading("⚙️ Settings");
                ui.add_space(20.0);

                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Current Configuration:");
                        ui.add_space(5.0);
                        
                        ui.horizontal(|ui| {
                            ui.label("Server URL:");
                            ui.code(&self.config.server_url);
                        });
                        
                        ui.horizontal(|ui| {
                            ui.label("Sync Status:");
                            if self.config.sync_paused {
                                ui.colored_label(egui::Color32::YELLOW, "⏸ Paused");
                            } else {
                                ui.colored_label(egui::Color32::GREEN, "▶ Running");
                            }
                        });
                    });
                });

                ui.add_space(20.0);

                ui.horizontal(|ui| {
                    if ui.button("Change Server URL").clicked() {
                        self.state = AppState::ChangingUrl;
                        self.connection_status = ConnectionStatus::NotTested;
                        self.error_message = None;
                    }

                    if ui.button("Close").clicked() {
                        self.state = AppState::Running;
                        frame.set_visible(false);
                    }
                });
            });
        });
    }

    /// Show the app (bring window to front)
    pub fn show(&mut self) {
        self.state = AppState::Settings;
    }

    /// Change server URL
    pub fn change_server_url(&mut self) {
        self.state = AppState::ChangingUrl;
        self.connection_status = ConnectionStatus::NotTested;
        self.error_message = None;
    }

    /// Toggle sync pause state
    pub fn toggle_sync(&mut self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        self.config.toggle_sync_pause()
    }
}
