#!/usr/bin/env bash
# Put one of the gate's readings on the VM's own desktop, so the host can
# photograph it. GNOME blocks an in-guest screenshot from ssh, so the picture
# is taken from the Hyper-V side with vmshot.ps1 -- which photographs whatever
# is on the VM's screen, and nothing else.
#
# Every capture is therefore a picture of a real desktop on a real machine
# showing this file's real bytes, not a render of them.
set -u
FILE=${1:?usage: show86.sh <file under ~/gate86-out>}
TITLE=${2:-8.6 My Party}

export XDG_RUNTIME_DIR=/run/user/1000
export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
# The session is GNOME on WAYLAND, so DISPLAY=:0 alone reaches Xwayland with no
# cookie: "Authorization required, but no authorization protocol specified".
# Both are lifted out of a process ALREADY IN the session rather than guessed --
# the Xauthority file is named with a random suffix per boot
# (.mutter-Xwaylandauth.XXXXXX) and cannot be written down anywhere.
INSIDE=$(pgrep -u "$USER" -f "gnome-terminal|nautilus|gjs" | head -1)
if [ -n "$INSIDE" ]; then
    eval "$(tr '\0' '\n' < "/proc/$INSIDE/environ" \
        | grep -E '^(DISPLAY|WAYLAND_DISPLAY|XAUTHORITY)=' \
        | sed 's/^/export /')"
fi
export DISPLAY=${DISPLAY:-:0}

pkill -f "gate86-show" 2>/dev/null
sleep 1

cat > "$HOME/gate86-show.sh" <<EOF
#!/usr/bin/env bash
clear
cat "$HOME/gate86-out/$FILE"
echo
echo "-- 8.6 My Party, yulon-ubuntu, \$(date -u '+%Y-%m-%d %H:%M:%SZ') --"
sleep 3600
EOF
chmod +x "$HOME/gate86-show.sh"

gnome-terminal --title="gate86-show $TITLE" --maximize -- bash "$HOME/gate86-show.sh" &
sleep 4
echo "opened $FILE on the desktop"
