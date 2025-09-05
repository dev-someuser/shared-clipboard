#!/bin/bash

set -e

cd "$(dirname "$0")"

echo "Запуск клиента буфера обмена..."
echo "Подключение к серверу: ${CLIPBOARD_SERVER_URL:-http://127.0.0.1:8080}"
echo "Демон будет синхронизировать локальный буфер обмена с сервером"
echo "Для остановки нажмите Ctrl+C"
echo ""

RUST_LOG=info cargo run --release --bin clipboard-client
