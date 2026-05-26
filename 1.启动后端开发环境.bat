@echo off

set CARGO_HOME=%USERPROFILE%\.cargo
start "jeeves-Backend" cmd /k "cd /d d:\Project\jeeves\backend && cargo run"
