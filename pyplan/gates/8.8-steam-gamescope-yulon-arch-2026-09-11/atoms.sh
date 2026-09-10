#!/bin/bash
# gamescope's own focus atoms, BOTH of its XWayland displays with each window's
# _NET_WM_PID, and the reaper line for the launched entry. argv1 = label.
#
# Round 2: the round-1 copy read the atoms and listed windows on :1 only, so the
# ":2 holds Yu'lon's window" and "_NET_WM_PID is what gamescope keys on" sentences
# in the README had no capture behind them. Both are read here.
export XAUTHORITY=/home/pk/.Xauthority
echo "--- $1  ($(date -Is))"
echo "  [atoms on the gamescope X display :1]"
for a in GAMESCOPE_FOCUSED_APP GAMESCOPE_FOCUSED_APP_GFX GAMESCOPE_FOCUSED_WINDOW \
         GAMESCOPECTRL_BASELAYER_APPID GAMESCOPECTRL_BASELAYER_WINDOW GAMESCOPE_KEYBOARD_FOCUS_DISPLAY; do
  echo "    $(DISPLAY=:1 xprop -root "$a" 2>&1)"
done
for d in 1 2; do
  echo "  [windows on :$d  (id / _NET_WM_PID / name)]"
  for w in $(DISPLAY=:$d xdotool search --name '.' 2>/dev/null); do
    n=$(DISPLAY=:$d xdotool getwindowname "$w" 2>/dev/null)
    p=$(DISPLAY=:$d xprop -id "$w" _NET_WM_PID 2>/dev/null | sed 's/.*= //')
    echo "    $w  pid=${p:-<none>}  $n"
  done
done
echo "  [the entry's own process, as Steam launched it]"
pgrep -af 'reaper SteamLaunch' | sed 's/^/    /' || echo "    <no reaper SteamLaunch process>"
pgrep -af '[m]ain\.py' | sed 's/^/    /'
echo "  [memory]"
free -m | sed -n 2p | sed 's/^/    /'
