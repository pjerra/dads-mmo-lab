@echo off
rem Dad's MMO Lab -- one-time setup for the PORTABLE DML Launcher.
rem Double-click me from inside the unpacked zip folder.
rem
rem Runs Setup-DML.ps1 as Administrator with -All: enables WSL2 if missing
rem (may ask for a restart, then continues by itself), installs Docker Desktop,
rem Git for Windows and the WebView2 runtime via winget, and points the
rem launcher settings at this folder. Nothing is downloaded for the launcher
rem itself -- it is already here as launcher.exe.
set "HERE=%~dp0"
if "%HERE:~-1%"=="\" set "HERE=%HERE:~0,-1%"
if not exist "%HERE%\launcher.exe" (
    echo launcher.exe was not found next to this file.
    echo Unpack the WHOLE zip first, then run Setup-DML.bat from inside that folder.
    pause
    exit /b 1
)
powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process powershell -Verb RunAs -ArgumentList @('-NoExit','-ExecutionPolicy','Bypass','-File',('\"{0}\Setup-DML.ps1\"' -f $env:HERE),'-All','-LauncherDir',('\"{0}\"' -f $env:HERE))"
if errorlevel 1 (
    echo.
    echo The Administrator prompt was declined. Setup needs it to enable WSL2.
    pause
)
