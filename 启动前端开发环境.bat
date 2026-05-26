@echo off
chcp 65001 >nul
echo ==============================
echo  Start jeeves Frontend + Backend
echo ==============================

start "jeeves-Frontend" cmd /k "cd /d d:\Project\jeeves && npm run dev"


echo.
echo Started successfully!
