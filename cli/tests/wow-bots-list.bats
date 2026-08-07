#!/usr/bin/env bats
# Batch 5 F1: `dml wow bots list` -- read-only paged browse of the random
# bot population (DB stub; SQL asserted via the query log).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose   # Task 6: DB verbs resolve schema names from the title dir
  use_mysql_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

# Two stub answers: COUNT first, then the row page.
seed_two_bots() {
  printf '2\n' > "$FIXTURE/count.tsv"
  # cols: guid, name, class, race, gender, level, online, zone
  printf '9001\tBotmage\t8\t10\t0\t80\t1\t1637\n9002\tBotwar\t1\t2\t1\t42\t0\t14\n' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/count.tsv $FIXTURE/rows.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
}

@test "bots list returns total + rows and does not drop the last row" {
  seed_two_bots
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.total')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.limit')" = "50" ]
  [ "$(echo "$output" | jq -r '.data.offset')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.bots | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.bots[0].name')" = "Botmage" ]
  [ "$(echo "$output" | jq -r '.data.bots[1].name')" = "Botwar" ]
  [ "$(echo "$output" | jq -r '.data.bots[0].online')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.bots[1].online')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.bots[0].online | type')" = "boolean" ]
  [ "$(echo "$output" | jq -r '.data.bots[1].zone')" = "14" ]
}

# Was "never RNDBOT%": bot identity used to be the playerbots table ALONE.
# That table is populated by mod-playerbots itself and can be EMPTY on a
# freshly built install (measured 2026-08-01: 0 rows next to 1000 bot
# characters), and `account IN (<empty set>)` is FALSE for every row -- so the
# bots page listed nothing while Home listed all 1000 bots as real players.
# Both signals are required now; the registry stays FIRST and authoritative.
@test "bots list identifies bots by the playerbots table OR the reserved account prefix" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  grep -q 'acore_playerbots.playerbots_account_type' "$FIXTURE/q.log"
  grep -q 'account_type IN (1,2)' "$FIXTURE/q.log"
  grep -q "UPPER(username) LIKE 'RNDBOT%'" "$FIXTURE/q.log"
  # OR, never AND: an install with an unpopulated registry must still match.
  grep -q 'account_type IN (1,2)) OR ' "$FIXTURE/q.log"
}

@test "bot identity: a custom prefix is read from playerbots.conf and LIKE-escaped" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  # _wow_server_dir resolves through _resolve_compose_dir, so the title dir
  # only counts once it holds a compose file.
  add_game wow-server-playerbots compose
  mkdir -p "$DML_GAMES_DIR/wow-server-playerbots/env/dist/etc/modules"
  printf 'AiPlayerbot.RandomBotAccountPrefix = "my_bot"\n' \
    > "$DML_GAMES_DIR/wow-server-playerbots/env/dist/etc/modules/playerbots.conf"
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  # `_` is a single-char LIKE wildcard -- unescaped, `my_bot%` would also
  # match `myXbot...` and silently widen the bot set.
  grep -q "UPPER(username) LIKE 'MY\\\\_BOT%'" "$FIXTURE/q.log"
}

@test "bot identity: an EMPTY prefix falls back, it never becomes LIKE '%'" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  # A BLANK CONF VALUE is the path that actually reaches the guard. Setting
  # only DML_BOT_ACCOUNT_PREFIX="" would prove nothing here: empty is treated
  # as unset, so with no conf on disk the default arrives by a second route
  # and the test stays green even with the guard deleted.
  add_game wow-server-playerbots compose
  mkdir -p "$DML_GAMES_DIR/wow-server-playerbots/env/dist/etc/modules"
  printf 'AiPlayerbot.RandomBotAccountPrefix = ""\n' \
    > "$DML_GAMES_DIR/wow-server-playerbots/env/dist/etc/modules/playerbots.conf"
  export DML_BOT_ACCOUNT_PREFIX=""
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  # The mirror-image failure: a match-all prefix would call every real
  # player's account a bot.
  [ "$(grep -c "LIKE '%'" "$FIXTURE/q.log")" = "0" ]
  grep -q "UPPER(username) LIKE 'RNDBOT%'" "$FIXTURE/q.log"
}

@test "bots list builds each filter into the SQL exactly" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --name Bot --class 8 --min-level 10 --max-level 80 --online --json
  [ "$status" -eq 0 ]
  grep -q "c.name LIKE 'Bot%'" "$FIXTURE/q.log"
  grep -q 'c.class = 8' "$FIXTURE/q.log"
  grep -q 'c.level BETWEEN 10 AND 80' "$FIXTURE/q.log"
  grep -q 'c.online = 1' "$FIXTURE/q.log"
}

@test "bots list omits absent filters from the SQL" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  ! grep -q 'LIKE' "$FIXTURE/q.log"
  ! grep -q 'c.class =' "$FIXTURE/q.log"
  ! grep -q 'c.level' "$FIXTURE/q.log"
  ! grep -q 'c.online = 1' "$FIXTURE/q.log"
}

@test "bots list min-level alone becomes >= and max-level alone <=" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --min-level 30 --json
  [ "$status" -eq 0 ]
  grep -q 'c.level >= 30' "$FIXTURE/q.log"
  rm -f "$FIXTURE/q.log" "$FIXTURE/seq.state"
  run bash "$DML" wow bots list --max-level 60 --json
  [ "$status" -eq 0 ]
  grep -q 'c.level <= 60' "$FIXTURE/q.log"
}

@test "bots list pagination flags reach the SQL and the envelope" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --limit 25 --offset 50 --json
  [ "$status" -eq 0 ]
  grep -q 'LIMIT 25 OFFSET 50' "$FIXTURE/q.log"
  [ "$(echo "$output" | jq -r '.data.limit')" = "25" ]
  [ "$(echo "$output" | jq -r '.data.offset')" = "50" ]
}

@test "bots list caps --limit at 200" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --limit 9999 --json
  [ "$status" -eq 0 ]
  grep -q 'LIMIT 200 OFFSET 0' "$FIXTURE/q.log"
  [ "$(echo "$output" | jq -r '.data.limit')" = "200" ]
}

@test "bots list: empty --limit/--offset fall back to defaults, not a fatal crash" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --limit "" --offset "" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.limit')" = "50" ]
  [ "$(echo "$output" | jq -r '.data.offset')" = "0" ]
  grep -q 'LIMIT 50 OFFSET 0' "$FIXTURE/q.log"
}

@test "bots list: an underscore in --name is escaped so it is not a LIKE wildcard" {
  seed_two_bots
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --name Bot_x --json
  [ "$status" -eq 0 ]
  # '_' is escaped with the ESCAPE marker; the trailing % stays a wildcard.
  grep -q "c.name LIKE 'Bot!_x%' ESCAPE '!'" "$FIXTURE/q.log"
  # the raw, unescaped pattern must NOT appear
  ! grep -q "c.name LIKE 'Bot_x%'" "$FIXTURE/q.log"
}

@test "bots list rejects a name prefix outside the charname allowlist" {
  run bash "$DML" wow bots list --name "x%' OR 1=1--" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow bots list --name 'a b' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "bots list rejects a class id outside 1-9,11" {
  run bash "$DML" wow bots list --class 10 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow bots list --class "8 OR 1=1" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "bots list rejects non-numeric level/limit/offset values" {
  for flag in --min-level --max-level --limit --offset; do
    run bash "$DML" wow bots list "$flag" "5; DROP" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "bots list maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow bots list --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "bots list with an empty page returns an empty array, not an error" {
  printf '0\n' > "$FIXTURE/count.tsv"
  printf '' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/count.tsv $FIXTURE/rows.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.bots | length')" = "0" ]
}
