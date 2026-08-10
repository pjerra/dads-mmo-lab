#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose   # Task 6: DB verbs resolve schema names from the title dir
  use_mysql_stub
}
teardown() { teardown_fixture; }

# Rows are: account_id, username, gm_level, guid, char_name, level (TSV;
# LEFT JOIN misses coalesced to empty strings by the SQL). gm_level is
# exercised separately in wow-account.bats; these rows just carry 0 so the
# existing grouping/character assertions below are unaffected.
@test "accounts groups characters under their account" {
  printf '251\tHYPEER\t0\t2502\tHypeer\t100\n253\tTEST1\t0\t2503\tTesten\t1\n253\tTEST1\t0\t2504\tAltchar\t5\n' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.accounts | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].username')" = "HYPEER" ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].id')" = "251" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].id')" = "253" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters[1].name')" = "Altchar" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters[0].level')" = "1" ]
}

@test "accounts keeps a character-less account with empty characters array" {
  printf '254\tDMLSOAP\t0\t\t\t\n' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].username')" = "DMLSOAP" ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].characters | length')" = "0" ]
}

@test "accounts survives the trailing-newline-stripped last row" {
  # printf without trailing \n = what command substitution feeds the parser
  printf '251\tHYPEER\t0\t2502\tHypeer\t100' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.accounts | length')" = "1" ]
}

@test "accounts maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow accounts --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "accounts SQL filters bot accounts" {
  # The stub answers any query, so assert on the QUERY text the arm builds:
  # the mysql stub records its -e argument to DML_STUB_DB_QUERY_LOG when set.
  printf '' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  grep -q "NOT UPPER(a.username) LIKE 'RNDBOT%'" "$FIXTURE/query.log"
  grep -q "<> 'AHBOT'" "$FIXTURE/query.log"
}

@test "accounts SQL honours a customised bot account prefix" {
  # Was hardcoded 'RNDBOT%': a server that changed
  # AiPlayerbot.RandomBotAccountPrefix got all of its bot accounts in the
  # launcher's character picker.
  printf '' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  export DML_BOT_ACCOUNT_PREFIX="fakebot"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  grep -q "NOT UPPER(a.username) LIKE 'FAKEBOT%'" "$FIXTURE/query.log"
  ! grep -q 'RNDBOT' "$FIXTURE/query.log"
}

@test "accounts SQL adds the registry exclusion when a playerbots schema resolves" {
  # mod-city-bots' citybot* citizens carry registry type 3 and no rndbot
  # prefix, so prefix-only listed all 400 as family (2026-08-10: 403 rows in
  # the picker, 3 real). The default fixture resolves acore_playerbots via the
  # seeded playerbots.conf.dist, so the exclusion must appear with that name.
  printf '' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  grep -q 'a.id NOT IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2,3))' "$FIXTURE/query.log"
}

@test "accounts SQL degrades to prefix-only on a box with no playerbots schema" {
  # The picker must not REQUIRE the playerbots schema: with nothing resolving
  # one, the registry subselect (which would error there) stays out entirely.
  rm -f "$DML_GAMES_DIR/wow-server-playerbots/env/dist/etc/modules/playerbots.conf.dist"
  printf '' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  grep -q "NOT UPPER(a.username) LIKE 'RNDBOT%'" "$FIXTURE/query.log"
  ! grep -q 'playerbots_account_type' "$FIXTURE/query.log"
}

@test "accounts SQL orders by a.id (grouping depends on contiguous rows)" {
  # _accounts_rows_to_json (30-db.sh) groups characters under an account by
  # watching for the account id to change between consecutive rows -- that
  # only works if same-account rows are contiguous, which depends on the
  # query's ORDER BY starting with a.id. The stub ignores query text and
  # returns canned rows regardless, so this pins the QUERY STRING itself
  # (same DML_STUB_DB_QUERY_LOG seam as the bot-account-filter test above).
  printf '' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  grep -q "ORDER BY a.id" "$FIXTURE/query.log"
}
