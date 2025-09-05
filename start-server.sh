#!/bin/bash

set -e

cd "$(dirname "$0")"

echo "Запуск сервера буфера обмена..."
echo "Сервер будет доступен на http://127.0.0.1:8080"
echo "WebSocket: ws://127.0.0.1:8080/ws"
echo "Для остановки нажмите Ctrl+C"
echo ""

RUST_LOG=info cargo run --release --bin clipboard-server
