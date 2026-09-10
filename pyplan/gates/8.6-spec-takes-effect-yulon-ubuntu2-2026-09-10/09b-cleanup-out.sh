#!/usr/bin/env bash
# Step 09b -- the last directory, removed after its own logs were copied off.
#
# A separate script from `09-cleanup.sh` for one reason: that script's log is
# written INTO `~/t18-out`, so it cannot record that directory's removal. This
# one is piped to the box over ssh (the lane's tree is already gone) and its
# output is `09b-cleanup-out.txt`.
set -u
"$HOME/claude-say" "T18 live: step 09b, removing ~/t18-out now that its logs are in the repo folder. Nothing under ~/wowserver is touched." >/dev/null
echo "T18 live, step 09b: ~/t18-out off the box -- $(date +'%H:%M:%S %Z') -- $(hostname)"
echo
echo "-- before --"
du -sh "$HOME/t18-out" 2>&1
rm -rf "$HOME/t18-out"
echo "-- after (nothing of this lane should be named) --"
ls -d "$HOME"/t18* 2>&1
echo
echo "-- the box still has the app's own artefacts, and only those --"
ls -l "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"
ls "$HOME/wowserver/env/dist/etc/modules/lua_scripts/" | sort
docker ps --format '{{.Names}}  {{.Status}}'
echo
echo "-- done -- $(date +'%H:%M:%S %Z') --"
