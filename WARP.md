# WARP.md

Guidance for working with this repository in Warp (and a concise technical reference).

## Overview

A Rust workspace with two components:
- Server (`server/`): Warp HTTP + WebSocket; keeps last clipboard and broadcasts updates
- Client (`client/`): Daemon that syncs local clipboard with the server (Linux + Windows)

## Key technical details

- Transport: WebSocket for realtime; HTTP for setting and reading clipboard
- Linux clipboard: wl-clipboard-rs
- Windows clipboard: clipboard-win
- Tray:
  - Linux: ksni (StatusNotifier) + generated icon; menu: status, Settings, Quit
  - Windows: tray-icon + generated icon; menu: status, Settings, Quit
- Settings window: eframe/egui — edit URL, test, Save (only Save applies changes)
- Reconnect loop with exponential backoff (1s..60s)
- Client keeps server URL in Arc<Mutex<String>> so it can be updated at runtime from Settings

## Build & run

Linux:
```bash
sudo apt-get install -y libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libdbus-1-dev
cargo build --release --bin clipboard-server
cargo build --release --bin clipboard-client
./start-server.sh
CLIPBOARD_SERVER_URL=http://127.0.0.1:8080 ./start-client.sh
```

Windows (PowerShell or cmd):
```cmd
cargo build --release --bin clipboard-server
cargo build --release --bin clipboard-client
start-server.bat
start-client.bat
```

Dev tooling:
```bash
cargo test
cargo check
cargo fmt
cargo clippy
RUST_LOG=debug cargo run --release --bin clipboard-server
RUST_LOG=debug cargo run --release --bin clipboard-client
```

## API quick test
```bash
curl http://127.0.0.1:8080/api/clipboard
curl -X POST http://127.0.0.1:8080/api/clipboard \
  -H "Content-Type: application/json" \
  -d '{"content":"Hello","content_type":"text","timestamp":1694234567}'
```

## System dependencies

Linux runtime/build:
- libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev (clipboard)
- libdbus-1-dev (ksni tray)

Windows:
- No extra system packages; GUI/tray via tray-icon

## Data structures (client/server contract)
- ClipboardData { content, html?, rtf?, image?, content_type, timestamp }
- ClipboardMessage { type: "clipboard_update", data: ClipboardData }

## Logging
- tracing + tracing-subscriber
- RUST_LOG=debug|info|warn|error

## Security
- Localhost bind by default; no auth; plaintext
- For remote networks, put the server behind TLS reverse proxy

See also: README.md for user-facing instructions and DOCKER.md for container notes.
