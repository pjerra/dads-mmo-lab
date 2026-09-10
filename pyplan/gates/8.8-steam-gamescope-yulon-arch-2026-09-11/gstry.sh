#!/bin/bash
export DISPLAY=:0 XAUTHORITY=/home/pk/.Xauthority
strip() { sed 's/\x1b\[[0-9;]*m//g'; }
try() {
  local label="$1"; shift
  echo
  echo "### $label"
  echo "\$ $*"
  echo "stamp: $(date -Is)"
  timeout 25 "$@" > /tmp/gs-try.log 2>&1
  echo "exit: $?"
  strip < /tmp/gs-try.log | grep -viE "scriptmgr|^$" | tail -14 | sed 's/^/    /'
}
echo "# gamescope on yulon-arch: every line tried, and what it answered"
echo "stamp: $(date -Is)"
echo
echo "The box is a Hyper-V VM with no GPU. gamescope 3.16.28, vulkan-swrast (lavapipe) 26.2.2."
try "the ticket's line, with a trivial child first"        gamescope -W 1920 -H 1080 -e -- sleep 10
try "backend sdl, named explicitly"                        gamescope --backend sdl -W 1920 -H 1080 -e -- sleep 10
try "backend headless"                                     gamescope --backend headless -W 1920 -H 1080 -e -- sleep 10
try "backend drm (X holds the only DRM node)"              gamescope --backend drm -W 1920 -H 1080 -e -- sleep 10
try "backend wayland (no wayland compositor here)"         gamescope --backend wayland -W 1920 -H 1080 -e -- sleep 10
try "sdl + --xwayland-count 2, the Deck's own shape"       gamescope --backend sdl -W 1920 -H 1080 --xwayland-count 2 -e -- sleep 10
try "nested, borderless, no -e"                            gamescope -W 1280 -H 720 -b -- sleep 10
