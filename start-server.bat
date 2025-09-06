@echo off

REM Change to the directory where the script is located
cd /d "%~dp0"

echo Запуск сервера буфера обмена...
echo Сервер будет доступен на http://127.0.0.1:8080
echo WebSocket: ws://127.0.0.1:8080/ws
echo Для остановки нажмите Ctrl+C
echo.

set RUST_LOG=info
clipboard-server.exe
