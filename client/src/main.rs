//! Shared Clipboard Client - Testing Version
//! 
//! Simple CLI version for testing core functionality

use tracing::{info, error};
use serde::{Deserialize, Serialize};

// Application modules
mod clipboard_manager;
mod config;
//mod gui;
//mod tray;
//mod icon;

use clipboard_manager::ClipboardManager;
use config::Config;

/// Clipboard data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardData {
    /// Plain text content (always present)
    pub content: String,
    /// Rich text formats (optional)
    pub html: Option<String>,
    pub rtf: Option<String>,
    /// Image data as base64 (optional)
    pub image: Option<String>,
    /// Metadata
    pub content_type: String, // "text", "html", "rtf", "image", "mixed"
    pub timestamp: u64,
}

/// Clipboard message for WebSocket communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: ClipboardData,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Shared Clipboard Client (Testing Mode)");

    // Test configuration
    let config = Config::load();
    info!("Configuration loaded successfully");
    info!("Server URL: {}", config.server_url);
    info!("Sync paused: {}", config.sync_paused);
    
    // If config is empty, create a default one
    if config.is_first_run() {
        info!("First run detected, saving default configuration...");
        if let Err(e) = config.save() {
            error!("Failed to save default config: {}", e);
        } else {
            info!("Default configuration saved");
        }
    }

    // Test clipboard manager
    match ClipboardManager::new() {
        Ok(mut manager) => {
            info!("Clipboard manager created successfully");
            
            // Test reading clipboard
            match manager.get_clipboard_data() {
                Ok(clipboard_data) => {
                    let content = &clipboard_data.content;
                    info!("Current clipboard content: {:?}", 
                          if content.len() > 100 { 
                              format!("{}...", &content[..100]) 
                          } else { 
                              content.clone() 
                          }
                    );
                    info!("Clipboard type: {}", clipboard_data.content_type);
                    if clipboard_data.html.is_some() {
                        info!("Has HTML content: true");
                    }
                    if clipboard_data.rtf.is_some() {
                        info!("Has RTF content: true");
                    }
                }
                Err(e) => {
                    error!("Failed to read clipboard: {}", e);
                }
            }

            // Create test clipboard data
            let test_content = "Test from clipboard-client - Hello World!";
            let test_data = ClipboardData {
                content: test_content.to_string(),
                html: None,
                rtf: None,
                image: None,
                content_type: "text".to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            
            // Test setting clipboard (using public API)
            match manager.set_clipboard_data_from_server(&test_data) {
                Ok(_) => {
                    info!("Successfully set clipboard content: {}", test_content);
                    
                    // Verify it was set
                    match manager.get_clipboard_data() {
                        Ok(clipboard_data) => {
                            if clipboard_data.content == test_content {
                                info!("✓ Clipboard set/get test passed");
                            } else {
                                error!("✗ Clipboard content mismatch. Expected: {}, Got: {}", test_content, clipboard_data.content);
                            }
                        }
                        Err(e) => {
                            error!("Failed to verify clipboard content: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to set clipboard: {}", e);
                }
            }
        }
        Err(e) => {
            error!("Failed to create clipboard manager: {}", e);
        }
    }

    info!("Testing completed");
    Ok(())
}
