@echo off
chcp 65001 >nul
echo ==============================
echo  Start NovaClaw Frontend + Backend
echo ==============================

start "NovaClaw-Frontend" cmd /k "cd /d C:\project\NovaClaw && npm run dev"


echo.
echo Started successfully!
