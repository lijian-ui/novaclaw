@echo off
chcp 65001 >nul
echo ==============================
echo  Stop all services and close windows
echo ==============================

taskkill /f /im node.exe >nul 2>&1

taskkill /f /fi "WINDOWTITLE:NovaClaw-Frontend*" >nul 2>&1

