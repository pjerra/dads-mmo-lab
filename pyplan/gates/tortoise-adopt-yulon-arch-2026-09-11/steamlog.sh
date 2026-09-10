#!/bin/bash
L=~/.local/share/Steam/logs/gameprocess_log.txt
echo "# Steam's own record of the two entry launches this ticket made"
echo "stamp: $(date -Is)"
echo
echo "Source: ~/.local/share/Steam/logs/gameprocess_log.txt (Steam's own file, not this run's)."
echo "The 64-bit ids are the ones T17's live run left: client 16370838898999296000,"
echo "server 17621032419199025152. Their 32-bit AppIds appear in the argv Steam records."
echo
echo "## Half 1 -- the client entry, launched from Big Picture at 22:45 (the run that reached"
echo "## the login screen and authenticated; the 22:25 launch is the one the four Escapes closed)"
grep -nE "^\[2026-09-10 22:(2[0-9]|4[0-9]|5[0-9])" "$L" | grep "16370838898999296000" | grep "tracked process \"" | sed 's/^/  > /'
echo
echo "## Half 2 -- the server entry, launched from the gamepad UI INSIDE gamescope at 23:03"
grep -nE "^\[2026-09-10 23:0[0-9]" "$L" | grep "17621032419199025152" | grep "tracked process \"" | sed 's/^/  > /'
echo
echo "## the same window, first and last line for each launch (no argv, just the bracket)"
grep -nE "^\[2026-09-10 22:4[5-9]" "$L" | grep "16370838898999296000" | head -2 | sed 's/^/  > /'
grep -nE "^\[2026-09-10 2[23]:" "$L" | grep "16370838898999296000" | grep "no longer tracking" | tail -2 | sed 's/^/  > /'
grep -nE "^\[2026-09-10 23:0[0-9]" "$L" | grep "17621032419199025152" | head -2 | sed 's/^/  > /'
grep -nE "^\[2026-09-10 23:0[0-9]" "$L" | grep "17621032419199025152" | grep "no longer tracking" | tail -2 | sed 's/^/  > /'
