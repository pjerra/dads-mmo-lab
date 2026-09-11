#!/usr/bin/env bash
# T26 live half, step 03 -- the client seat.
#
# THIS ONE RUNS ON THE LAPTOP and drives `vmhost` (the Hyper-V host,
# DESKTOP-FP27AUV) over ssh, because that is where the 3.3.5a client is. Every
# other step in this folder runs on `yulon-ubuntu2`.
#
# WHY NOT `yulon-win11`, which the brief names as the client seat: that box has
# no 3.3.5a client at all (`D:\clients` holds a TBC 2.4.3 tree and two zips),
# and the TBC client launched there for a renderer probe drew no window --
# `MainWindowHandle` 0 and a zero-byte `Logs\gx.log` after two minutes -- while
# `vmshot.ps1 -VMName yulon-win11` photographs an all-black 1024x768 console
# frame although the guest's own `CopyFromScreen` shows a 1920x1080 desktop. The
# probe is `03a-win11-probe.log`. The host's client is the one 8.4a and 8.5a
# drove into the world, so it is the one this lane uses.
#
# The password is written to `C:\Users\PK\t26\pw.txt` on the host, read once by
# `t26client.ps1 -Stage login`, and deleted by step 08. It is in no log here.
set -u

STAGE=${1:?stage}
shift || true

# The argument line is BASE64-ENCODED for the trip. Measured at 01:59 and
# again at 02:00: a value carrying spaces does not survive bash -> ssh ->
# Windows sshd -> PowerShell with either kind of quote -- once the text reached
# the client as `/console` alone, once `t26run.ps1` was handed six arguments.
ARGS="-Stage $STAGE"
for a in "$@"; do
    case "$a" in
        *\ *) ARGS="$ARGS \"$a\"" ;;
        *)    ARGS="$ARGS $a" ;;
    esac
done
B64=$(printf '%s' "$ARGS" | base64 -w0)
ssh vmhost "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\Users\\PK\\t26\\t26run.ps1 -B64 $B64"
