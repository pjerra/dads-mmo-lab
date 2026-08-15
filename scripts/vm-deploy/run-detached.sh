#!/bin/bash
# Run a LONG command on a remote Windows box so it survives the SSH session.
#
# Usage:
#   scripts/vm-deploy/run-detached.sh --host perzi@100.99.161.102 \
#       --name dml-rebuild-now \
#       --cmd 'dml-wow.exe module rebuild --backup' \
#       [--key ~/.ssh/dml_vm] \
#       [--workdir 'C:\Users\perzi\dml-deploy'] \
#       [--env 'DML_GAMES_DIR=C:\Users\perzi\dml-native']
#
# WHY THIS EXISTS (2026-08-15, cost two dead rebuilds):
#
# Windows OpenSSH kills the child process tree when the session ends. A
# 90-minute `module rebuild` started with `Start-Process -NoNewWindow -PassThru`
# over SSH therefore died about a minute after disconnecting -- twice -- with
# no error in its own output, because it was KILLED rather than failed. Its
# captured stdout simply stopped mid-sentence, which reads exactly like a
# crash and sent the diagnosis in the wrong direction.
#
# The fix is to hand ownership to the machine: a scheduled task runs the job
# under the user's own session, so it outlives the connection entirely.
#
# A second trap this avoids: streaming a live process's stdout through
# PowerShell's `Start-Job` does NOT work -- the Process object cannot be
# marshalled across the job boundary, so nothing drains the pipe and the child
# dies when it fills. Always redirect to a FILE and tail that instead.
#
# `/it` (interactive) is deliberate: Docker Desktop is per-user on this box and
# its engine is only reachable from the logged-on session.
set -uo pipefail

HOST=""; NAME=""; CMD=""
KEY="$HOME/.ssh/dml_vm"
WORKDIR='C:\Users\perzi\dml-deploy'
ENVLINE=""

while [ $# -gt 0 ]; do
  case "$1" in
    --host) HOST="$2"; shift 2 ;;
    --name) NAME="$2"; shift 2 ;;
    --cmd) CMD="$2"; shift 2 ;;
    --key) KEY="$2"; shift 2 ;;
    --workdir) WORKDIR="$2"; shift 2 ;;
    --env) ENVLINE="$2"; shift 2 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$HOST" ] && [ -n "$NAME" ] && [ -n "$CMD" ] || {
  echo "ERROR: --host, --name and --cmd are all required (see --help)" >&2; exit 2; }
case "$NAME" in *[!A-Za-z0-9._-]*) echo "ERROR: --name must be [A-Za-z0-9._-]+" >&2; exit 2 ;; esac

SSH="ssh -i $KEY -o StrictHostKeyChecking=no -o ConnectTimeout=20 -o BatchMode=yes $HOST"
LOG="$WORKDIR\\$NAME.log"
ERR="$WORKDIR\\$NAME.err"

# The wrapper .cmd exists for two reasons: schtasks /tr loses quoting around
# paths with spaces, and redirection has to happen inside the job (the task
# scheduler gives it no stdout).
PS="\$lines = @(
 '@echo off',
 $( [ -n "$ENVLINE" ] && echo "'set $ENVLINE'," )
 'cd /d $WORKDIR',
 '$CMD > $LOG 2> $ERR',
 'echo EXITCODE=%errorlevel% >> $LOG'
)
Set-Content -Path '$WORKDIR\\run-$NAME.cmd' -Value \$lines -Encoding ascii
schtasks /delete /tn $NAME /f 2>\$null | Out-Null
schtasks /create /tn $NAME /tr '$WORKDIR\\run-$NAME.cmd' /sc once /st 23:59 /it /f | Out-Null
schtasks /run /tn $NAME | Out-Null
Start-Sleep -Seconds 15
schtasks /query /tn $NAME /fo LIST /v | Select-String -Pattern Status
Write-Output '--- first output ---'
Get-Content '$LOG' -Tail 5 -ErrorAction SilentlyContinue"

$SSH "powershell -NoProfile -EncodedCommand $(printf '%s' "$PS" | iconv -f UTF-8 -t UTF-16LE | base64 -w0)" 2>/dev/null \
  | grep -v -E "CLIXML|<Objs|Preparing modules"

echo
echo "Task '$NAME' launched. It now belongs to the remote machine, not to this SSH session."
echo "Follow it with:  ssh ... \"powershell -NoProfile -Command \\\"Get-Content '$LOG' -Tail 20\\\"\""
