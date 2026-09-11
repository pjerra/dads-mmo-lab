#!/bin/bash
echo "# gamescope and a Vulkan device on this box"
echo "stamp: $(date -Is)"
echo
echo "## the pacman line that was run (Half 2, first step)"
echo "  printf 'yulon\\n' | sudo -S -p '' pacman -Sy --noconfirm --needed gamescope vulkan-swrast"
echo "  exit 0; the transaction pulled sdl3, sdl2-compat, seatd, xcb-util-errors, xorg-xwayland, gamescope"
echo
echo "## versions"
gamescope --version 2>&1 | head -2 | sed 's/\x1b\[[0-9;]*m//g' | sed "s/^/  > /"
pacman -Q gamescope vulkan-swrast xorg-xwayland 2>&1 | sed "s/^/  > /"
echo
echo "## the Vulkan ICDs the loader can see (this box has no GPU)"
ls /usr/share/vulkan/icd.d/ 2>&1 | sed "s/^/  > /"
echo
echo "## the machine"
echo "  > display adapters (lspci):"
lspci 2>/dev/null | grep -iE 'vga|3d controller|display' | sed "s/^/      /" || echo "      lspci found none / not installed"
echo "  > /dev/dri: $(ls /dev/dri 2>&1 | tr '\n' ' ')"
echo "  > X server: $(pgrep -a Xorg | head -1)"
echo
echo "## memory before gamescope was started"
free -m | head -2 | sed "s/^/  > /"
