#!/usr/bin/env bash
# Step 03 -- the activation, pressed the Modules tab's own way first.
#
# The tab has two custom-module routes and this box can only be offered the
# folder one: `mod-playerbots` is not a shipped manifest (there is no
# `manifests/wow-wotlk/modules/mod-playerbots.json`), it came in with the
# catalog entry's own emulator sources (`catalog.json:19-24`, dest
# `modules/mod-playerbots`), so the tab's shipped list has no row for it. What
# it does have is "add a module from a folder", and the folder is already on
# disk.
#
# Both presses go through `wotlk_modules.install_custom(applier)` -- the tab's
# ONE custom-install seam -- over the applier `controller_view._for_wotlk`
# builds, with the T7 guard's `world_running` seam attached. The world is left
# RUNNING on purpose: the guard refuses only `applied_by="direct"` SQL into a
# world-held database (`apply.py:_refuse_live_world`), a folder-derived
# manifest's SQL is `db-import` and is reported rather than run, so the guard
# has nothing to refuse and says so by not saying anything.
source "$(dirname "$0")/lib.sh"

say "step 03, pressing the module conf activation. First the Modules tab's own folder route, then -- if it refuses -- the same seam without the copy. The world stays UP; the T7 guard refuses only direct SQL and this manifest has none."
{
hdr "T18 live, step 03: the activation through the app"

sect "the world is up while this is pressed, and that is the guard's own answer"
$PRESS liveness

sect "before: is there a deployed conf at all?"
ls -l "$HOME/wowserver/env/dist/etc/modules/playerbots.conf" 2>&1
$PRESS conf

sect "PRESS 1 -- the Modules tab's folder route, verbatim"
echo "(derive_folder(<the clone>) then install_custom(manifest, <that folder>),"
echo " which is what the view does with the folder the user chose)"
$PRESS activate-folder
echo "exit=$?"

sect "PRESS 2 -- the same seam, with no folder to copy"
echo "(install_custom(manifest, None): the bytes are already at modules/mod-playerbots,"
echo " so there is nothing to copy in and the pass is clone -> complete -> conf)"
$PRESS activate
echo "exit=$?"

sect "what landed on disk"
ls -l "$HOME/wowserver/env/dist/etc/modules/"
echo "-- byte-identical to the module's own template? --"
md5sum "$HOME/wowserver/modules/mod-playerbots/conf/playerbots.conf.dist" \
       "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"
echo "-- and the spec keys are in the deployed file, at their own line numbers --"
grep -c '^AiPlayerbot.PremadeSpecName' "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"
grep -n '^AiPlayerbot.PremadeSpecName.8.[0-9] ' "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

sect "what the container sees at the path sConfigMgr reads"
docker exec ac-worldserver ls -l /azerothcore/env/dist/etc/modules/playerbots.conf
docker exec ac-worldserver md5sum /azerothcore/env/dist/etc/modules/playerbots.conf

sect "the app's own reading of the conf now -- still the RUNNING world's old config"
$PRESS conf

sect "and what the app's picker offers now, before the restart"
$PRESS specs

sect "the manifest the app derived and persisted"
find "$HOME/.config" "$HOME/.local/share" -name 'mod-playerbots.json' 2>/dev/null
find "$HOME/.config" "$HOME/.local/share" -path '*manifests/user*' -name '*.json' 2>/dev/null | head

sect "done"
} 2>&1 | tee "$OUT/03-activate.log"
say "step 03 done -- the module conf activation has been pressed."
