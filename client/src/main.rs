use futures_util::{SinkExt, StreamExt};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};
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
    last_server_timestamp: u64,
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
            last_server_timestamp: 0,
        })
    }

    async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting clipboard client daemon");

        // Get initial clipboard content
        if let Ok(clipboard_data) = self.clipboard_manager.get_clipboard_data() {
            self.last_local_content = clipboard_data.content;
        }

        // Connect to WebSocket
        let ws_url = format!("ws://{}/ws", self.server_url.replace("http://", "").replace("https://", ""));
        let url = Url::parse(&ws_url)?;
        
        let (ws_stream, _) = connect_async(url).await?;
        info!("Connected to WebSocket server");
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Start clipboard monitoring task
        let mut clipboard_manager = ClipboardManager::new().unwrap();
        let mut last_content = self.last_local_content.clone();
        let http_client = self.http_client.clone();
        let server_url = self.server_url.clone();
        
        let monitor_task = tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(500));
            
            loop {
                interval.tick().await;
                
                match clipboard_manager.get_clipboard_data() {
                    Ok(clipboard_data) => {
                        if clipboard_data.content != last_content && !clipboard_data.content.is_empty() {
                            info!("Local clipboard changed: {} chars, type: {}", 
                                  clipboard_data.content.len(), clipboard_data.content_type);
                            
                            if clipboard_data.html.is_some() {
                                info!("  - Has HTML content");
                            }
                            if clipboard_data.rtf.is_some() {
                                info!("  - Has RTF content");
                            }
                            if clipboard_data.image.is_some() {
                                info!("  - Has image content");
                            }
                            
                            last_content = clipboard_data.content.clone();
                            
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
                        if !e.to_string().contains("empty") {
                            warn!("Failed to read clipboard: {}", e);
                        }
                    }
                }
            }
        });

        // Handle WebSocket messages
        let mut clipboard_setter = ClipboardManager::new().unwrap();
        let websocket_task = tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(clipboard_msg) = serde_json::from_str::<ClipboardMessage>(&text) {
                            if clipboard_msg.msg_type == "clipboard_update" {
                                info!("Received clipboard update from server: {} chars, type: {}", 
                                      clipboard_msg.data.content.len(), clipboard_msg.data.content_type);
                                
                                if clipboard_msg.data.html.is_some() {
                                    info!("  - Contains HTML content");
                                }
                                if clipboard_msg.data.rtf.is_some() {
                                    info!("  - Contains RTF content");
                                }
                                if clipboard_msg.data.image.is_some() {
                                    info!("  - Contains image content");
                                }
                                
                                // Set rich clipboard content
                                if let Err(e) = clipboard_setter.set_clipboard_data(&clipboard_msg.data) {
                                    error!("Failed to set clipboard: {}", e);
                                } else {
                                    info!("Successfully updated local clipboard with rich content");
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
        result = client.start() => {
            if let Err(e) = result {
                error!("Client error: {}", e);
                return Err(e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
    }

    info!("Clipboard client daemon stopped");
    Ok(())
}
