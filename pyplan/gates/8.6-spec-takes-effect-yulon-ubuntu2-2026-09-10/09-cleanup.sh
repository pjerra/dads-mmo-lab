#!/usr/bin/env bash
# Step 09 -- this lane's own directories off the box. Run AFTER the logs have
# been copied into the repo folder. Nothing under `~/wowserver` is touched
# here: what this lane left there is named in `07-teardown.sh` and read back in
# `08-box-as-found.log`.
set -u
OUT="$HOME/t18-out"
say() { "$HOME/claude-say" "T18 live: $*" >/dev/null; }
stamp() { date +"%H:%M:%S %Z"; }

say "step 09, removing this lane's own directories from the box (~/t18-live, ~/t18-out). Nothing under ~/wowserver is touched."
{
echo "=============================================================="
echo "T18 live, step 09: this lane's directories off the box -- $(stamp) -- $(hostname)"
echo "=============================================================="
echo
echo "---- what is there now -- $(stamp) ----"
du -sh "$HOME/t18-live" "$OUT" 2>&1
echo
echo "---- removing -- $(stamp) ----"
rm -rf "$HOME/t18-live"
echo "removed ~/t18-live"
echo
echo "---- what is left of this lane -- $(stamp) ----"
ls -d "$HOME"/t18* 2>&1
echo "(only ~/t18-out, which holds this log, should be named above)"
echo
echo "---- the harness is not on the server -- $(stamp) ----"
ls "$HOME/wowserver/env/dist/etc/modules/lua_scripts/" | grep -c t18 || true
echo "(0 = gone)"
echo
echo "---- done -- $(stamp) ----"
} 2>&1 | tee "$OUT/09-cleanup.log"
say "step 09 done -- this lane's tree is off the box."
