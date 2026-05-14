@echo off
:: 全局禁用 QuickEdit 模式（防止点击 cmd 窗口导致进程输出挂起）
reg add HKCU\Console /v QuickEdit /t REG_DWORD /d 0 /f >nul 2>&1

start "NovaClaw-Backend" cmd /k "cd /d C:\project\NovaClaw\backend && cargo run"
