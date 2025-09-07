use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, warn, error};

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Server URL for clipboard synchronization
    pub server_url: String,
    /// Whether synchronization is currently paused
    pub sync_paused: bool,
    /// Window position and size settings
    pub window: WindowConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Remember window position
    pub remember_position: bool,
    /// Last window position (x, y)
    pub position: Option<(f32, f32)>,
    /// Last window size (width, height)
    pub size: Option<(f32, f32)>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            sync_paused: false,
            window: WindowConfig::default(),
        }
    }
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            remember_position: true,
            position: None,
            size: Some((400.0, 200.0)),
        }
    }
}

impl Config {
    /// Get the config file path
    pub fn config_file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|mut path| {
            path.push("shared-clipboard");
            path.push("config.toml");
            path
        })
    }

    /// Load configuration from file
    pub fn load() -> Self {
        let config_path = match Self::config_file_path() {
            Some(path) => path,
            None => {
                warn!("Could not determine config directory");
                return Self::default();
            }
        };

        if !config_path.exists() {
            debug!("Config file doesn't exist: {:?}", config_path);
            return Self::default();
        }

        match fs::read_to_string(&config_path) {
            Ok(content) => {
                match toml::from_str::<Config>(&content) {
                    Ok(config) => {
                        debug!("Loaded configuration from {:?}", config_path);
                        config
                    }
                    Err(e) => {
                        error!("Failed to parse config file: {}", e);
                        Self::default()
                    }
                }
            }
            Err(e) => {
                error!("Failed to read config file: {}", e);
                Self::default()
            }
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config_path = Self::config_file_path()
            .ok_or("Could not determine config directory")?;

        // Create config directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(&config_path, content)?;
        
        debug!("Saved configuration to {:?}", config_path);
        Ok(())
    }

    /// Check if this is the first run (no server URL configured)
    pub fn is_first_run(&self) -> bool {
        self.server_url.is_empty()
    }

    /// Update server URL and save
    pub fn set_server_url(&mut self, url: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.server_url = url;
        self.save()
    }

    /// Toggle sync pause state and save
    pub fn toggle_sync_pause(&mut self) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        self.sync_paused = !self.sync_paused;
        self.save()?;
        Ok(self.sync_paused)
    }

    /// Update window settings
    pub fn update_window_config(&mut self, position: Option<(f32, f32)>, size: Option<(f32, f32)>) {
        if let Some(pos) = position {
            self.window.position = Some(pos);
        }
        if let Some(size) = size {
            self.window.size = Some(size);
        }
        // Save in background, ignore errors for window state
        let _ = self.save();
    }
}
