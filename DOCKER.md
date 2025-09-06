# Docker Deployment Guide for Shared Clipboard Server

This guide contains instructions for deploying the shared clipboard server in Docker containers.

## Quick Start

### Using Docker Compose (Recommended)

```bash
# Build and start
docker-compose up -d

# View logs
docker-compose logs -f clipboard-server

# Stop
docker-compose down
```

### Using Docker CLI

```bash
# Build image
docker build -t shared-clipboard-server .

# Run container
docker run -d \
  --name clipboard-server \
  -p 8080:8080 \
  -e RUST_LOG=info \
  shared-clipboard-server

# View logs
docker logs -f clipboard-server

# Stop and remove
docker stop clipboard-server
docker rm clipboard-server
```

## Configuration

### Environment Variables

| Variable | Default Value | Description |
|----------|---------------|-------------|
| `RUST_LOG` | `info` | Logging level (`debug`, `info`, `warn`, `error`) |
| `RUST_BACKTRACE` | `1` | Enable stack trace on errors |

### Ports

- **8080**: HTTP API and WebSocket server

## Production Deployment

### With Reverse Proxy (Nginx)

```nginx
server {
    listen 80;
    server_name your-clipboard-server.com;

    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_cache_bypass $http_upgrade;
        
        # WebSocket support
        proxy_read_timeout 86400;
        proxy_send_timeout 86400;
    }
}
```

### With SSL/TLS (Let's Encrypt)

```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  clipboard-server:
    build: .
    container_name: shared-clipboard-server
    ports:
      - "127.0.0.1:8080:8080"  # Bind only to localhost
    environment:
      - RUST_LOG=warn  # Less logging in production
    restart: unless-stopped
    
  nginx:
    image: nginx:alpine
    container_name: clipboard-nginx
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
      - ./ssl:/etc/nginx/ssl
    depends_on:
      - clipboard-server
    restart: unless-stopped
```

## API Endpoints

After starting the container, the following endpoints are available:

- **GET /api/clipboard** - Get current clipboard contents
- **POST /api/clipboard** - Set clipboard contents
- **WebSocket /ws** - WebSocket connection for real-time updates

### Usage Examples

```bash
# Health check
curl http://localhost:8080/api/clipboard

# Set text content
curl -X POST http://localhost:8080/api/clipboard \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Hello from Docker!",
    "html": null,
    "rtf": null, 
    "image": null,
    "content_type": "text",
    "timestamp": 1694234567
  }'

# Set HTML content
curl -X POST http://localhost:8080/api/clipboard \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Hello World!",
    "html": "<p><strong>Hello</strong> from Docker!</p>",
    "rtf": null,
    "image": null,
    "content_type": "html",
    "timestamp": 1694234567
  }'
```

## Monitoring

### Health Check

```bash
# Check container status
docker-compose ps

# Manual health check
curl -f http://localhost:8080/api/clipboard
```

### Logs

```bash
# View logs
docker-compose logs clipboard-server

# Real-time logs
docker-compose logs -f clipboard-server

# Logs with timestamps
docker-compose logs -t clipboard-server
```

### Resource Metrics

```bash
# Resource usage
docker stats clipboard-server

# Detailed information
docker inspect clipboard-server
```

## Troubleshooting

### Common Issues

1. **Container won't start**
   ```bash
   docker-compose logs clipboard-server
   ```

2. **Port already in use**
   ```bash
   # Find process using port 8080
   lsof -i :8080
   
   # Or change port in docker-compose.yml
   ports:
     - "8081:8080"
   ```

3. **Network issues**
   ```bash
   # Check network settings
   docker network ls
   docker inspect clipboard-server
   ```

### Debugging

```bash
# Run in interactive mode
docker run -it --rm -p 8080:8080 shared-clipboard-server

# Connect to running container
docker exec -it clipboard-server /bin/sh

# Rebuild without cache
docker-compose build --no-cache
```

## Updates

```bash
# Stop service
docker-compose down

# Rebuild image
docker-compose build

# Start updated version
docker-compose up -d

# Verify update
curl http://localhost:8080/api/clipboard
```

## Security Notes

- Server binds to localhost by default for security
- No authentication implemented - designed for trusted networks
- Data transmitted in plaintext - use reverse proxy with SSL for production
- Only supports text and HTML clipboard content (no images)

## Performance

The containerized server is lightweight and efficient:
- Memory usage: ~10-20MB
- CPU usage: Minimal when idle
- Network: Low bandwidth usage
- Startup time: < 1 second

For production deployments with multiple clients, consider:
- Setting appropriate resource limits
- Using a reverse proxy for SSL termination
- Implementing proper logging aggregation
- Setting up monitoring alerts
