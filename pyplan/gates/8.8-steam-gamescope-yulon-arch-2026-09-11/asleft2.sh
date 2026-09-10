#!/bin/bash
echo "# Half 2, the box as left"
echo "stamp: $(date -Is)"
echo
echo "## processes"
pgrep -x steam >/dev/null && echo "  > steam runs (pid $(pgrep -x steam | head -1))" || echo "  > NO steam process"
pgrep -f 'steam.sh.*bigpicture' >/dev/null && echo "  > and it was started with -bigpicture" || echo "  > -bigpicture not on its argv"
pgrep -c gamescope >/dev/null 2>&1 && [ "$(pgrep -c gamescope)" -gt 0 ] && echo "  > gamescope runs" || echo "  > no gamescope process"
pgrep -f "[m]ain\.py" >/dev/null && echo "  > a Yu'lon main.py runs" || echo "  > no Yu'lon process"
pgrep -f "[W]oW.exe" >/dev/null && echo "  > the client runs" || echo "  > no client process"
echo
echo "## the window in front on :0"
DISPLAY=:0 XAUTHORITY=/home/pk/.Xauthority xdotool getactivewindow getwindowname 2>&1 | sed 's/^/  > /'
echo
echo "## containers"
docker ps -a --format "{{.Names}}|{{.Status}}" | sed 's/^/  > /'
echo
echo "## ports 3724 / 8090"
ss -ltn | grep -E "3724|8090" | sed 's/^/  > /' || echo "  > nothing listening on either port"
echo
echo "## packages this ticket added to the box"
pacman -Q gamescope vulkan-swrast vulkan-tools 2>&1 | sed 's/^/  > /'
echo
echo "## Yu'lon's state dir"
ls -la ~/.local/share/yulon/ | sed 's/^/  > /'
echo
echo "## the scratch tree this run used"
ls ~/t29 | tr '\n' ' ' | sed 's/^/  > /'
echo
echo "## memory"
free -m | head -2 | sed 's/^/  > /'
