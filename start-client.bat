@echo off

REM Change to the directory where the script is located
cd /d "%~dp0"

echo Запуск клиента буфера обмена...
if defined CLIPBOARD_SERVER_URL (
    echo Подключение к серверу: %CLIPBOARD_SERVER_URL%
) else (
    echo Подключение к серверу: http://127.0.0.1:8080
)
echo Демон будет синхронизировать локальный буфер обмена с сервером
echo Для остановки нажмите Ctrl+C
echo.

set RUST_LOG=info
clipboard-client.exe
