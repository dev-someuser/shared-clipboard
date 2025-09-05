# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

This is a shared clipboard system written in Rust, consisting of a WebSocket/HTTP server and a Linux daemon client. The system enables real-time clipboard synchronization across multiple Linux machines through a centralized server.

## Architecture

The project uses a Cargo workspace with two main components:

- **Server** (`/server`): Warp-based HTTP/WebSocket server that manages clipboard state and broadcasts updates to connected clients
- **Client** (`/client`): Linux daemon that monitors local clipboard changes and synchronizes with the server

### Key Technical Details

- **Communication**: WebSocket for real-time updates + HTTP API for clipboard operations
- **Clipboard Integration**: Uses `arboard` library for X11/Wayland clipboard access
- **Concurrency**: Tokio async runtime with broadcast channels for client notifications
- **Data Format**: JSON messages with `ClipboardData` struct containing content and timestamp
- **Client Management**: UUID-based client tracking with automatic cleanup on disconnect

## Common Commands

### Building and Running
```bash
# Build entire workspace
cargo build --release

# Build specific component  
cargo build --release --bin clipboard-server
cargo build --release --bin clipboard-client

# Run server (defaults to 127.0.0.1:8080)
./start-server.sh
# or
cd server && cargo run --release

# Run client (connects to server)
./start-client.sh  
# or
cd client && cargo run --release

# Run client with custom server URL
CLIPBOARD_SERVER_URL=http://192.168.1.100:8080 ./start-client.sh
```

### Development Commands
```bash
# Run tests
cargo test

# Check code
cargo check

# Format code
cargo fmt

# Run clippy linting
cargo clippy

# Run with debug logging
RUST_LOG=debug cargo run --release --bin clipboard-server
RUST_LOG=debug cargo run --release --bin clipboard-client
```

### API Testing
```bash
# Get current clipboard content
curl http://127.0.0.1:8080/api/clipboard

# Set clipboard content  
curl -X POST http://127.0.0.1:8080/api/clipboard \
  -H "Content-Type: application/json" \
  -d '{"content": "Hello World!", "timestamp": 1694234567}'

# Test examples
./examples/api-usage.sh
```

## System Dependencies

Before building, install required system packages:

```bash
# Ubuntu/Debian
sudo apt install libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev

# Arch Linux
sudo pacman -S libxcb

# Fedora  
sudo dnf install libxcb-devel
```

## Development Notes

### Server Architecture
- Uses Warp web framework with filter-based routing
- Maintains client connections in `HashMap<String, UnboundedSender>`
- Global clipboard state stored in `Arc<Mutex<Option<ClipboardData>>>`
- Broadcast channel distributes updates to all connected WebSocket clients

### Client Architecture  
- Polls local clipboard every 500ms for changes
- Sends updates to server via HTTP POST to `/api/clipboard`
- Receives updates from server via WebSocket connection
- Handles both X11 and Wayland clipboard systems through arboard

### Key Data Structures
- `ClipboardData`: Contains `content: String` and `timestamp: u64`
- `ClipboardMessage`: WebSocket message wrapper with `type` field and `data`
- Server maintains both HTTP and WebSocket endpoints for flexibility

### Error Handling
- Client gracefully handles clipboard access errors (common when clipboard is empty)
- WebSocket disconnections trigger automatic client cleanup
- HTTP requests include retry logic for network failures

### Logging
- Uses `tracing` crate for structured logging
- Set `RUST_LOG=debug` for detailed operation logs
- Set `RUST_LOG=info` for standard operation logs
- Set `RUST_LOG=warn` for errors and warnings only

## Configuration

### Environment Variables
- `CLIPBOARD_SERVER_URL`: Server URL for client (default: `http://127.0.0.1:8080`)
- `RUST_LOG`: Logging level (`debug`, `info`, `warn`, `error`)
- `DISPLAY`: X11 display (required for clipboard access)

### Security Considerations
- Server listens only on localhost by default
- No authentication implemented - designed for trusted networks
- Data transmitted in plaintext
- Only text clipboard content supported (no images/files)
