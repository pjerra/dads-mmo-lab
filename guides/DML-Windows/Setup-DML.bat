@echo off
rem Dad's MMO Lab -- one-time setup for the PORTABLE DML Launcher.
rem Double-click me from inside the unpacked zip folder.
rem
rem It self-elevates (one UAC prompt), then runs Setup-DML.ps1 -All, which
rem installs what the launcher needs -- WSL2, Docker Desktop, Git for Windows,
rem WebView2 -- and points the launcher at this folder. If it enables WSL2 it
rem will restart and finish by itself. Nothing is downloaded for the launcher
rem itself: it is already here as launcher.exe.
setlocal
set "HERE=%~dp0"
if "%HERE:~-1%"=="\" set "HERE=%HERE:~0,-1%"

if not exist "%HERE%\launcher.exe" (
    echo launcher.exe was not found next to this file.
    echo Unpack the WHOLE zip first, then run Setup-DML.bat from inside that folder.
    pause
    exit /b 1
)

rem Already elevated? `net session` needs admin, so its success is the test.
net session >nul 2>&1
if %errorlevel% equ 0 goto :run

rem Not elevated -- relaunch THIS script elevated (one UAC prompt), then stop
rem this unelevated copy. Passing only the .bat's own path avoids the nested
rem argument-array quoting that the previous version got wrong.
echo Requesting Administrator (one prompt)...
powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
exit /b 0

:run
rem Elevated from here on. Plain -File call -- no RunAs, no argument array.
powershell -NoProfile -ExecutionPolicy Bypass -File "%HERE%\Setup-DML.ps1" -All -LauncherDir "%HERE%"
echo.
echo Setup finished. You can close this window.
pause
