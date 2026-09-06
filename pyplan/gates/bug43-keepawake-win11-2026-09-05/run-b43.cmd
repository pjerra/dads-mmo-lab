@echo off
del /f /q C:\gate\evidence\b43.exitcode 2>nul
echo ==== install start %DATE% %TIME% source=0ad9d99a bug-checklist section 43 keep-awake press >> C:\gate\evidence\b43-wrapper.log
cd /d C:\gate\b43-src\b43-src\pylauncher
C:\Users\pk\co\dads-mmo-lab-yulon-phase7\pylauncher\.venv\Scripts\python.exe -u -m yulon.install_wiring wow-tbc --server-dir C:\gate\tbc-server --client-dir C:\gate\client\WoW-Client-2.4.3 > C:\gate\evidence\b43.log 2>&1
echo %ERRORLEVEL% > C:\gate\evidence\b43.exitcode
echo ==== install end %DATE% %TIME% errorlevel %ERRORLEVEL% >> C:\gate\evidence\b43-wrapper.log
