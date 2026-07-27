@echo off
rem Start the BUILT DML Launcher (target\release\launcher.exe) against the
rem NATIVE Docker Desktop server. Double-click me.
rem
rem This is the release-build sibling of the dev-mode
rem "Start DML Launcher (native experiment).bat" on the Desktop, and it sets
rem the SAME four variables. All four matter:
rem
rem   DML_BACKEND   native  -> drive Docker Desktop directly. WITHOUT it the
rem                            launcher defaults to WSL mode, asks the bash CLI
rem                            inside the dml-arch distro, and correctly reports
rem                            that (different, stopped) install as offline.
rem   DML_GAMES_DIR         -> where the native titles live.
rem   DML_SCRIPT            -> the bash `dml` script. Still needed by the
rem                            features that have not been ported yet (install,
rem                            some module operations, self-update) and by the
rem                            Eluna bridge, whose lua source root is
rem                            <parent of DML_SCRIPT>\lua. Missing it is why
rem                            those features appear broken when the exe is
rem                            started straight from Explorer or the taskbar.
rem   DML_YQ_BIN            -> yq, used by the compose/override readers.
rem
rem %~dp0 = this script's folder, so it works wherever the repo is cloned.
title DML Launcher (native)
set "DML_BACKEND=native"
set "DML_GAMES_DIR=%USERPROFILE%\dml-native"
set "DML_SCRIPT=%~dp0cli\dml"
set "DML_YQ_BIN=%USERPROFILE%\dml-native\tools\yq.exe"
start "" "%~dp0target\release\launcher.exe"
