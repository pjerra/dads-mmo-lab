#!/usr/bin/env bash
# Put one of this gate's readings on the VM's own desktop, so the Hyper-V host
# can photograph it with vmshot.ps1. GNOME blocks an in-guest screenshot driven
# from ssh, so the picture is taken from the host side and shows whatever is on
# the VM's screen -- a real desktop on a real machine showing this file's real
# bytes, not a render of them.
#
# Lifted from 8.6's show86.sh with the paths changed; the environment is read
# out of a process ALREADY IN the session because the Xauthority file is named
# with a random per-boot suffix and cannot be written down anywhere.
set -u
FILE=${1:?usage: show.sh <file under $GATE_OUT> [title] [tail-lines]}
TITLE=${2:-rebuild live}
TAIL=${3:-0}
OUT=${GATE_OUT:-$HOME/rebuild-gate-out}

export XDG_RUNTIME_DIR=/run/user/1000
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
INSIDE=$(pgrep -u "$USER" -f "gnome-terminal|nautilus|gjs" | head -1)
if [ -n "$INSIDE" ]; then
    eval "$(tr '\0' '\n' < "/proc/$INSIDE/environ" \
        | grep -E '^(DISPLAY|WAYLAND_DISPLAY|XAUTHORITY)=' \
        | sed 's/^/export /')"
fi
export DISPLAY=${DISPLAY:-:0}

pkill -f "rebuild-gate-show" 2>/dev/null
sleep 1

cat > "$HOME/rebuild-gate-show.sh" <<EOF
#!/usr/bin/env bash
clear
if [ "$TAIL" -gt 0 ]; then
    tail -n "$TAIL" "$OUT/$FILE"
else
    cat "$OUT/$FILE"
fi
echo
echo "-- $TITLE -- yulon-ubuntu2 -- \$(date -u '+%Y-%m-%d %H:%M:%SZ') --"
echo "-- ac-worldserver: \$(docker inspect -f '{{.State.Status}} pid={{.State.Pid}} restarts={{.RestartCount}}' ac-worldserver 2>&1) --"
sleep 3600
EOF
chmod +x "$HOME/rebuild-gate-show.sh"

gnome-terminal --title="rebuild-gate-show $TITLE" --maximize -- bash "$HOME/rebuild-gate-show.sh" &
sleep 4
echo "opened $FILE on the desktop"
