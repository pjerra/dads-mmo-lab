#!/bin/bash
# gamescope's own focus atoms on its nested X display, plus the window list.
# argv1 = label
export DISPLAY=:1 XAUTHORITY=/home/pk/.Xauthority
echo "--- $1  ($(date -Is))"
for a in GAMESCOPE_FOCUSED_APP GAMESCOPE_FOCUSED_APP_GFX GAMESCOPE_FOCUSED_WINDOW GAMESCOPECTRL_BASELAYER_APPID GAMESCOPECTRL_BASELAYER_WINDOW GAMESCOPE_KEYBOARD_FOCUS_DISPLAY; do
  v=$(xprop -root "$a" 2>&1)
  echo "    $v"
done
echo "    windows on :1: $(xdotool search --name '.' getwindowname %@ 2>/dev/null | tr '\n' '|')"
echo "    steam-tracked children: $(pgrep -c -f 'reaper SteamLaunch' 2>/dev/null || echo 0)"
