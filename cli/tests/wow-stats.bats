#!/usr/bin/env bats
# `wow stats`: the Statistics page's single read-only envelope (48-stats.sh).
# The query order is FIXED (see the 48-stats.sh header) -- the happy path
# stubs all 16 mysql calls positionally via DML_STUB_DB_ROWS_SEQ.
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
  printf '4\t1\t250\t120\t900000000\n'      > "$FIXTURE/q01"  # fam tot/on, bot tot/on, bot playtime
  printf '0\t1\t50\n7\t2\t30\n'             > "$FIXTURE/q02"  # level buckets
  printf '1\t40\n8\t30\n'                   > "$FIXTURE/q03"  # classes
  printf '120\t134\n'                       > "$FIXTURE/q04"  # factions
  printf 'Milla\t80\t1\nRndbot\t80\t0\n'    > "$FIXTURE/q05"  # top levels
  printf '3\t27\n'                          > "$FIXTURE/q06"  # guilds, members
  printf '1211290000\t90000\t1211200000\n'  > "$FIXTURE/q07"  # copper totals
  printf 'Goldy\t1211290000\t1\n'           > "$FIXTURE/q08"  # richest
  printf '412\t998877\n'                    > "$FIXTURE/q09"  # auction
  printf '9\t2\n'                           > "$FIXTURE/q10"  # mail
  printf 'Milla\t80\t1\t63720\t1750000000\t12\t345\t678\n' > "$FIXTURE/q11"  # journey
  printf '57\t400000\t90000\t260\n'         > "$FIXTURE/q12"  # uptime aggregates
  printf 'DML\n'                            > "$FIXTURE/q13"  # realm name
  printf '1750000000\t3600\n1750100000\t7200\n' > "$FIXTURE/q14"  # recent boots
  printf '1637\t18\n17\t11\n'               > "$FIXTURE/q15"  # bot zones
  printf '0\t60\n1\t50\n571\t10\n'          > "$FIXTURE/q16"  # continents
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/q01 $FIXTURE/q02 $FIXTURE/q03 $FIXTURE/q04 $FIXTURE/q05 $FIXTURE/q06 $FIXTURE/q07 $FIXTURE/q08 $FIXTURE/q09 $FIXTURE/q10 $FIXTURE/q11 $FIXTURE/q12 $FIXTURE/q13 $FIXTURE/q14 $FIXTURE/q15 $FIXTURE/q16"
}

@test "stats happy path assembles all five groups from the 16 queries" {
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
  [ "$(echo "$output" | jq -r '.data.population.classes[1].class')" = "8" ]
  [ "$(echo "$output" | jq -r '.data.population.classes[1].count')" = "30" ]
  [ "$(echo "$output" | jq -r '.data.population.factions.alliance')" = "120" ]
  [ "$(echo "$output" | jq -r '.data.population.factions.horde')" = "134" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels[0].name')" = "Milla" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels[0].family')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels[1].family')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.population.guilds.count')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.population.guilds.members')" = "27" ]
  # economy
  [ "$(echo "$output" | jq -r '.data.economy.copper.total')" = "1211290000" ]
  [ "$(echo "$output" | jq -r '.data.economy.copper.family')" = "90000" ]
  [ "$(echo "$output" | jq -r '.data.economy.copper.bots')" = "1211200000" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest[0].name')" = "Goldy" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest[0].copper')" = "1211290000" ]
  [ "$(echo "$output" | jq -r '.data.economy.auction.count')" = "412" ]
  [ "$(echo "$output" | jq -r '.data.economy.auction.buyout')" = "998877" ]
  [ "$(echo "$output" | jq -r '.data.economy.mail.total')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.economy.mail.to_family')" = "2" ]
  # journey
  [ "$(echo "$output" | jq -r '.data.journey | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].name')" = "Milla" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].playtime')" = "63720" ]
  [ "$(echo "$output" | jq -r '.data.journey[0].last_seen')" = "1750000000" ]
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
  [ "$(echo "$output" | jq -r '.data.botwatch.continents[2].map')" = "571" ]
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
  [ "$(echo "$output" | jq -r '.data.population.top_levels | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.economy.copper.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.economy.richest | length')" = "0" ]
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
  # ...while name-keyed rows keep the row and zero the numerics.
  [ "$(echo "$output" | jq -r '.data.population.top_levels[0].level')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.population.top_levels[0].family')" = "false" ]
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
}

@test "stats rejects unknown flags with BAD_ARG" {
  run bash "$DML" wow stats --bogus --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
