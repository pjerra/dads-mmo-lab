#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "party online lists human online chars (rows come pre-filtered by SQL)" {
  # cols: guid, name, class, level
  printf '2503\tTesten\t8\t1\n' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.online[0].name')" = "Testen" ]
  [ "$(echo "$output" | jq -r '.data.online[0].class')" = "8" ]
}

@test "party online SQL excludes bot accounts" {
  printf '' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  grep -q 'playerbots_account_type' "$FIXTURE/q.log"
  grep -q 'online' "$FIXTURE/q.log"
}

@test "party online: a NULL guid/class/level degrades to 0 instead of breaking the JSON" {
  # cols: guid, name, class(NULL), level(NULL) -- mysql -N emits NULLs as the
  # literal "NULL"; unguarded that yields `"class":NULL,` -> invalid JSON that
  # blanks the party card. Same 0 degradation as `players online`.
  printf '2503\tTesten\tNULL\tNULL\n' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.online[0].guid')" = "2503" ]
  [ "$(echo "$output" | jq -r '.data.online[0].class')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.online[0].level')" = "0" ]
}

@test "party online: a non-numeric guid degrades to 0 rather than emitting invalid JSON" {
  printf 'NULL\tTesten\t8\t1\n' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online[0].guid')" = "0" ]
}

@test "party online maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow party online --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "party add rejects a class outside the allowlist" {
  use_curl_stub
  run bash "$DML" wow party add --player Testen --class necromancer --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "party add rejects an offline/unknown player" {
  use_mysql_stub
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party add --player Ghost --class mage --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "party add confirms a join when a new group member appears" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  # Call 1 = online-guard lookup (player guid 2503).
  # Call 2 = pre-fire group snapshot (solo: just the player).
  # Call 3+ = post-fire poll (player + new bot guid 9001).
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/before.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after.tsv"
  # Bot name lookup for guid 9001.
  printf 'Botmage\n' > "$FIXTURE/botname.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/before.tsv $FIXTURE/after.tsv $FIXTURE/botname.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=3 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.added')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.bot')" = "Botmage" ]
}

@test "party add group-diff targets the new bot, not a pre-existing one" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  # Before-set already has bot 8000; only 9001 is new. A dropped-diff
  # refactor could still report joined:true here by latching onto the
  # pre-existing guid, so assert on the QUERY LOG: the name-lookup query
  # (the only query with a literal guid=<n>) must target the new bot 9001,
  # never the pre-existing bot 8000.
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n8000\n' > "$FIXTURE/before.tsv"
  printf '2503\n8000\n9001\n' > "$FIXTURE/after.tsv"
  printf 'Botmage\n' > "$FIXTURE/botname.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/before.tsv $FIXTURE/after.tsv $FIXTURE/botname.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=3 DML_PARTY_POLL_SLEEP=0
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "true" ]
  grep -q 'guid=9001' "$FIXTURE/q.log"
  ! grep -q 'guid=8000' "$FIXTURE/q.log"
}

@test "party add returns joined:false with a note when no member appears in time" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/solo.tsv"   # never grows
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/solo.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=2 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.note')" != "null" ]
}

@test "party add still succeeds (bot:null) when the bot-name lookup fails" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  # Same as the join case, but the name-lookup (call 4) returns empty rows,
  # simulating a transient ac-database blip: the bot has already joined, so
  # the add must still emit ONE success envelope (joined:true) with bot:null.
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/before.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after.tsv"
  printf '' > "$FIXTURE/noname.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/before.tsv $FIXTURE/after.tsv $FIXTURE/noname.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=3 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.bot')" = "null" ]
}

@test "party add fires the correct bridge command over SOAP" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.xml"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"; printf '2503\n' > "$FIXTURE/solo.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/solo.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class druid --gender female --json
  [ "$status" -eq 0 ]
  cap="$(cat "$FIXTURE/cap.xml")"; cmd="${cap#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "dml_addclass Testen druid female" ]
}

@test "party add with no --gender fires the bridge command without a trailing token" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.xml"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"; printf '2503\n' > "$FIXTURE/solo.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/solo.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  cap="$(cat "$FIXTURE/cap.xml")"; cmd="${cap#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "dml_addclass Testen mage" ]
}

@test "party list returns group members with bot flags" {
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  # members rows: guid, name, class, level, is_bot, online (Batch 5: online dot)
  printf '2503\tTesten\t8\t1\t0\t1\n9001\tBotmage\t8\t80\t1\t0\n' > "$FIXTURE/mem.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/mem.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party list --player Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.members | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.members[1].name')" = "Botmage" ]
  [ "$(echo "$output" | jq -r '.data.members[1].is_bot')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.members[0].is_bot')" = "false" ]
  # jq -r prints bare true/false for BOTH a JSON boolean and the string
  # "true"/"false", so assert the TYPE too -- a regression to a quoted is_bot
  # would slip past the value asserts above.
  [ "$(echo "$output" | jq -r '.data.members[1].is_bot | type')" = "boolean" ]
  [ "$(echo "$output" | jq -r '.data.members[0].is_bot | type')" = "boolean" ]
  # Batch 5 F5 (per-bot online dot): online is a real JSON boolean mapped from
  # characters.online (Testen online -> true, the offline bot -> false).
  [ "$(echo "$output" | jq -r '.data.members[0].online')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.members[1].online')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.members[0].online | type')" = "boolean" ]
  [ "$(echo "$output" | jq -r '.data.members[1].online | type')" = "boolean" ]
}

@test "party list of an offline player is NOT_FOUND" {
  use_mysql_stub
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  run bash "$DML" wow party list --player Ghost --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "party kick fires dml_uninvite THEN the master logout whisper, in order" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/cap.txt"
  run bash "$DML" wow party kick --player Testen --bot Botmage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.kicked')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.dismissed')" = "true" ]
  grep -q 'dml_uninvite Botmage' "$FIXTURE/cap.txt"
  grep -q 'dml_whisper Testen Botmage logout' "$FIXTURE/cap.txt"
  # order: uninvite BEFORE the dismissal whisper
  [ "$(grep -n 'dml_uninvite Botmage' "$FIXTURE/cap.txt" | head -1 | cut -d: -f1)" -lt \
    "$(grep -n 'dml_whisper Testen Botmage logout' "$FIXTURE/cap.txt" | head -1 | cut -d: -f1)" ]
}

@test "party kick rejects a bad bot name" {
  use_curl_stub
  run bash "$DML" wow party kick --player Testen --bot 'x y' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "party kick without --player is BAD_ARG (no half-kick without a dismisser)" {
  use_curl_stub
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow party kick --bot Botmage --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/curl.log" ]
}

@test "party dismiss-all uninvites + logout-whispers every stubbed party bot, in order" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/cap.txt"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf 'Botmage\nBotwar\n' > "$FIXTURE/bots.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/bots.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party dismiss-all --player Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.dismissed')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.attempted')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.bots | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.bots[0]')" = "Botmage" ]
  # Per bot: uninvite then logout whisper; bots processed in list order.
  u1="$(grep -n 'dml_uninvite Botmage' "$FIXTURE/cap.txt" | head -1 | cut -d: -f1)"
  w1="$(grep -n 'dml_whisper Testen Botmage logout' "$FIXTURE/cap.txt" | head -1 | cut -d: -f1)"
  u2="$(grep -n 'dml_uninvite Botwar' "$FIXTURE/cap.txt" | head -1 | cut -d: -f1)"
  w2="$(grep -n 'dml_whisper Testen Botwar logout' "$FIXTURE/cap.txt" | head -1 | cut -d: -f1)"
  [ "$u1" -lt "$w1" ]; [ "$w1" -lt "$u2" ]; [ "$u2" -lt "$w2" ]
}

@test "party dismiss-all fails honestly when EVERY SOAP fire fails (no fabricated success)" {
  # SOAP down while the DB is up: the online-guard and member query succeed,
  # every uninvite/whisper fails. The old code exited 0 with dismissed:N --
  # now this is an error envelope mapped like _party_fire (rc 4 -> UNREACHABLE).
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf 'Botmage\nBotwar\n' > "$FIXTURE/bots.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/bots.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party dismiss-all --player Testen --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}

@test "party dismiss-all maps an all-faults run to SOAP_FAULT with the bridge hint" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf 'Botmage\n' > "$FIXTURE/bots.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/bots.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party dismiss-all --player Testen --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
  echo "$output" | grep -q 'bridge-setup'
}

@test "party dismiss-all counts only real dismissals on a partial failure" {
  use_curl_stub
  # SOAP call order: uninvite Botmage (ok), whisper Botmage (ok),
  # uninvite Botwar (FAULT), whisper Botwar (fault too -- still best-effort).
  export DML_STUB_CURL_SEQ="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml $BATS_TEST_DIRNAME/fixtures/soap-ok.xml $BATS_TEST_DIRNAME/fixtures/soap-fault.xml $BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  export DML_STUB_CURL_SEQ_STATE="$FIXTURE/soap.state"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf 'Botmage\nBotwar\n' > "$FIXTURE/bots.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/bots.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party dismiss-all --player Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.dismissed')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.attempted')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.bots | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.bots[0]')" = "Botmage" ]
}

@test "party dismiss-all with an offline player is NOT_FOUND before any SOAP fire" {
  use_mysql_stub
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow party dismiss-all --player Ghost --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  [ ! -f "$FIXTURE/curl.log" ]
}

@test "party dismiss-all with no party bots dismisses zero and still succeeds" {
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/nobots.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/nobots.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party dismiss-all --player Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.dismissed')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.attempted')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.bots | length')" = "0" ]
}

@test "party relogin fires dml_login player+bot over SOAP" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.xml"
  run bash "$DML" wow party relogin --player Testen --bot Botmage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.relogged')" = "true" ]
  cap="$(cat "$FIXTURE/cap.xml")"; cmd="${cap#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "dml_login Testen Botmage" ]
}

@test "party kick maps a SOAP fault to a bridge-setup hint" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow party kick --player Testen --bot Botmage --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "party kick maps HTTP 401 to SOAP_AUTH" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow party kick --player Testen --bot Botmage --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
}

@test "party kick maps curl connection failure to SOAP_UNREACHABLE" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow party kick --player Testen --bot Botmage --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}

@test "party kick still reports kicked when the dismissal whisper fails (best-effort)" {
  # First SOAP call (uninvite) succeeds, second (logout whisper) faults:
  # the kick itself already worked, so the envelope stays ok with
  # dismissed:false instead of half-failing after the uninvite.
  use_curl_stub
  export DML_STUB_CURL_SEQ="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml $BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  export DML_STUB_CURL_SEQ_STATE="$FIXTURE/soap.state"
  run bash "$DML" wow party kick --player Testen --bot Botmage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.kicked')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.dismissed')" = "false" ]
}

# ---------- botcmd (My Party phase 2) ----------

@test "party botcmd fires the exact whisper string for each action" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action gear --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.sent')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.action')" = "gear" ]
  grep -q 'dml_whisper Testen Botmage autogear' "$FIXTURE/cap.txt"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action talents --json
  [ "$status" -eq 0 ]
  grep -q 'dml_whisper Testen Botmage talents autopick' "$FIXTURE/cap.txt"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action maintain --json
  [ "$status" -eq 0 ]
  grep -q 'dml_whisper Testen Botmage maintenance' "$FIXTURE/cap.txt"
}

@test "party botcmd rejects an unknown action with the allowlist hint" {
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action dance --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  echo "$output" | grep -q 'gear talents maintain'
}

@test "party botcmd rejects invalid player and bot names" {
  run bash "$DML" wow party botcmd --player 'x; drop' --bot Botmage --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow party botcmd --player Testen --bot 'x; drop' --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "party botcmd offline player maps to NOT_FOUND naming the player" {
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party botcmd --player Ghost --bot Botmage --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -q 'Ghost'
}

@test "party botcmd offline bot maps to NOT_FOUND with the party hint" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/none.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/none.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party botcmd --player Testen --bot Goneb --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -q 'Goneb'
  echo "$output" | grep -qi 'party'
}

@test "party botcmd maps a SOAP fault to SOAP_FAULT with the bridge-setup hint" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
  echo "$output" | grep -q 'bridge-setup'
}

# ---------- --spec (Batch 5 F5: party wizard light) ----------

# Stubbed confirmed join: guid lookup -> pre-fire snapshot -> post-fire poll
# (new bot 9001) -> bot-name lookup, same SEQ pattern as the add tests above.
seed_confirmed_join() {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/before.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after.tsv"
  printf 'Botmage\n' > "$FIXTURE/botname.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/before.tsv $FIXTURE/after.tsv $FIXTURE/botname.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=3 DML_PARTY_POLL_SLEEP=0
}

@test "party add --spec whispers the exact spec then autogear after a confirmed join" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/cap.txt"
  use_mysql_stub
  seed_confirmed_join
  run bash "$DML" wow party add --player Testen --class mage --spec "frost pve" --json
  [ "$status" -eq 0 ]
  grep -q 'dml_whisper Testen Botmage talents spec frost pve' "$FIXTURE/cap.txt"
  grep -q 'dml_whisper Testen Botmage autogear' "$FIXTURE/cap.txt"
  # order: spec BEFORE autogear (gear must follow the new talents)
  [ "$(grep -n 'talents spec frost pve' "$FIXTURE/cap.txt" | cut -d: -f1)" -lt \
    "$(grep -n 'Botmage autogear' "$FIXTURE/cap.txt" | cut -d: -f1)" ]
  [ "$(echo "$output" | jq -r '.data.spec')" = "frost pve" ]
  [ "$(echo "$output" | jq -r '.data.spec_applied')" = "true" ]
}

@test "party add rejects a spec outside the allowlist before any SOAP call" {
  use_curl_stub
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow party add --player Testen --class mage --spec "frost pve; .server shutdown" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/curl.log" ]
  # names absent from the live conf are rejected too (no invented symmetry)
  run bash "$DML" wow party add --player Testen --class druid --spec "bear pvp" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "party add without --spec has no spec keys in the payload (regression net)" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  seed_confirmed_join
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "true" ]
  [ "$(echo "$output" | jq -r '.data | has("spec")')" = "false" ]
  [ "$(echo "$output" | jq -r '.data | has("spec_applied")')" = "false" ]
}

@test "party add --spec with an unconfirmed join skips the whispers and reports spec_applied:false" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/cap.txt"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/solo.tsv"   # never grows
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/solo.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=2 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --spec "frost pve" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.spec_applied')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.note')" != "null" ]
  ! grep -q 'dml_whisper' "$FIXTURE/cap.txt"
}

@test "party botcmd action spec whispers talents spec <name>" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action spec --spec "resto pve" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.action')" = "spec" ]
  grep -q 'dml_whisper Testen Botmage talents spec resto pve' "$FIXTURE/cap.txt"
}

@test "party botcmd action spec without --spec (or with a bad one) is BAD_ARG" {
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action spec --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action spec --spec "necro pve" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
