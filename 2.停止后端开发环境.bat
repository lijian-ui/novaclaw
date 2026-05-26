@echo off


taskkill /f /im cargo.exe >nul 2>&1
taskkill /f /im backend.exe >nul 2>&1


taskkill /f /fi "WINDOWTITLE:jeeves-Backend*" >nul 2>&1

