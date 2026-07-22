@echo off
rem Start the DML Launcher in dev mode. Double-click me.
rem %~dp0 = this script's folder, so it works wherever the repo is cloned.
title DML Launcher (dev)
cd /d "%~dp0launcher"
call npm run tauri dev
rem Keep the window open if the dev server exits/crashes so the error is readable.
pause
