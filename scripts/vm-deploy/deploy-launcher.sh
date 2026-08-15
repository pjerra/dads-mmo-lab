#!/bin/bash
# Deploy a locally built DML Launcher to a remote Windows box over SSH.
#
# Usage (from the repo root, after `npm run tauri build`):
#   scripts/vm-deploy/deploy-launcher.sh --host perzi@100.99.161.102 \
#       [--key ~/.ssh/dml_vm] \
#       [--installer "target/release/bundle/nsis/DML Launcher_0.1.0_x64-setup.exe"] \
#       [--staging 'C:\Users\perzi\dml-deploy'] \
#       [--app 'C:\Users\perzi\AppData\Local\DML Launcher']
#
# Every step below is here because the obvious version of it went wrong on a
# real machine (2026-08-15, dad's server). Read the comments before
# simplifying any of them.
set -uo pipefail

HOST=""
KEY="$HOME/.ssh/dml_vm"
INSTALLER="target/release/bundle/nsis/DML Launcher_0.1.0_x64-setup.exe"
STAGING='C:\Users\perzi\dml-deploy'
APP='C:\Users\perzi\AppData\Local\DML Launcher'

while [ $# -gt 0 ]; do
  case "$1" in
    --host) HOST="$2"; shift 2 ;;
    --key) KEY="$2"; shift 2 ;;
    --installer) INSTALLER="$2"; shift 2 ;;
    --staging) STAGING="$2"; shift 2 ;;
    --app) APP="$2"; shift 2 ;;
    -h|--help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$HOST" ] || { echo "ERROR: --host is required (see --help)" >&2; exit 2; }
[ -f "$INSTALLER" ] || { echo "ERROR: installer not found: $INSTALLER" >&2; exit 2; }

SSH="ssh -i $KEY -o StrictHostKeyChecking=no -o ConnectTimeout=20 -o BatchMode=yes $HOST"
say() { printf '\n=== %s ===\n' "$1"; }

# ---------------------------------------------------------------------------
# 0. Refuse ONLY if the launcher is driving a long operation.
#
# The question is not "is a build running" but "would stopping the launcher
# kill it". A rebuild started from the launcher's own Modules page is a child
# of launcher.exe and dies with it; one started as a detached scheduled task
# hangs off cmd.exe and does not care:
#     dml-wow.exe pid=3440 parent=5496 (cmd.exe)   <- survives a launcher swap
#     docker.exe  pid=4932 parent=3440 (dml-wow.exe)
#
# And the test is process AGE, not existence: the launcher ALWAYS has a
# transient `docker.exe` child because it polls server status every few
# seconds, so an existence check refused every deploy forever. A poll lives
# seconds, a build for an hour. A CommandLine read is not usable to tell them
# apart either -- CIM returns it empty for these short-lived children.
# ---------------------------------------------------------------------------
say "0. refuse only if the launcher is driving a long operation"
PROBE_PS='$L = (Get-CimInstance Win32_Process | Where-Object Name -eq launcher.exe | ForEach-Object { $_.ProcessId }); $old = @(); if ($L) { $old = Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -in $L -and $_.Name -in @("dml-wow.exe","docker.exe","cmd.exe") -and ((Get-Date) - $_.CreationDate).TotalSeconds -gt 60 } }; Write-Output ("LAUNCHER_KIDS=" + ($old | Measure-Object).Count); Get-CimInstance Win32_Process | Where-Object Name -eq docker.exe | ForEach-Object { "BUILD=" + $_.CommandLine }; Write-Output ==PROBE_OK=='
probe=$($SSH "powershell -NoProfile -EncodedCommand $(printf '%s' "$PROBE_PS" | iconv -f UTF-8 -t UTF-16LE | base64 -w0)" 2>/dev/null | tr -d '\r')
if ! echo "$probe" | grep -q "PROBE_OK"; then
  echo "ABORT: could not probe $HOST. Nothing was touched."; exit 1
fi
kids=$(echo "$probe" | sed -n 's/^LAUNCHER_KIDS=\([0-9]*\).*/\1/p')
if [ "${kids:-0}" != "0" ]; then
  echo "ABORT: the launcher has $kids long-lived child process(es) -- it is probably driving a build that would die with it."; exit 1
fi
if echo "$probe" | grep -q "build ac-worldserver"; then
  echo "note: a DETACHED worldserver compile is running -- not a child of the launcher, so this deploy leaves it alone."
else
  echo "no long-running launcher operation in flight."
fi

say "1. stage the installer"
scp -i "$KEY" -o StrictHostKeyChecking=no "$INSTALLER" "$HOST:${STAGING//\\//}/DML-Launcher-setup.exe" >/dev/null || {
  echo "ABORT: could not copy the installer."; exit 1; }
$SSH "powershell -NoProfile -Command \"Get-Item '$STAGING\\DML-Launcher-setup.exe' | ForEach-Object { Write-Output (\$_.Length.ToString() + ' bytes staged') }\"" 2>/dev/null

say "2. record what is there now"
$SSH "powershell -NoProfile -Command \"Get-Item '$APP\\launcher.exe' | ForEach-Object { Write-Output ('BEFORE ' + \$_.Length + ' bytes ' + \$_.LastWriteTime.ToString('s')) }\"" 2>/dev/null

# Back up BEFORE stopping anything, so "undo" exists ahead of the first
# irreversible step rather than after it.
say "3. back up the existing install"
$SSH "powershell -NoProfile -Command \"\$b='$STAGING\\launcher-backup'; if (Test-Path \$b) { Remove-Item -Recurse -Force \$b }; Copy-Item -Recurse '$APP' \$b; Write-Output ('backed up to ' + \$b + ' (' + (Get-ChildItem -Recurse \$b | Measure-Object).Count + ' items)')\"" 2>/dev/null

say "4. stop the running launcher"
$SSH 'powershell -NoProfile -Command "Get-Process launcher -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Seconds 3; Write-Output (\"launcherProcs=\" + (Get-Process launcher -ErrorAction SilentlyContinue | Measure-Object).Count)"' 2>/dev/null

say "5. silent install"
$SSH "powershell -NoProfile -Command \"\$p = Start-Process -FilePath '$STAGING\\DML-Launcher-setup.exe' -ArgumentList '/S' -Wait -PassThru; Write-Output ('installer exit=' + \$p.ExitCode)\"" 2>/dev/null

# Judge the install by EFFECT. The installer's exit code is 0 on paths that
# changed nothing; only the binary's own size/timestamp proves a swap.
say "6. verify the binary actually changed"
$SSH "powershell -NoProfile -Command \"Get-Item '$APP\\launcher.exe' | ForEach-Object { Write-Output ('AFTER  ' + \$_.Length + ' bytes ' + \$_.LastWriteTime.ToString('s')) }\"" 2>/dev/null

# ---------------------------------------------------------------------------
# 7. Restart in the INTERACTIVE session.
#
# Two separate traps, both hit for real:
#   * A GUI app started over SSH lands in session 0 and is invisible on the
#     user's desktop -- hence the schtasks /it driver.
#   * schtasks /tr LOSES the quoting around a path containing spaces, so
#     'C:\...\DML Launcher\launcher.exe' became 'C:\...\DML' plus an argument
#     and failed with 0x80070002 (file not found). The task therefore points
#     at a wrapper .cmd on a space-free path, written here.
# ---------------------------------------------------------------------------
say "7. restart in the interactive session"
WRAP_PS="\$q=[char]34; \$body='start ' + \$q + \$q + ' ' + \$q + '$APP\\launcher.exe' + \$q; Set-Content -Path '$STAGING\\start-launcher.cmd' -Value \$body -Encoding ascii
schtasks /delete /tn dml-launcher-start /f 2>\$null | Out-Null
schtasks /create /tn dml-launcher-start /tr '$STAGING\\start-launcher.cmd' /sc once /st 23:59 /it /f | Out-Null
schtasks /run /tn dml-launcher-start | Out-Null
Start-Sleep -Seconds 10
\$p = Get-Process launcher -ErrorAction SilentlyContinue
Write-Output ('launcherProcs=' + (\$p | Measure-Object).Count)
if (\$p) { \$p | ForEach-Object { Write-Output ('started=' + \$_.StartTime.ToString('s')) } }
schtasks /query /tn dml-launcher-start /fo LIST /v | Select-String -Pattern Result"
$SSH "powershell -NoProfile -EncodedCommand $(printf '%s' "$WRAP_PS" | iconv -f UTF-8 -t UTF-16LE | base64 -w0)" 2>/dev/null | grep -v -E "CLIXML|<Objs|Preparing modules"

say "done"
