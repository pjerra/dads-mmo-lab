#!/bin/bash
# Send one key to a gamescope X display and record exactly what was sent.
# argv: <display-number> <xdotool key spec> [note]
# Round 2: round 1 pressed Resume with no recorded command.
export XAUTHORITY=/home/pk/.Xauthority
d=$1; k=$2; shift 2
echo "  \$ DISPLAY=:$d xdotool key $k        # $*"
echo "    stamp: $(date -Is)"
DISPLAY=:$d xdotool key "$k"
echo "    exit: $?"
