#!/usr/bin/env bash
# Step 04 -- is `bot add <name>` reachable over the app's own SOAP seam at all?
# Every press is preceded by an EMPTY `docker logs --since` window, stamped with
# a trailing Z (memory note `docker-logs-since-is-client-local`).
source "$HOME/t26-out/lib.sh"

# press <pattern> <command...>  -- open a window, prove it empty, press, read it.
press() {
    local pat="$1"; shift
    local since; since=$(utc)
    echo
    echo ">>> window opens at SINCE=$since  (grep pattern: $pat)"
    local n; n=$(docker logs ac-worldserver --since "$since" 2>&1 | grep -cE "$pat" || true)
    echo ">>> lines matching in the window BEFORE the press: $n"
    $PRESS send "$@"
    sleep 3
    echo ">>> the world's own log for that window:"
    docker logs ac-worldserver --since "$since" 2>&1 | grep -E "$pat" || echo "(no matching line)"
}

say "step 04, asking the SOAP seam for the playerbots bot-add command in every spelling. Read-only presses; nothing is added to any party."
{
hdr "T26 measure, step 04: what the SOAP seam can and cannot reach"

sect "who is asking, and its rights"
$PRESS who
qt "SELECT id,gmlevel,RealmID FROM acore_auth.account_access ORDER BY id;"
echo "(SEC_ADMINISTRATOR = 3; AzerothCore's SOAP refuses anything below it)"

sect "the seam is alive"
press "dml_bridge_ping" dml_bridge_ping

sect "1. what the console/SOAP caller is even OFFERED under .playerbots"
press "NOTHING-EXPECTED" ".playerbots"
press "NOTHING-EXPECTED" ".help playerbots"

sect "2. the command the ticket names, in every spelling -- case (a) SAME ACCOUNT as the master"
echo "master Jurnaar (guid 1, account 1); Nore (guid 2) is on the SAME account"
press "NOTHING-EXPECTED" ".playerbots bot add Nore"
press "NOTHING-EXPECTED" "playerbots bot add Nore"
press "NOTHING-EXPECTED" ".bot add Nore"
press "NOTHING-EXPECTED" "bot add Nore"

sect "3. case (b) a character on a DIFFERENT account, not in the master's guild"
echo "master Jurnaar (account 1, no guild); Grirmirn (guid 11) is on account 2 and in no guild"
press "NOTHING-EXPECTED" ".playerbots bot add Grirmirn"

sect "4. case (c) a GUILD MATE on another account"
echo "master Dalio (guid 4, account 1, guild 12 'Violet Dawn'); Jordanik (guid 24, account 3) is in the same guild"
press "NOTHING-EXPECTED" ".playerbots bot add Jordanik"

sect "5. the sibling subcommands the same table declares Console::No"
press "NOTHING-EXPECTED" ".playerbots bot list"
press "NOTHING-EXPECTED" ".playerbots account linkedAccounts"

sect "6. the ONE playerbots subcommand that IS Console::Yes, to prove the prefix works"
press "NOTHING-EXPECTED" ".playerbots rndbot stats"

sect "7. and the app's own relay, which is what the panel actually uses"
press "dml_addclass|dml_login|dml_uninvite" "dml_bridge_ping"

sect "the group table is untouched by all of the above"
q "SELECT COUNT(*) FROM acore_characters.group_member;"
q "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;"

sect "done"
} 2>&1 | tee "$OUT/04-soap.log"
say "step 04 done."
