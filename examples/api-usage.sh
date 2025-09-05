#!/bin/bash

# Примеры использования API сервера буфера обмена

echo "=== Примеры использования API ==="
echo ""

# Получить текущее содержимое буфера обмена
echo "1. Получение текущего содержимого буфера обмена:"
echo "curl http://127.0.0.1:8080/api/clipboard"
echo ""

# Установить содержимое буфера обмена
echo "2. Установка содержимого буфера обмена:"
echo 'curl -X POST http://127.0.0.1:8080/api/clipboard \\'
echo '  -H "Content-Type: application/json" \\'
echo '  -d '\''{"content": "Привет, мир!", "timestamp": 1694234567}'\'''
echo ""

# WebSocket подключение
echo "3. WebSocket подключение для real-time синхронизации:"
echo "ws://127.0.0.1:8080/ws"
echo ""

echo "Для тестирования запустите:"
echo "./start-server.sh"
echo ""
echo "А затем в другом терминале:"
echo "./start-client.sh"
