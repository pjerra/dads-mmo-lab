#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
}
teardown() { teardown_fixture; }

@test "characters lists an account's chars with gold in gold-units" {
  export DML_STUB_ACCOUNT_ID=1
  printf '4\tPriesttest\t80\t5\t1\t0\t123456\n' > "$FIXTURE/chars.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/chars.tsv"
  run bash "$DML" wow characters --account admin --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.characters[0].name')" = "Priesttest" ]
  [ "$(echo "$output" | jq -r '.data.characters[0].level')" = "80" ]
  [ "$(echo "$output" | jq -r '.data.characters[0].gold')" = "12" ]
}

# NOTE (deviation from the brief, disclosed): the brief's fixture row for this
# test packed 10 tab-separated fields ('...Hearthstone\t1\t1\t6418') but the
# implementation's SELECT (and its matching `read -r nm lvl cls money slot
# entry iname q disp`) only has 9 columns: name,level,class,money,slot,entry,
# item_name,quality,displayid. With one extra field, bash `read` folds the two
# trailing values into the last variable ($disp) WITH their separating tab
# preserved literally ("1\t6418"), which then lands unescaped and unquoted in
# the emitted JSON (`"displayid":1<TAB>6418}`) -- invalid JSON that breaks jq
# for the whole object, including the earlier `.data.note` field. The brief
# itself flags this as a possible mismatch ("adjust the fixture TSV column
# order to match the SELECT ... Update the test fixture in Step 1 if the
# column order differs"). Fixed here by dropping the duplicate quality field
# so the fixture has exactly 9 columns.
@test "paperdoll returns equipped items with note last_saved" {
  printf 'Priesttest\t80\t5\t123456\t0\t6948\tHearthstone\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/pd.tsv"
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.note')" = "last_saved" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].name')" = "Hearthstone" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].displayid')" = "6418" ]
  [ "$(echo "$output" | jq -r '.data.gold')" = "12" ]
}

@test "paperdoll rejects a bad char name" {
  run bash "$DML" wow paperdoll --char 'no good' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

# --- Extra coverage beyond the brief -----------------------------------
# Error-path coverage mirroring the convention established by
# wow-items.bats / wow-teleport.bats: DB_UNREACHABLE on a failing query, and
# NOT_FOUND for an unresolvable account / character.

@test "characters requires --account" {
  run bash "$DML" wow characters --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "characters maps db failure to DB_UNREACHABLE" {
  export DML_STUB_ACCOUNT_ID=1
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow characters --account admin --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "characters maps an unknown account to NOT_FOUND" {
  : > "$FIXTURE/empty.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/empty.tsv"
  run bash "$DML" wow characters --account ghost --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "paperdoll maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "paperdoll maps an unknown character to NOT_FOUND" {
  : > "$FIXTURE/empty.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/empty.tsv"
  run bash "$DML" wow paperdoll --char Ghosttest --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
