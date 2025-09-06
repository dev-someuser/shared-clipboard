use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{info, warn};
use warp::Filter;

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

type Clients = Arc<Mutex<HashMap<String, tokio::sync::mpsc::UnboundedSender<warp::ws::Message>>>>;
type ClipboardState = Arc<Mutex<Option<ClipboardData>>>;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Shared state
    let clipboard_state: ClipboardState = Arc::new(Mutex::new(None));
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    let (tx, _rx) = broadcast::channel::<ClipboardData>(100);
    let broadcast_tx = Arc::new(tx);

    // WebSocket route
    let clients_ws = clients.clone();
    let clipboard_state_ws = clipboard_state.clone();
    let broadcast_tx_ws = broadcast_tx.clone();
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(warp::any().map(move || clients_ws.clone()))
        .and(warp::any().map(move || clipboard_state_ws.clone()))
        .and(warp::any().map(move || broadcast_tx_ws.clone()))
        .and_then(ws_handler);

    // HTTP API route for setting clipboard
    let clipboard_state_api = clipboard_state.clone();
    let broadcast_tx_api = broadcast_tx.clone();
    let api_route = warp::path!("api" / "clipboard")
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::any().map(move || clipboard_state_api.clone()))
        .and(warp::any().map(move || broadcast_tx_api.clone()))
        .and_then(set_clipboard);

    // HTTP API route for getting clipboard
    let clipboard_state_get = clipboard_state.clone();
    let get_route = warp::path!("api" / "clipboard")
        .and(warp::get())
        .and(warp::any().map(move || clipboard_state_get.clone()))
        .and_then(get_clipboard);

    let routes = ws_route.or(api_route).or(get_route);

    info!("Starting clipboard server on 127.0.0.1:8080");
    warp::serve(routes).run(([127, 0, 0, 1], 8080)).await;
}

async fn ws_handler(
    ws: warp::ws::Ws,
    clients: Clients,
    clipboard_state: ClipboardState,
    broadcast_tx: Arc<broadcast::Sender<ClipboardData>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(ws.on_upgrade(move |socket| handle_client(socket, clients, clipboard_state, broadcast_tx)))
}

async fn handle_client(
    ws: warp::ws::WebSocket,
    clients: Clients,
    clipboard_state: ClipboardState,
    broadcast_tx: Arc<broadcast::Sender<ClipboardData>>,
) {
    let client_id = uuid::Uuid::new_v4().to_string();
    info!("New client connected: {}", client_id);

    let (mut ws_tx, mut ws_rx) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Add client to clients map
    {
        let mut clients_lock = clients.lock().await;
        clients_lock.insert(client_id.clone(), tx);
    }

    // Send current clipboard state to new client
    if let Some(current_data) = clipboard_state.lock().await.as_ref() {
        let message = ClipboardMessage {
            msg_type: "clipboard_update".to_string(),
            data: current_data.clone(),
        };
        if let Ok(json) = serde_json::to_string(&message) {
            let _ = ws_tx.send(warp::ws::Message::text(json)).await;
        }
    }

    // Subscribe to broadcasts
    let mut broadcast_rx = broadcast_tx.subscribe();

    // Spawn task to handle outgoing messages
    let client_id_clone = client_id.clone();
    let ws_tx_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // Handle direct messages to this client
                msg = rx.recv() => {
                    match msg {
                        Some(message) => {
                            if ws_tx.send(message).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                // Handle broadcast messages
                broadcast_msg = broadcast_rx.recv() => {
                    match broadcast_msg {
                        Ok(data) => {
                            let message = ClipboardMessage {
                                msg_type: "clipboard_update".to_string(),
                                data,
                            };
                            if let Ok(json) = serde_json::to_string(&message) {
                                if ws_tx.send(warp::ws::Message::text(json)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        info!("Client {} disconnected", client_id_clone);
    });

    // Handle incoming messages from client
    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => {
                if msg.is_text() {
                    let text = msg.to_str().unwrap();
                    if let Ok(clipboard_msg) = serde_json::from_str::<ClipboardMessage>(text) {
                        if clipboard_msg.msg_type == "clipboard_set" {
                            // Update clipboard state
                            {
                                let mut state = clipboard_state.lock().await;
                                *state = Some(clipboard_msg.data.clone());
                            }
                            // Broadcast to all clients
                            let _ = broadcast_tx.send(clipboard_msg.data);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("WebSocket error for client {}: {}", client_id, e);
                break;
            }
        }
    }

    // Remove client from clients map
    {
        let mut clients_lock = clients.lock().await;
        clients_lock.remove(&client_id);
    }

    // Cancel the outgoing message task
    ws_tx_task.abort();
}

async fn set_clipboard(
    data: ClipboardData,
    clipboard_state: ClipboardState,
    broadcast_tx: Arc<broadcast::Sender<ClipboardData>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    info!("Setting clipboard via HTTP API: {} chars, type: {}", 
          data.content.len(), data.content_type);
    
    if data.html.is_some() {
        info!("  - Contains HTML content");
    }
    if data.rtf.is_some() {
        info!("  - Contains RTF content");
    }
    if data.image.is_some() {
        info!("  - Contains image content");
    }

    // Update clipboard state
    {
        let mut state = clipboard_state.lock().await;
        *state = Some(data.clone());
    }

    // Broadcast to all WebSocket clients
    let _ = broadcast_tx.send(data.clone());

    Ok(warp::reply::json(&data))
}

async fn get_clipboard(
    clipboard_state: ClipboardState,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = clipboard_state.lock().await;
    match state.as_ref() {
        Some(data) => Ok(warp::reply::json(data)),
        None => Ok(warp::reply::json(&ClipboardData {
            content: String::new(),
            html: None,
            rtf: None,
            image: None,
            content_type: "text".to_string(),
            timestamp: 0,
        })),
    }
}
