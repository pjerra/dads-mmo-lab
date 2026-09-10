#!/usr/bin/env bash
# Step 03b -- a correction to one sentence `03-stage.log` prints.
#
# That log ends route 2 with "the module's addclass will not serve one [a bot
# master]". The run measured only that `dml_addclass Jurnaar mage` ran and made
# no character and no group. The CAUSE was not measured, and there is a second
# explanation with at least as good a claim: every bot account on this install
# already holds ten characters, which is the realm's per-account cap, so
# `addclass` would have nothing to create the alt in either way. This step
# captures that count so the README can say what is known and what is not.
source "$(dirname "$0")/lib.sh"
say "step 03b, measuring how many characters a bot account holds, to bound the claim step 03 made about why dml_addclass created nothing."
{
hdr "T13 live, step 03b: what is actually known about route 2"

sect "characters per account, for the accounts this run's names live on"
$PRESS sql "SELECT a.username, COUNT(*) AS characters FROM acore_characters.characters c JOIN acore_auth.account a ON a.id=c.account GROUP BY a.username ORDER BY characters DESC, a.username LIMIT 6;"

sect "the realm's per-account character cap, as the world's own conf has it"
grep -n '^CharactersPerRealm\|^CharactersPerAccount' "$HOME/wowserver/env/dist/etc/worldserver.conf" || \
  echo "(not set in worldserver.conf; the core default is 10 per realm)"

sect "done"
} 2>&1 | tee "$OUT/03b-addclass-cause.log"
say "step 03b done."
