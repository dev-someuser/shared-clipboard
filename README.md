# Shared Clipboard

Система общего буфера обмена, состоящая из сервера на Rust и клиента-демона для Linux.

## Архитектура

Проект состоит из двух компонентов:

1. **Сервер** (`/server`) - принимает запросы на вставку текста в буфер обмена и уведомляет всех подключенных клиентов о новых данных
2. **Клиент** (`/client`) - демон для Linux, который отслеживает изменения в локальном буфере обмена и синхронизирует их с сервером

## Особенности

- Поддержка как X11, так и Wayland (через библиотеку `arboard`)
- WebSocket соединения для real-time уведомлений
- HTTP API для отправки данных в буфер обмена
- Автоматическая синхронизация буфера обмена между всеми подключенными клиентами
- Логирование с помощью `tracing`

## Установка и сборка

### Предварительные требования

- Rust 1.70+ с Cargo
- На Linux: X11 или Wayland сессия
- Системные пакеты для работы с буфером обмена:
  ```bash
  # Ubuntu/Debian
  sudo apt install libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
  
  # Arch Linux  
  sudo pacman -S libxcb
  
  # Fedora
  sudo dnf install libxcb-devel
  ```

### Сборка

```bash
# Сборка сервера
cd server
cargo build --release

# Сборка клиента
cd ../client  
cargo build --release
```

## Использование

### Запуск сервера

```bash
cd server
cargo run --release
```

Сервер будет доступен на `http://127.0.0.1:8080`

Доступные эндпоинты:
- `GET /api/clipboard` - получить текущее содержимое буфера обмена
- `POST /api/clipboard` - установить содержимое буфера обмена
- `ws://127.0.0.1:8080/ws` - WebSocket соединение для real-time обновлений

### Запуск клиента

```bash
cd client
cargo run --release
```

По умолчанию клиент подключается к `http://127.0.0.1:8080`. Для изменения адреса сервера используйте переменную окружения:

```bash
CLIPBOARD_SERVER_URL=http://192.168.1.100:8080 cargo run --release
```

### Запуск как демон

Для запуска клиента как системного сервиса создайте файл `/etc/systemd/system/clipboard-client.service`:

```ini
[Unit]
Description=Shared Clipboard Client
After=graphical-session.target

[Service]
Type=simple
User=yourusername
Environment=DISPLAY=:0
Environment=CLIPBOARD_SERVER_URL=http://127.0.0.1:8080
ExecStart=/path/to/clipboard-client
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
```

Затем:

```bash
sudo systemctl enable clipboard-client.service
sudo systemctl start clipboard-client.service
```

## API

### HTTP API

#### Получить содержимое буфера обмена

```bash
curl http://127.0.0.1:8080/api/clipboard
```

#### Установить содержимое буфера обмена

```bash
curl -X POST http://127.0.0.1:8080/api/clipboard \
  -H "Content-Type: application/json" \
  -d '{"content": "Hello World!", "timestamp": 1694234567}'
```

### WebSocket API

Подключение к WebSocket: `ws://127.0.0.1:8080/ws`

#### Сообщения от сервера к клиенту

```json
{
  "type": "clipboard_update",
  "data": {
    "content": "текст из буфера обмена",
    "timestamp": 1694234567
  }
}
```

#### Сообщения от клиента к серверу

```json
{
  "type": "clipboard_set", 
  "data": {
    "content": "новый текст для буфера обмена",
    "timestamp": 1694234567
  }
}
```

## Логи

Оба компонента используют `tracing` для логирования. Уровень логирования можно настроить через переменную окружения `RUST_LOG`:

```bash
RUST_LOG=info cargo run --release    # Информационные сообщения
RUST_LOG=debug cargo run --release   # Подробные сообщения
RUST_LOG=warn cargo run --release    # Только предупреждения и ошибки
```

## Безопасность

- Сервер по умолчанию слушает только на localhost (127.0.0.1)
- Нет аутентификации - система предназначена для использования в доверенной сети
- Данные передаются в открытом виде

## Ограничения

- Поддерживается только синхронизация текста (не изображения или файлы)
- Требуется графическая сессия для работы с буфером обмена
- Клиент работает только на Linux

## Устранение неполадок

### Ошибки доступа к буферу обмена

Убедитесь, что:
- У вас запущена X11 или Wayland сессия
- Установлены необходимые системные библиотеки
- Переменная `DISPLAY` установлена правильно

### Проблемы с подключением

- Проверьте, что сервер запущен и доступен
- Убедитесь в корректности URL сервера
- Проверьте логи на наличие ошибок сети
