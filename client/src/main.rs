use futures_util::StreamExt;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use url::Url;

mod clipboard_manager;
use clipboard_manager::ClipboardManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipboardData {
    // Plain text content (always present)
    content: String,
    // Rich text formats (optional)
    html: Option<String>,
    rtf: Option<String>,
    // Image data as base64 (optional)
    image: Option<String>,
    // Metadata
    content_type: String, // "text", "html", "rtf", "image", "mixed"
    timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClipboardMessage {
    #[serde(rename = "type")]
    msg_type: String,
    data: ClipboardData,
}

struct ClipboardClient {
    clipboard_manager: ClipboardManager,
    http_client: HttpClient,
    server_url: String,
    last_local_content: String,
    last_local_image: Option<String>,
}

impl ClipboardClient {
    fn new(server_url: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let clipboard_manager = ClipboardManager::new()?;
        let http_client = HttpClient::new();
        
        Ok(Self {
            clipboard_manager,
            http_client,
            server_url,
            last_local_content: String::new(),
            last_local_image: None,
        })
    }

    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting clipboard client daemon");
        
        // On Linux, ensure DISPLAY is set for X11 clipboard access
        #[cfg(target_os = "linux")]
        if std::env::var("DISPLAY").is_err() {
            warn!("DISPLAY environment variable not set - clipboard access may be limited");
            warn!("Try running: export DISPLAY=:0 before starting the client");
        }

        // Get initial clipboard content
        if let Ok(clipboard_data) = self.clipboard_manager.get_clipboard_data() {
            self.last_local_content = clipboard_data.content;
            self.last_local_image = clipboard_data.image.clone();
        }

        // Connect to WebSocket
        let ws_url = format!("ws://{}/ws", self.server_url.replace("http://", "").replace("https://", ""));
        let url = Url::parse(&ws_url)?;
        
        let (ws_stream, _) = connect_async(url).await?;
        info!("Connected to WebSocket server");
        
        let (_ws_sender, mut ws_receiver) = ws_stream.split();

        // Create shared clipboard manager for both tasks
        let shared_clipboard_manager = std::sync::Arc::new(std::sync::Mutex::new(ClipboardManager::new().unwrap()));
        
        // Start clipboard monitoring task
        let clipboard_manager_for_monitor = shared_clipboard_manager.clone();
        let http_client = self.http_client.clone();
        let server_url = self.server_url.clone();
        
        let monitor_task = tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100)); // Very frequent polling for Linux clipboard reliability
            
            loop {
                interval.tick().await;
                
                // Try to get clipboard data with retry for robustness
                let clipboard_result = {
                    let mut attempts = 0;
                    const MAX_ATTEMPTS: u8 = 3;
                    let mut last_error = None;
                    
                    loop {
                        attempts += 1;
                        let result = {
                            let mut manager = clipboard_manager_for_monitor.lock().unwrap();
                            manager.get_clipboard_data()
                        };
                        
                        match result {
                            Ok(data) => break Ok(data),
                            Err(e) if attempts < MAX_ATTEMPTS => {
                                last_error = Some(e);
                                // Small delay between retries
                                tokio::time::sleep(Duration::from_millis(10)).await;
                                continue;
                            }
                            Err(e) => break Err(e),
                        }
                    }
                };
                
                match clipboard_result {
                    Ok(clipboard_data) => {
                        // Use smart change detection to avoid ping-pong loops
                        let content_changed = {
                            let mut manager = clipboard_manager_for_monitor.lock().unwrap();
                            manager.has_content_changed(&clipboard_data, false, None)
                        };
                        
                        if content_changed {
                            let size_desc = match clipboard_data.content_type.as_str() {
                                "image" => format!("image data"),
                                "text" => format!("{} chars", clipboard_data.content.len()),
                                _ => format!("{} chars + rich content", clipboard_data.content.len()),
                            };
                            
                            info!("Local clipboard changed: {}, type: {}", size_desc, clipboard_data.content_type);
                            
                            if clipboard_data.html.is_some() {
                                info!("  - Has HTML content");
                            }
                            if clipboard_data.rtf.is_some() {
                                info!("  - Has RTF content");
                            }
                            if clipboard_data.image.is_some() {
                                info!("  - Has image content");
                            }
                            
                            // Mark content as sent before sending to avoid processing it back
                            {
                                let mut manager = clipboard_manager_for_monitor.lock().unwrap();
                                manager.mark_content_as_sent(&clipboard_data);
                            }
                            
                            // Send to server via HTTP
                            let url = format!("{}/api/clipboard", server_url);
                            if let Err(e) = http_client
                                .post(&url)
                                .json(&clipboard_data)
                                .send()
                                .await
                            {
                                warn!("Failed to send clipboard to server: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        // Ignore clipboard errors (common when clipboard is empty or inaccessible)
                        let error_str = e.to_string();
                        if !error_str.contains("empty") && !error_str.contains("not available") {
                            // Only log non-trivial errors, but not too frequently
                            if error_str.contains("Connection") || error_str.contains("Display") {
                                warn!("Clipboard system error (may need X11/Wayland focus): {}", e);
                            } else {
                                debug!("Clipboard access failed: {}", e);
                            }
                        }
                    }
                }
            }
        });

        // Handle WebSocket messages
        let clipboard_manager_for_websocket = shared_clipboard_manager.clone();
        let websocket_task = tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(clipboard_msg) = serde_json::from_str::<ClipboardMessage>(&text) {
                            if clipboard_msg.msg_type == "clipboard_update" {
                                info!("Received clipboard update from server: {} chars, type: {}", 
                                      clipboard_msg.data.content.len(), clipboard_msg.data.content_type);
                                
                                // Check if this is our own content returned from server
                                let is_own_content = {
                                    let manager = clipboard_manager_for_websocket.lock().unwrap();
                                    manager.is_own_content_returned(&clipboard_msg.data)
                                };
                                
                                if is_own_content {
                                    info!("  - This is our own content returned from server, ignoring");
                                    continue;
                                }
                                
                                if clipboard_msg.data.html.is_some() {
                                    info!("  - Contains HTML content");
                                }
                                if clipboard_msg.data.rtf.is_some() {
                                    info!("  - Contains RTF content");
                                }
                                if clipboard_msg.data.image.is_some() {
                                    info!("  - Contains image content");
                                }
                                
                                // Use smart clipboard setting to avoid ping-pong loops
                                let result = {
                                    let mut manager = clipboard_manager_for_websocket.lock().unwrap();
                                    manager.set_clipboard_data_from_server(&clipboard_msg.data)
                                };
                                
                                if let Err(e) = result {
                                    error!("Failed to set clipboard: {}", e);
                                } else {
                                    info!("Successfully updated local clipboard with rich content (smart mode)");
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket connection closed by server");
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Wait for either task to complete
        tokio::select! {
            _ = monitor_task => {
                info!("Clipboard monitor task ended");
            }
            _ = websocket_task => {
                info!("WebSocket task ended");
            }
        }

        Ok(())
    }

    async fn run_with_reconnect(&mut self) {
        let mut reconnect_delay = Duration::from_secs(1);
        const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(60);
        
        loop {
            match self.start().await {
                Ok(()) => {
                    info!("Connection ended normally, attempting reconnect...");
                    reconnect_delay = Duration::from_secs(1); // Reset delay on successful connection
                }
                Err(e) => {
                    error!("Connection failed: {}, retrying in {:?}...", e, reconnect_delay);
                }
            }
            
            tokio::time::sleep(reconnect_delay).await;
            
            // Exponential backoff with maximum delay
            reconnect_delay = std::cmp::min(reconnect_delay * 2, MAX_RECONNECT_DELAY);
            
            info!("Attempting to reconnect to {}", self.server_url);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get server URL from environment or use default
    let server_url = std::env::var("CLIPBOARD_SERVER_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    info!("Starting clipboard client daemon, connecting to: {}", server_url);

    let mut client = ClipboardClient::new(server_url)?;
    
    // Handle graceful shutdown
    tokio::select! {
        _ = client.run_with_reconnect() => {
            info!("Client reconnection loop ended");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
    }

    info!("Clipboard client daemon stopped");
    Ok(())
}
