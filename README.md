# 📋 Shared Clipboard

[![Release](https://img.shields.io/github/v/release/your-username/shared-clipboard)](https://github.com/your-username/shared-clipboard/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/your-username/shared-clipboard/ci.yml)](https://github.com/your-username/shared-clipboard/actions)
[![Docker](https://img.shields.io/badge/docker-ready-blue)](https://github.com/your-username/shared-clipboard/blob/main/DOCKER.md)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](https://github.com/your-username/shared-clipboard/blob/main/LICENSE)

**Cross-platform clipboard synchronization system with rich text support** 🚀

Share your clipboard content in real-time across multiple devices (Linux & Windows) with support for plain text, HTML, and RTF formats.

![Demo](https://via.placeholder.com/800x400/1f1f1f/ffffff?text=Shared+Clipboard+Demo)

## ✨ Features

- 🌐 **Cross-platform**: Linux (Wayland) and Windows support
- 📝 **Rich text support**: Plain text, HTML, and RTF formats
- ⚡ **Real-time sync**: WebSocket connections for instant updates
- 🔌 **REST API**: HTTP endpoints for easy integration
- 🐳 **Docker ready**: Containerized deployment with health checks
- 🔧 **Easy setup**: Simple installation with pre-built binaries
- 📊 **Monitoring**: Built-in health checks and structured logging

## 🏗️ Architecture

The system consists of two main components:

- **Server** (`/server`): Warp-based HTTP/WebSocket server that manages clipboard state
- **Client** (`/client`): Cross-platform daemon that monitors local clipboard changes

```
┌─────────────┐    WebSocket/HTTP    ┌─────────────┐
│   Client    │ ◄─────────────────► │   Server    │
│  (Linux)    │                     │  (Rust)     │
└─────────────┘                     └─────────────┘
       ▲                                    ▲
       │                                    │
   Clipboard                          Clipboard State
   Monitoring                         & Broadcasting
       │                                    │
       ▼                                    ▼
┌─────────────┐                     ┌─────────────┐
│   Client    │ ◄─────────────────► │   Client    │
│ (Windows)   │                     │  (Linux)    │
└─────────────┘                     └─────────────┘
```

## 🚀 Quick Start

### Option 1: Download Pre-built Binaries (Recommended)

1. Go to [Releases](https://github.com/your-username/shared-clipboard/releases)
2. Download the appropriate package for your OS:
   - **Windows**: `shared-clipboard-windows-v1.1.0.zip`
   - **Linux**: `shared-clipboard-linux-v1.1.0.tar.gz`
3. Extract and follow the README inside

*Note: macOS support coming soon!*

### Option 2: Docker Deployment

```bash
# Run server in Docker
docker run -d -p 8080:8080 ghcr.io/your-username/shared-clipboard-server:latest

# Or with docker-compose
wget https://raw.githubusercontent.com/your-username/shared-clipboard/main/docker-compose.yml
docker compose up -d
```

See [DOCKER.md](DOCKER.md) for detailed Docker deployment guide.

### Option 3: Build from Source

**Prerequisites:**
- Rust 1.70+ with Cargo
- Linux: Wayland session + system packages

```bash
# Install system dependencies (Linux only)
sudo apt install libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev

# Clone and build
git clone https://github.com/your-username/shared-clipboard
cd shared-clipboard
cargo build --release
```

## 📚 Usage

### Using Pre-built Binaries

**Windows:**
1. Extract `shared-clipboard-windows.zip`
2. Run `start-server.bat` on one machine (server)
3. Run `start-client.bat` on other machines
4. Edit `start-client.bat` to change server URL if needed

**Linux:**
1. Extract the archive: `tar -xzf shared-clipboard-linux.tar.gz`
2. Make executable: `chmod +x *.sh clipboard-*`
3. Run server: `./start-server.sh`
4. Run client on other machines: `./start-client.sh`
5. Set custom server: `export CLIPBOARD_SERVER_URL=http://192.168.1.100:8080`

### Using Docker

```bash
# Start server
docker run -d -p 8080:8080 \  
  --name clipboard-server \  
  ghcr.io/your-username/shared-clipboard-server

# Check status
docker logs clipboard-server
curl http://localhost:8080/api/clipboard
```

## 🔌 API Reference

### HTTP Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/clipboard` | Get current clipboard content |
| `POST` | `/api/clipboard` | Set clipboard content |
| `WS` | `/ws` | WebSocket for real-time updates |

### Data Format

```json
{
  "content": "Plain text content",
  "html": "<p>Rich HTML content</p>",
  "rtf": "{\\rtf1 RTF content}",
  "image": null,
  "content_type": "text|html|rtf|mixed",
  "timestamp": 1694234567
}
```

### Examples

```bash
# Get clipboard
curl http://localhost:8080/api/clipboard

# Set text content
curl -X POST http://localhost:8080/api/clipboard \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Hello World!",
    "content_type": "text",
    "timestamp": 1694234567
  }'

# Set HTML content
curl -X POST http://localhost:8080/api/clipboard \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Hello World!",
    "html": "<p><strong>Hello</strong> World!</p>",
    "content_type": "html",
    "timestamp": 1694234567
  }'
```

## 🔍 Monitoring & Logging

Configure logging level with `RUST_LOG` environment variable:

```bash
export RUST_LOG=info   # Default: info messages
export RUST_LOG=debug  # Detailed debug information
export RUST_LOG=warn   # Only warnings and errors
```

## 🛡️ Security

- Server binds to localhost (127.0.0.1) by default
- No authentication - intended for trusted networks
- Data transmitted in plain text
- Use reverse proxy with SSL for production

## 📚 Documentation

- [Docker Deployment Guide](DOCKER.md)
- [Technical Documentation](WARP.md)
- [API Examples](examples/)
- [Troubleshooting](#-troubleshooting)

## 🛠️ Troubleshooting

### Common Issues

**Clipboard Access Errors (Linux):**
- Ensure Wayland session is running
- Install required system libraries
- Check Wayland compositor compatibility

**Connection Issues:**
- Verify server is running: `curl http://localhost:8080/api/clipboard`
- Check firewall settings
- Ensure correct server URL in client

**Windows Issues:**
- Run as administrator if clipboard access fails
- Check antivirus software permissions

### Debug Mode

```bash
# Enable debug logging
RUST_LOG=debug ./start-server.sh
RUST_LOG=debug ./start-client.sh

# Check server health
curl -f http://localhost:8080/api/clipboard
```

## 🤝 Contributing

We welcome contributions! Please see:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## 📝 License

This project is licensed under either of:

- Apache License, Version 2.0
- MIT License

at your option.

## 📞 Support

- 🐛 [Report Issues](https://github.com/your-username/shared-clipboard/issues)
- 💬 [Discussions](https://github.com/your-username/shared-clipboard/discussions)
- 📧 [Security Issues](mailto:security@example.com)
