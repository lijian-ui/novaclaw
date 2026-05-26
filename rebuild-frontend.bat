@echo off
chcp 65001 >nul
cd /d d:\Project\jeeves
call npm run build
echo.
echo Build complete! Refresh your browser.
pause
