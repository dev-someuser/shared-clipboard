# Docker Deployment для Shared Clipboard Server

Этот файл содержит инструкции по развертыванию сервера буфера обмена в Docker контейнере.

## Быстрый старт

### С помощью Docker Compose (рекомендуется)

```bash
# Сборка и запуск
docker-compose up -d

# Просмотр логов
docker-compose logs -f clipboard-server

# Остановка
docker-compose down
```

### С помощью Docker CLI

```bash
# Сборка образа
docker build -t shared-clipboard-server .

# Запуск контейнера
docker run -d \
  --name clipboard-server \
  -p 8080:8080 \
  -e RUST_LOG=info \
  shared-clipboard-server

# Просмотр логов
docker logs -f clipboard-server

# Остановка и удаление
docker stop clipboard-server
docker rm clipboard-server
```

## Конфигурация

### Переменные окружения

| Переменная | Значение по умолчанию | Описание |
|------------|----------------------|----------|
| `RUST_LOG` | `info` | Уровень логирования (`debug`, `info`, `warn`, `error`) |
| `RUST_BACKTRACE` | `1` | Включение трассировки стека при ошибках |

### Порты

- **8080**: HTTP API и WebSocket сервер

## Производственное развертывание

### С обратным прокси (Nginx)

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
        
        # WebSocket поддержка
        proxy_read_timeout 86400;
        proxy_send_timeout 86400;
    }
}
```

### С SSL/TLS (Let's Encrypt)

```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  clipboard-server:
    build: .
    container_name: shared-clipboard-server
    ports:
      - "127.0.0.1:8080:8080"  # Привязка только к localhost
    environment:
      - RUST_LOG=warn  # Меньше логов в продакшене
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

После запуска контейнера доступны следующие endpoints:

- **GET /api/clipboard** - Получить текущее содержимое буфера обмена
- **POST /api/clipboard** - Установить содержимое буфера обмена
- **WebSocket /ws** - WebSocket соединение для real-time обновлений

### Примеры использования

```bash
# Проверка работоспособности
curl http://localhost:8080/api/clipboard

# Установка содержимого
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
```

## Мониторинг

### Health Check

```bash
# Проверка состояния контейнера
docker-compose ps

# Ручная проверка health check
curl -f http://localhost:8080/api/clipboard
```

### Логи

```bash
# Просмотр логов
docker-compose logs clipboard-server

# Логи в реальном времени
docker-compose logs -f clipboard-server

# Логи с временными метками
docker-compose logs -t clipboard-server
```

### Метрики ресурсов

```bash
# Использование ресурсов
docker stats clipboard-server

# Детальная информация
docker inspect clipboard-server
```

## Устранение неполадок

### Основные проблемы

1. **Контейнер не запускается**
   ```bash
   docker-compose logs clipboard-server
   ```

2. **Порт уже занят**
   ```bash
   # Найти процесс использующий порт 8080
   lsof -i :8080
   
   # Или изменить порт в docker-compose.yml
   ports:
     - "8081:8080"
   ```

3. **Проблемы с сетью**
   ```bash
   # Проверить сетевые настройки
   docker network ls
   docker inspect clipboard-server
   ```

### Отладка

```bash
# Запуск в интерактивном режиме
docker run -it --rm -p 8080:8080 shared-clipboard-server

# Подключение к работающему контейнеру
docker exec -it clipboard-server /bin/sh

# Пересборка без кеша
docker-compose build --no-cache
```

## Обновление

```bash
# Остановка сервиса
docker-compose down

# Пересборка образа
docker-compose build

# Запуск обновленной версии
docker-compose up -d

# Проверка обновления
curl http://localhost:8080/api/clipboard
```
