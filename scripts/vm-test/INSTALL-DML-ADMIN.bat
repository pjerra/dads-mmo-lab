@echo off
title DML - install everything (Administrator)

REM ===================================================================
REM  Double-click this. It elevates ONCE, up front, and every command
REM  below inherits that elevated token -- winget, wsl.exe, the Defender
REM  exclusions, the launcher installer, all of it. That is the fix for
REM  the failure this file exists to prevent: a setup that elevates for
REM  SOME steps leaves the rest half-installed, and nothing says which.
REM
REM  TWO THINGS IT DELIBERATELY TAKES OVER FROM THE PS1:
REM
REM  1. THE AUTO-RESTART. Install-DML-Native.ps1 counts down 60s and
REM     reboots by itself. This file runs it with stdin redirected from
REM     NUL, which the script reads as "no way to cancel a countdown" and
REM     therefore does not restart at all (its own documented rule: an
REM     unstoppable automatic restart is worse than the one asked for).
REM     Control comes back here instead.
REM  2. THE RESUME. The script queues itself in HKCU RunOnce, and Windows
REM     runs RunOnce entries WITHOUT elevation -- so the resumed half
REM     (Docker, Git, the Defender exclusions) would run unprivileged and
REM     fail. This file overwrites that entry with a pointer to ITSELF,
REM     so the resume re-elevates through UAC like the first run did.
REM
REM  NOT SHIPPABLE AS-IS -- ONE DECISION FIRST. It passes -InstallDocker
REM  unconditionally, which installs Docker Desktop silently. That is
REM  right for a test VM the owner asked for it on, and it CONTRADICTS
REM  the standing ruling in Install-DML-Native.ps1: Docker Desktop is a
REM  separate product whose licence (free personal, paid above a size
REM  threshold) is the user's decision, so the script instructs by
REM  default and only installs when asked. Before this becomes the
REM  consumer entry point, either drop -InstallDocker or put a visible
REM  yes/no in front of it. -InstallGit has no such problem: Git is GPL
REM  with no threshold.
REM ===================================================================

set "SELF=%~f0"
set "HERE=%~dp0"

REM --- 1. Elevate ----------------------------------------------------
REM S-1-16-12288 is the High Mandatory Level SID. Testing the SID rather
REM than parsing words keeps this working on a non-English Windows.
whoami /groups | find "S-1-16-12288" >nul
if errorlevel 1 goto elevate
goto elevated

:elevate
echo.
echo   This installer needs Administrator rights.
echo   Click YES on the Windows prompt that appears.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -Verb RunAs -FilePath $env:SELF"
exit /b

:elevated
pushd "%HERE%"
echo.
echo   ==============================================================
echo    DML native setup
echo    Running as Administrator - CONFIRMED
echo   ==============================================================
echo.

REM --- 2. The PowerShell installer has to be next to this file -------
set "PS1=%HERE%Install-DML-Native.ps1"
if not exist "%PS1%" goto no_ps1

REM --- 3. Is a locally built launcher sitting here too? ---------------
REM If yes, the PS1 must NOT fetch one from GitHub: that would install
REM the newest RELEASE over the build being tested. So -NoLauncher goes
REM on, and this file installs the local exe itself at the end.
set "LOCALEXE="
for %%F in ("%HERE%DML Launcher_*-setup.exe") do set "LOCALEXE=%%~fF"

set "ARGS=-InstallDocker -InstallGit"
if defined LOCALEXE set "ARGS=%ARGS% -NoLauncher"

if defined LOCALEXE echo   Launcher build found here - will install it at the end:
if defined LOCALEXE echo     %LOCALEXE%
if not defined LOCALEXE echo   No local launcher build here - the script will fetch the newest release.
echo.
echo   Starting: Install-DML-Native.ps1 %ARGS%
echo.

REM stdin from NUL on purpose - see the header. Do not remove it without
REM reading why, or this file loses control to a 60-second reboot.
powershell -NoProfile -ExecutionPolicy Bypass -File "%PS1%" %ARGS% <nul
set "RC=%ERRORLEVEL%"

echo.
if not "%RC%"=="0" goto not_done

REM --- 4. Success. Install the local build, if there is one. ---------
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\RunOnce" /v DMLNativeSetup /f >nul 2>&1
if not defined LOCALEXE goto done

echo   Installing the DML Launcher build from this folder...
start /wait "" "%LOCALEXE%" /S
set "IRC=%ERRORLEVEL%"

REM VERIFY. A silent installer that fails silently is the worst
REM combination there is, so something has to be on disk before this
REM claims anything. Same three roots the PS1 checks.
set "FOUND="
if exist "%LOCALAPPDATA%\DML Launcher\*.exe" set "FOUND=%LOCALAPPDATA%\DML Launcher"
if exist "%ProgramFiles%\DML Launcher\*.exe" set "FOUND=%ProgramFiles%\DML Launcher"
if exist "%ProgramFiles(x86)%\DML Launcher\*.exe" set "FOUND=%ProgramFiles(x86)%\DML Launcher"
if defined FOUND goto launcher_ok

echo.
echo   PROBLEM: the launcher installer exited %IRC% but no launcher is on disk.
echo   Run it by hand and watch for an error:
echo     %LOCALEXE%
echo.
goto finish

:launcher_ok
echo   Installed: %FOUND%
goto done

:done
echo.
echo   ==============================================================
echo    DONE. Open DML Launcher from the Start menu.
echo    It starts Docker Desktop itself - you do not need to first.
echo    The first server install builds from source and takes hours.
echo   ==============================================================
goto finish

:not_done
REM Queue OURSELVES, not the PS1, so the resume is elevated too. Windows
REM deletes a RunOnce value before running it, so this fires once.
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\RunOnce" /v DMLNativeSetup /t REG_SZ /d "\"%SELF%\"" /f >nul 2>&1
echo   ==============================================================
echo    NOT FINISHED YET (exit code %RC%)
echo.
echo    Read the yellow lines above - they name what is missing.
echo    If they say RESTART: restart this PC now. This file runs again
echo    by itself at logon and will ask for Administrator once more.
echo.
echo    If it does not start on its own, just double-click this file
echo    again. Running it twice is safe - it skips what is done.
echo   ==============================================================
goto finish

:no_ps1
echo   PROBLEM: Install-DML-Native.ps1 is not in this folder.
echo.
echo   This file and Install-DML-Native.ps1 have to sit together.
echo   Looked in: %HERE%
echo.
goto finish

:finish
echo.
popd
pause
