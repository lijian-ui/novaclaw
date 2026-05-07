@echo off
chcp 65001 >nul
echo ==============================
echo  Stop all services and close windows
echo ==============================

taskkill /f /im node.exe >nul 2>&1
taskkill /f /im cargo.exe >nul 2>&1
taskkill /f /im backend.exe >nul 2>&1

taskkill /f /fi "WINDOWTITLE:NovaClaw-Frontend*" >nul 2>&1
taskkill /f /fi "WINDOWTITLE:NovaClaw-Backend*" >nul 2>&1

echo All services stopped and windows closed.
pause >nul