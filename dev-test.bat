@echo off
chcp 65001 >nul
title NovaClaw Dev Test

echo ==============================
echo  Building frontend...
echo ==============================
cd /d d:\Project\novaclaw
call npm run build
if %errorlevel% neq 0 (
    echo Frontend build failed!
    pause
    exit /b 1
)

echo.
echo ==============================
echo  Starting backend server...
echo ==============================
cd backend
cargo run
