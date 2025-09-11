# 📋 Shared Clipboard

Cross-platform clipboard synchronization (Linux + Windows) with rich text, system tray, and a simple settings window.

## Features

- Cross-platform: Linux (Wayland/X11 tray via StatusNotifier) and Windows (system tray)
- Rich text support: plain text, HTML, RTF
- Real-time sync via WebSocket + HTTP API
- Tray icon with menu: status, Settings (URL edit/test/save), Quit
- Lightweight server and daemon client

## Architecture (overview)

- Server (`server/`): Warp HTTP + WebSocket; stores last clipboard and broadcasts updates
- Client (`client/`): Daemon monitors local clipboard and syncs with server

See also: Technical details in WARP.md

## Quick start

Build from source (recommended for now):

```bash
# Linux prerequisites
sudo apt-get update
sudo apt-get install -y libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libdbus-1-dev

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
source "$HOME/.cargo/env"

# Build client and server
cargo build --release --bin clipboard-server
cargo build --release --bin clipboard-client

# Run
./start-server.sh &
CLIPBOARD_SERVER_URL=http://127.0.0.1:8080 ./start-client.sh
```

Docker: see DOCKER.md

## Usage

- The client starts minimized with a tray icon
- Right-click tray:
  - Connected • <url> / Disconnected • <url> (disabled label)
  - Settings — edit URL, test connectivity, Save to apply
  - Quit — exit the daemon

Environment variables:
- CLIPBOARD_SERVER_URL (default: http://127.0.0.1:8080)
- RUST_LOG (info|debug|warn|error)

## API (brief)

- GET /api/clipboard — current content
- POST /api/clipboard — set content
- WebSocket /ws — updates

See WARP.md for message structures and more details.

## Troubleshooting

- Linux Wayland/X11 clipboard: ensure required libxcb* are installed and a compositor is running
- Connection errors: verify server URL and network reachability
- Logs: set RUST_LOG=debug and check terminal output

## Documentation

- Technical details: WARP.md
- Docker guide: DOCKER.md

## License

MIT OR Apache-2.0
