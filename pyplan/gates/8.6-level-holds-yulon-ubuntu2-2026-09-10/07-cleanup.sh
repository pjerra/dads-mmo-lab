#!/usr/bin/env bash
# T16 live half, step 07 -- this lane's own directories off the box.
#
# `~/t16-live` is the copy of the worktree every press ran from and `~/t16-out`
# the logs, which are in the repo folder by the time this runs. Nothing under
# `~/wowserver` is touched here: step 05 already took the one file this lane put
# there back off.
set -u
. "$(dirname "$0")/lib.sh"

say "step 07 -- removing this lane's own directories from the box. Nothing under ~/wowserver is touched."

hdr "T16 step 07 -- cleanup"

sect "what is about to go"
du -sh "$HOME/t16-live" "$HOME/t16-out" 2>/dev/null
ls -1 "$HOME/t16-out" | head -20

sect "the server folder, untouched by this step"
ls -1 "$LUA"
docker ps --format '{{.Names}}\t{{.Status}}'

echo "removing $HOME/t16-live and $HOME/t16-out at $(stamp)"
say "step 07 finished -- this lane's directories are gone."
echo "step 07 finished $(stamp)"
