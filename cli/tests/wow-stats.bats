#!/usr/bin/env bats
# `wow stats`: the Statistics page's single read-only envelope (48-stats.sh).
# The query order is FIXED (see the 48-stats.sh header) -- the happy path
# stubs all 19 mysql calls positionally via DML_STUB_DB_ROWS_SEQ (call 1 is
# the playerbots-schema probe).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
}
teardown() { teardown_fixture; }

# One fixture file per query, in the documented order.
make_stats_rows() {
  printf '1\n'                              > "$FIXTURE/q00"  # playerbots-schema probe
  printf '4\t1\t250\t120\t900000000\n'      > "$FIXTURE/q01"  # fam tot/on, bot tot/on, bot playtime
  printf '0\t1\t50\n7\t2\t30\n'             > "$FIXTURE/q02"  # level buckets
  printf '1\t3\t37\n8\t0\t30\n'             > "$FIXTURE/q03"  # classes: class, fam, bot
  printf '3\t1\t117\t133\n'                 > "$FIXTURE/q04"  # factions: famA famH botA botH
  printf 'Milla\t80\n'                      > "$FIXTURE/q05"  # top levels FAMILY
  printf 'Rndbot\t80\n'                     > "$FIXTURE/q06"  # top levels BOTS
  printf '3\t27\n'                          > "$FIXTURE/q07"  # guilds, members
  printf '1211290000\t90000\t1211200000\n'  > "$FIXTURE/q08"  # copper totals
  printf 'Milla\t90000\n'                   > "$FIXTURE/q09"  # richest FAMILY
  printf 'Goldy\t1211290000\n'              > "$FIXTURE/q10"  # richest BOTS
  printf '412\t998877\n'                    > "$FIXTURE/q11"  # auction
  printf '9\t2\n'                           > "$FIXTURE/q12"  # mail
  printf 'Milla\t80\t1\t63720\t1750000000\t1\t12\t345\t678\n' > "$FIXTURE/q13"  # journey (online=1)
  printf '57\t400000\t90000\t260\n'         > "$FIXTURE/q14"  # uptime aggregates
  printf 'DML\n'                            > "$FIXTURE/q15"  # realm name
  printf '1750000000\t3600\n1750100000\t7200\n' > "$FIXTURE/q16"  # recent boots
  printf '1637\t18\n17\t11\n'               > "$FIXTURE/q17"  # bot zones
  printf '0\t60\n571\t10\n-1\t3\n'          > "$FIXTURE/q18"  # continents (+ other-worlds bucket)
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/q00 $FIXTURE/q01 $FIXTURE/q02 $FIXTURE/q03 $FIXTURE/q04 $FIXTURE/q05 $FIXTURE/q06 $FIXTURE/q07 $FIXTURE/q08 $FIXTURE/q09 $FIXTURE/q10 $FIXTURE/q11 $FIXTURE/q12 $FIXTURE/q13 $FIXTURE/q14 $FIXTURE/q15 $FIXTURE/q16 $FIXTURE/q17 $FIXTURE/q18"
}

@test "stats happy path assembles all five groups from the 19 queries" {
  make_stats_rows
  run bash "$DML" wow stats --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  # population
  [ "$(echo "$output" | jq -r '.data.population.family.total')" = "4" ]
  [ "$(echo "$output" | jq -r '.data.population.family.online')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.population.bots.total')" = "250" ]
  [ "$(echo "$output" | jq -r '.data.population.bots.online')" = "120" ]
  [ "$(echo "$output" | jq -r '.data.population.levels | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.population.levels[1].bucket')" = "7" ]
  [ "$(echo "$output" | jq -r '.data.population.levels[1].family')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.population.levels[1].bots')" = "30" ]
  # classes: segmented; zero-count rows dropped per segment (class 8 has no
  # family chars -> only in bots)
  [ "$(echo "$output" | jq -r '.data.population.classes.family | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.population.classes.family[0].class')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.population.classes.family[0].count')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.population.classes.bots | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.population.classes.bots[1].class')" = "8" ]
  [ "$(echo "$output" | jq -r '.data.population.classes.bots[1].count')" = "30" ]
  # factions: segmented
  [ "$(echo "$output" | jq -r '.data.population.factions.family.alliance')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.population.factions.family.horde')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.population.factions.bots.alliance')" = "117" ]
  [ "$(echo "$output" | jq -r '.data.population.factions.bots.horde')" = "133" ]
  # top levels: segmented, family flag fixed per segment
  [ "$(echo "$output" | jq -r '.data.population.top_levels.family[0].name')" = "Milla" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels.family[0].family')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels.bots[0].name')" = "Rndbot" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels.bots[0].family')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.population.guilds.count')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.population.guilds.members')" = "27" ]
  # economy
  [ "$(echo "$output" | jq -r '.data.economy.copper.total')" = "1211290000" ]
  [ "$(echo "$output" | jq -r '.data.economy.copper.family')" = "90000" ]
  [ "$(echo "$output" | jq -r '.data.economy.copper.bots')" = "1211200000" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest.family[0].name')" = "Milla" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest.family[0].family')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest.bots[0].name')" = "Goldy" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest.bots[0].copper')" = "1211290000" ]
  [ "$(echo "$output" | jq -r '.data.economy.auction.count')" = "412" ]
  [ "$(echo "$output" | jq -r '.data.economy.auction.buyout')" = "998877" ]
  [ "$(echo "$output" | jq -r '.data.economy.mail.total')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.economy.mail.to_family')" = "2" ]
  # journey
  [ "$(echo "$output" | jq -r '.data.journey | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].name')" = "Milla" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].playtime')" = "63720" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].last_seen')" = "1750000000" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].online')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].online | type')" = "boolean" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].kills')" = "12" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].achievements')" = "345" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].quests')" = "678" ]
  # history
  [ "$(echo "$output" | jq -r '.data.history.boots')" = "57" ]
  [ "$(echo "$output" | jq -r '.data.history.total_uptime')" = "400000" ]
  [ "$(echo "$output" | jq -r '.data.history.longest')" = "90000" ]
  [ "$(echo "$output" | jq -r '.data.history.peak')" = "260" ]
  [ "$(echo "$output" | jq -r '.data.history.realm')" = "DML" ]
  [ "$(echo "$output" | jq -r '.data.history.recent | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.history.recent[0].start')" = "1750000000" ]
  [ "$(echo "$output" | jq -r '.data.history.recent[0].uptime')" = "3600" ]
  # bot watch
  [ "$(echo "$output" | jq -r '.data.botwatch.zones | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.zones[0].zone')" = "1637" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.zones[0].count')" = "18" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.continents | length')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.continents[1].map')" = "571" ]
  # 8d: the "other worlds" bucket arrives as map -1 and stays a number
  [ "$(echo "$output" | jq -r '.data.botwatch.continents[2].map')" = "-1" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.continents[2].count')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.playtime')" = "900000000" ]
}

@test "stats: an empty database answers with zeros and empty arrays, not an error" {
  # No DML_STUB_DB_ROWS* at all: every query returns nothing with exit 0.
  run bash "$DML" wow stats --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.population.family.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.population.bots.online')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.population.levels | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels.family | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels.bots | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.population.classes.family | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.economy.copper.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest.family | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest.bots | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.journey | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.history.boots')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.history.realm')" = "" ]
  [ "$(echo "$output" | jq -r '.data.history.recent | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.zones | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.playtime')" = "0" ]
}

@test "stats: NULL/garbage numerics degrade to 0 and never break the JSON" {
  # The SAME garbage row answers every query (mysql -N prints NULL literally).
  printf 'NULL\tNULL\tNULL\tNULL\tNULL\n' > "$FIXTURE/garbage"
  export DML_STUB_DB_ROWS="$FIXTURE/garbage"
  run bash "$DML" wow stats --json
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.' >/dev/null       # whole envelope stays valid JSON
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.population.family.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.economy.copper.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.history.peak')" = "0" ]
  # Row-keyed arrays DROP rows whose key column is garbage (levels/zones/
  # continents key on bucket/zone/map)...
  [ "$(echo "$output" | jq -r '.data.population.levels | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.zones | length')" = "0" ]
  # ...while name-keyed rows keep the row and zero the numerics. The family
  # flag is fixed per segment now (true in the family list, false in bots).
  [ "$(echo "$output" | jq -r '.data.population.top_levels.family[0].level')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels.family[0].family')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels.bots[0].family')" = "false" ]
}

@test "stats: leading zeros are normalized (007 -> 7), not emitted as invalid JSON" {
  printf '007\t008\t009\t010\t011\n' > "$FIXTURE/zeros"
  export DML_STUB_DB_ROWS="$FIXTURE/zeros"
  run bash "$DML" wow stats --json
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.' >/dev/null
  [ "$(echo "$output" | jq -r '.data.population.family.total')" = "7" ]
  [ "$(echo "$output" | jq -r '.data.population.family.online')" = "8" ]
  [ "$(echo "$output" | jq -r '.data.population.bots.total')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.population.bots.online')" = "10" ]
  [ "$(echo "$output" | jq -r '.data.botwatch.playtime')" = "11" ]
}

@test "stats maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow stats --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "stats SQL uses the bot idiom and excludes the system accounts" {
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow stats --json
  [ "$status" -eq 0 ]
  grep -q 'playerbots_account_type' "$FIXTURE/q.log"
  grep -q "username IN ('AHBOT','DMLSOAP')" "$FIXTURE/q.log"
  grep -q 'online = 1' "$FIXTURE/q.log"
  # 8a: the level-bucket floor is clamped so a level-0 row can't wrap the
  # unsigned subtraction into a strict-mode query error.
  grep -q 'GREATEST(c.level,1)' "$FIXTURE/q.log"
  # 8d: continents are bounded to the four playable maps + the -1 bucket.
  grep -q 'CASE WHEN c.map IN (0,1,530,571)' "$FIXTURE/q.log"
}

@test "stats: a missing playerbots schema degrades bot stats to zero instead of failing (8c)" {
  # Call 1 (the probe) fails; every later query succeeds. The envelope must
  # come back ok, and no post-probe query may reference the missing schema.
  make_stats_rows
  export DML_STUB_DB_EXIT_SEQ="1 0"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow stats --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  # Only the probe itself mentions the playerbots schema...
  [ "$(grep -c 'playerbots_account_type' "$FIXTURE/q.log")" = "1" ]
  # ...and the substituted FALSE reaches the real queries.
  grep -q '0=1' "$FIXTURE/q.log"
}

@test "stats: a failed query on a REACHABLE database gets the honest hint (8c)" {
  # Probe ok, query 2 fails, and the caller's SELECT 1 recheck succeeds ->
  # the hint must say the DB is reachable, not "is ac-database running?".
  printf '1\n' > "$FIXTURE/ok.tsv"; : > "$FIXTURE/none.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/ok.tsv $FIXTURE/none.tsv $FIXTURE/ok.tsv"
  export DML_STUB_DB_EXIT_SEQ="0 1 0"
  run bash "$DML" wow stats --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
  echo "$output" | grep -q 'reachable but a query failed'
}

@test "stats rejects unknown flags with BAD_ARG" {
  run bash "$DML" wow stats --bogus --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
