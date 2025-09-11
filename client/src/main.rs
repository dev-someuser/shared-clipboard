use futures_util::StreamExt;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use url::Url;

mod clipboard_manager;
use clipboard_manager::ClipboardManager;

#[cfg(target_os = "linux")]
mod tray;
#[cfg(target_os = "windows")]
mod tray_win;

mod settings;
mod config;

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

#[derive(Clone, Debug)]
enum Command { SetUrl(String), OpenSettings, Quit }

#[derive(Clone, Debug)]
enum Event { Connected, Disconnected, UrlChanged(String), Error(String) }

struct ClipboardClient {
    clipboard_manager: ClipboardManager,
    http_client: HttpClient,
    url_tx: tokio::sync::watch::Sender<String>,
    url_rx: tokio::sync::watch::Receiver<String>,
    cmd_tx: tokio::sync::mpsc::UnboundedSender<Command>,
    evt_tx: tokio::sync::broadcast::Sender<Event>,
    last_local_content: String,
    last_local_image: Option<String>,
    #[cfg(target_os = "linux")]
    tray: Option<tray::TrayController>,
    #[cfg(target_os = "windows")]
    tray_win: Option<tray_win::TrayController>,
}

impl ClipboardClient {
    fn new(initial_url: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let clipboard_manager = ClipboardManager::new()?;
        let http_client = HttpClient::new();
        let (url_tx, url_rx) = tokio::sync::watch::channel(initial_url.clone());
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<Command>();
        let (evt_tx, _evt_rx) = tokio::sync::broadcast::channel::<Event>(16);

        // Command loop
        let url_tx_clone = url_tx.clone();
        let evt_tx_clone = evt_tx.clone();
        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    Command::SetUrl(u) => {
                        // Notify only if actually changed, to avoid endless reconnect loops
                        let changed = url_tx_clone.send_if_modified(|cur| {
                            if *cur != u { *cur = u.clone(); true } else { false }
                        });
                        if changed {
                            let _ = evt_tx_clone.send(Event::UrlChanged(String::new()));
                        }
                    }
                    Command::Quit => {
                        let _ = evt_tx_clone.send(Event::Disconnected);
                        break;
                    }
                    Command::OpenSettings => {}
                }
            }
        });

        #[cfg(target_os = "linux")]
        let tray = Some(tray::start_tray(initial_url.clone(), cmd_tx.clone()));
        #[cfg(target_os = "windows")]
        let tray_win = Some(tray_win::start_tray(initial_url.clone(), cmd_tx.clone()));

        Ok(Self {
            clipboard_manager,
            http_client,
            url_tx,
            url_rx,
            cmd_tx,
            evt_tx,
            last_local_content: String::new(),
            last_local_image: None,
            #[cfg(target_os = "linux")]
            tray,
            #[cfg(target_os = "windows")]
            tray_win,
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
        let current_url = self.url_rx.borrow().clone();
        let ws_url = format!("ws://{}/ws", current_url.replace("http://", "").replace("https://", ""));
        let url = Url::parse(&ws_url)?;
        
        let (ws_stream, _) = connect_async(url).await?;
        info!("Connected to WebSocket server");
        
        // Update tray connectivity status
        #[cfg(target_os = "linux")]
        if let Some(tray) = &self.tray {
            tray.set_connected(true);
        }
        #[cfg(target_os = "windows")]
        if let Some(tray) = &self.tray_win {
            tray.set_connected(true);
        }
        
        let (_ws_sender, mut ws_receiver) = ws_stream.split();

        // Create shared clipboard manager for both tasks
        let shared_clipboard_manager = std::sync::Arc::new(std::sync::Mutex::new(ClipboardManager::new().unwrap()));
        
        // Start clipboard monitoring task
        let clipboard_manager_for_monitor = shared_clipboard_manager.clone();
        let http_client = self.http_client.clone();
        let mut url_rx_for_monitor = self.url_rx.clone();
        
        let monitor_task = tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100)); // frequent polling
            let mut last_post: Option<Instant> = None;
            const MIN_POST_INTERVAL: Duration = Duration::from_millis(200);
            
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
                            // Rate-limit posts
                            let now = Instant::now();
                            if let Some(prev) = last_post { if now.duration_since(prev) < MIN_POST_INTERVAL { continue; } }
                            last_post = Some(now);

                            let url = {
                                let base = url_rx_for_monitor.borrow().clone();
                                format!("{}/api/clipboard", base)
                            };
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
        let mut url_rx_for_ws = self.url_rx.clone();
        let websocket_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    maybe_msg = ws_receiver.next() => {
                        match maybe_msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(clipboard_msg) = serde_json::from_str::<ClipboardMessage>(&text) {
                                    if clipboard_msg.msg_type == "clipboard_update" {
                                        info!("Received clipboard update from server: {} chars, type: {}",
                                              clipboard_msg.data.content.len(), clipboard_msg.data.content_type);
                                        // Check if this is our own content returned from server
                                        let is_own_content = {
                                            let manager = clipboard_manager_for_websocket.lock().unwrap();
                                            manager.is_own_content_returned(&clipboard_msg.data)
                                        };
                                        if is_own_content { info!("  - Own content returned, ignoring"); continue; }
                                        if clipboard_msg.data.html.is_some() { info!("  - Contains HTML content"); }
                                        if clipboard_msg.data.rtf.is_some() { info!("  - Contains RTF content"); }
                                        if clipboard_msg.data.image.is_some() { info!("  - Contains image content"); }

                                        let result = {
                                            let mut manager = clipboard_manager_for_websocket.lock().unwrap();
                                            manager.set_clipboard_data_from_server(&clipboard_msg.data)
                                        };
                                        if let Err(e) = result { error!("Failed to set clipboard: {}", e); }
                                        else { info!("Successfully updated local clipboard (smart mode)"); }
                                    }
                                }
                            }
                            Some(Ok(Message::Close(_))) => { info!("WebSocket connection closed by server"); break; }
                            Some(Err(e)) => { error!("WebSocket error: {}", e); break; }
                            _ => {}
                        }
                    }
                    _ = url_rx_for_ws.changed() => {
                        info!("URL changed, reconnecting WebSocket");
                        break;
                    }
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

        // Mark tray as disconnected before returning
        #[cfg(target_os = "linux")]
        if let Some(tray) = &self.tray {
            tray.set_connected(false);
        }
        #[cfg(target_os = "windows")]
        if let Some(tray) = &self.tray_win {
            tray.set_connected(false);
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
            
            info!("Attempting to reconnect to {}", self.url_rx.borrow().as_str());
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();


    // Если запущено в режиме настроек отдельным процессом, просто показать окно и выйти
    {
        let args = std::env::args().collect::<Vec<_>>();
        if args.iter().any(|a| a == "--settings") {
            let mut url = "http://127.0.0.1:8080".to_string();
            if let Ok(env_url) = std::env::var("CLIPBOARD_SERVER_URL") { url = env_url; }
            for a in &args { if let Some(s) = a.strip_prefix("--url=") { url = s.to_string(); } }
            let connected = args.iter().any(|a| a == "--connected");
            if let Some(new_url) = crate::settings::run_settings_ui(url, connected) { println!("{}", new_url); }
            return Ok(());
        }
    }

    // Load config or env
    let server_url = config::load_server_url().unwrap_or_else(|| {
        std::env::var("CLIPBOARD_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
    });

    info!("Starting clipboard client daemon, connecting to: {}", server_url);

    let mut client = ClipboardClient::new(server_url.clone())?;

    // Persist URL changes
    let mut url_rx_for_persist = client.url_rx.clone();
    tokio::spawn(async move {
        let mut last = url_rx_for_persist.borrow().clone();
        loop {
            if url_rx_for_persist.changed().await.is_err() { break; }
            let current = url_rx_for_persist.borrow().clone();
            if current != last { let _ = config::save_server_url(&current); last = current.clone(); }
        }
    });

    // Handle graceful shutdown
    tokio::select! {
        _ = client.run_with_reconnect() => { info!("Client reconnection loop ended"); }
        _ = tokio::signal::ctrl_c() => { info!("Received Ctrl+C, shutting down..."); }
    }

    info!("Clipboard client daemon stopped");
    Ok(())
}
