# Docker guide

This project’s server can be run in a container. Below are minimal, current instructions. For building from source and client usage, see README.md.

## Run with Docker CLI

```bash
docker build -t shared-clipboard-server ./server
# or use a published image if available
# docker run -d -p 8080:8080 ghcr.io/<org>/shared-clipboard-server:latest

docker run -d \
  --name clipboard-server \
  -p 8080:8080 \
  -e RUST_LOG=info \
  shared-clipboard-server

# Check
curl http://localhost:8080/api/clipboard
```

## docker-compose (optional)

```yaml
services:
  clipboard-server:
    build: ./server
    ports:
      - "8080:8080"
    environment:
      - RUST_LOG=info
    restart: unless-stopped
```

## Notes

- Exposes HTTP API and WebSocket on port 8080
- No auth, plaintext transport — use a reverse proxy with TLS if needed
- See README.md for API examples and WARP.md for technical details
