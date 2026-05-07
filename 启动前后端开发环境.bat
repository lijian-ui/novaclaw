@echo off
chcp 65001 >nul
echo ==============================
echo  Start NovaClaw Frontend + Backend
echo ==============================

start "NovaClaw-Frontend" cmd /k "cd /d C:\project\NovaClaw && npm run dev"
start "NovaClaw-Backend" cmd /k "cd /d C:\project\NovaClaw\backend && cargo run"

echo.
echo Started successfully!
pause >nul