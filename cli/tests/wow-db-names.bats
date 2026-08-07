#!/usr/bin/env bats
# Task 6 (feat/core-family): bash schema-name resolution -- the mirror of
# crates/dml-wow/src/db.rs database_names_for_title / config.rs
# database_info_value. The DB verbs resolve acore_world/characters/auth (and
# the OPTIONAL playerbots schema) from the server's own config instead of
# hardcoding them: compose env (override, then base) -> worldserver.conf ->
# its .dist, refusing DB_NAMES_UNRESOLVED rather than guessing. These tests
# are the proof of the mirror -- no parity suite compares SQL text or argv,
# so a resolved-vs-hardcoded divergence is invisible to every other test as
# long as both sides land on acore_* (which the stock fixtures do).
#
# NEGATIVE asserts here use `grep -c`-comparison / `run`+status forms, never
# a bare `! cmd`: POSIX exempts a `!`-prefixed command from errexit, so under
# bats a mid-test `! grep -q ...` asserts NOTHING (only a last-line `!`
# counts, via the test function's return value). Caught live 2026-08-07: the
# mutation that made _bot_account_where guess acore_playerbots stayed GREEN
# against a mid-test `! grep playerbots_account_type` until it was rewritten
# to the counting form.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose   # seeds the stock *DatabaseInfo .dists
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

# A base compose whose ac-worldserver environment renames all four schemas --
# plus decoy services carrying the SAME keys, which must never be the source
# (the real native compose has exactly this shape: ac-db-import and
# ac-authserver both carry AC_LOGIN_DATABASE_INFO).
renamed_compose() {
  cat > "$SDIR/docker-compose.yml" <<'EOF'
services:
  ac-db-import:
    environment:
      AC_LOGIN_DATABASE_INFO: "decoy;3306;u;p;decoy_auth"
      AC_WORLD_DATABASE_INFO: "decoy;3306;u;p;decoy_world"
  ac-worldserver:
    tty: true
    environment:
      AC_LOGS_DIR: "/azerothcore/env/dist/logs"
      AC_LOGIN_DATABASE_INFO: "ac-database;3306;root;${DB_ROOT_PASSWORD:-password};my_auth"
      AC_WORLD_DATABASE_INFO: "ac-database;3306;root;pw;my_world"
      AC_CHARACTER_DATABASE_INFO: "ac-database;3306;root;pw;my_chars"
      AC_PLAYERBOTS_DATABASE_INFO: "ac-database;3306;root;pw;my_pb"
  ac-authserver:
    environment:
      AC_LOGIN_DATABASE_INFO: "decoy;3306;u;p;decoy_auth2"
EOF
}

# ---------- (a) renamed names flow end-to-end ----------

@test "renamed schemas: query argv and SQL carry the resolved names, never acore_" {
  renamed_compose
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow items search --name thunder --json
  [ "$status" -eq 0 ]
  grep -q " my_world " "$FIXTURE/q.log"
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  grep -q " my_chars " "$FIXTURE/q.log"
  grep -q "my_pb.playerbots_account_type" "$FIXTURE/q.log"
  grep -q "my_auth.account" "$FIXTURE/q.log"
  [ "$(grep -c "acore_" "$FIXTURE/q.log")" = "0" ]
}

@test "renamed schemas: the mysqldump set is the resolved names" {
  renamed_compose
  use_backup_stub
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  grep -q -- "--databases my_chars my_pb my_auth --single-transaction --quick" "$FIXTURE/calls.log"
  [ "$(grep -c "acore_" "$FIXTURE/calls.log")" = "0" ]
}

# ---------- (b) the LIST compose env form ----------

@test "list-form compose environment resolves (split at the FIRST =)" {
  cat > "$SDIR/docker-compose.yml" <<'EOF'
services:
  ac-worldserver:
    environment:
      - AC_LOGIN_DATABASE_INFO=h;3306;u;p;seq_auth
      - "AC_WORLD_DATABASE_INFO=h;3306;u;p=x;seq_world"
      - AC_CHARACTER_DATABASE_INFO=h;3306;u;p;seq_chars
      - NOEQUALS
EOF
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 0 ]
  grep -q " seq_world " "$FIXTURE/q.log"
}

@test "config list reads the override's LIST-form environment too" {
  # _cfg_env_load_map's yq program widened in the SAME change as the Rust
  # parse_override_env Sequence arm -- the two surfaces learn the form
  # together (the env map feeds config list, whose parity is pinned).
  use_mysql_stub
  cat > "$SDIR/docker-compose.override.yml" <<'EOF'
services:
  ac-worldserver:
    environment:
      - AC_RATE_XP_KILL=7
EOF
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .value')" = "7" ]
}

# ---------- (c) tier precedence ----------

@test "precedence: dist answers alone, conf beats dist, base compose beats conf, override beats base" {
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  # 1. only the seeded .dist -> stock name
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 0 ]
  grep -q " acore_world " "$FIXTURE/q.log"
  # 2. a live conf beats the .dist
  cat > "$SDIR/env/dist/etc/worldserver.conf" <<'EOF'
WorldDatabaseInfo = "h;3306;u;p;conf_world"
EOF
  : > "$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 0 ]
  grep -q " conf_world " "$FIXTURE/q.log"
  # 3. the base compose env beats the conf
  cat > "$SDIR/docker-compose.yml" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_WORLD_DATABASE_INFO: "h;3306;u;p;base_world"
EOF
  : > "$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 0 ]
  grep -q " base_world " "$FIXTURE/q.log"
  # 4. the override compose beats the base
  cat > "$SDIR/docker-compose.override.yml" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_WORLD_DATABASE_INFO: "h;3306;u;p;ovr_world"
EOF
  : > "$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 0 ]
  grep -q " ovr_world " "$FIXTURE/q.log"
}

# ---------- (d) the unresolvable refusal ----------

@test "unresolvable names refuse with DB_NAMES_UNRESOLVED and run NO query" {
  rm -f "$SDIR/env/dist/etc/worldserver.conf.dist" \
        "$SDIR/env/dist/etc/modules/playerbots.conf.dist"
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_NAMES_UNRESOLVED" ]
  [ ! -s "$FIXTURE/q.log" ]
}

@test "backup create refuses unresolved names before any docker call" {
  rm -f "$SDIR/env/dist/etc/worldserver.conf.dist" \
        "$SDIR/env/dist/etc/modules/playerbots.conf.dist"
  use_backup_stub
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup create --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"DB_NAMES_UNRESOLVED"'
  # the dump never ran -- nothing was logged
  [ ! -s "$FIXTURE/calls.log" ]
}

# ---------- (e) truncated value ----------

@test "a truncated 4-field value refuses -- the password never becomes a schema" {
  cat > "$SDIR/env/dist/etc/worldserver.conf.dist" <<'EOF'
LoginDatabaseInfo     = "127.0.0.1;3306;acore;secretpw"
WorldDatabaseInfo     = "127.0.0.1;3306;acore;acore;acore_world"
CharacterDatabaseInfo = "127.0.0.1;3306;acore;acore;acore_characters"
EOF
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_NAMES_UNRESOLVED" ]
  [ ! -s "$FIXTURE/q.log" ]
  [[ "$output" != *secretpw* ]]
}

# ---------- (f) hostile charset + all-digits ----------

@test "a schema name outside [A-Za-z0-9_\$] refuses (the splice charset gate)" {
  cat > "$SDIR/env/dist/etc/worldserver.conf.dist" <<'EOF'
LoginDatabaseInfo     = "127.0.0.1;3306;acore;acore;acore_auth"
WorldDatabaseInfo     = "127.0.0.1;3306;acore;acore;bad-name`x"
CharacterDatabaseInfo = "127.0.0.1;3306;acore;acore;acore_characters"
EOF
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_NAMES_UNRESOLVED" ]
  [ ! -s "$FIXTURE/q.log" ]
}

@test "an all-digits schema name refuses (not a legal unquoted identifier)" {
  cat > "$SDIR/env/dist/etc/worldserver.conf.dist" <<'EOF'
LoginDatabaseInfo     = "127.0.0.1;3306;acore;acore;acore_auth"
WorldDatabaseInfo     = "127.0.0.1;3306;acore;acore;12345"
CharacterDatabaseInfo = "127.0.0.1;3306;acore;acore;acore_characters"
EOF
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_NAMES_UNRESOLVED" ]
  [ ! -s "$FIXTURE/q.log" ]
}

# ---------- (g) the re-anchored write allowlist ----------

@test "write allowlist: the resolved characters name is accepted on a renamed server" {
  renamed_compose
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  printf '7\t0\n' > "$FIXTURE/row.tsv"   # guid 7, offline
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  run bash "$DML" wow teleport-coords --char Kiddo --map 0 --x 1 --y 2 --z 3 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.teleported')" = "true" ]
  update_line="$(grep 'UPDATE characters SET position_x' "$FIXTURE/q.log")"
  [[ "$update_line" == *" my_chars "* ]]
}

@test "write allowlist: a non-member schema is refused on a renamed server" {
  # module repair derives db_full by acore_ prefix concatenation (recorded
  # exception); on a renamed server that literal is NOT in the resolved trio
  # and _db_write_stmt must refuse it -- no INSERT reaches the stub.
  renamed_compose
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  mkdir -p "$SDIR/modules/mod-transmog/.git"
  mkdir -p "$SDIR/modules/mod-transmog/data/sql/db-characters"
  printf 'CREATE TABLE t (id INT);\n' > "$SDIR/modules/mod-transmog/data/sql/db-characters/t.sql"
  run bash "$DML" wow module repair --key mod-transmog --db characters --mode mark --files t.sql --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
  # The refused write never reached docker: no INSERT logged (the log file
  # may not even exist -- grep exit 1 and 2 are both "no match here").
  run grep -q 'INSERT INTO updates' "$FIXTURE/q.log"
  [ "$status" -ne 0 ]
}

# ---------- (h) optional playerbots: absent degrades, never guesses ----------

@test "no playerbots schema: bot SQL is prefix-only -- never LIKE '%', never 0=1" {
  rm -f "$SDIR/env/dist/etc/modules/playerbots.conf.dist"
  use_mysql_stub
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow bots list --json
  [ "$status" -eq 0 ]
  [ "$(grep -c "playerbots_account_type" "$FIXTURE/q.log")" = "0" ]
  grep -q "UPPER(username) LIKE 'RNDBOT%'" "$FIXTURE/q.log"
  [ "$(grep -c "LIKE '%'" "$FIXTURE/q.log")" = "0" ]
  [ "$(grep -c "0=1" "$FIXTURE/q.log")" = "0" ]
}

@test "no playerbots schema: the dump omits it and the copy narrates the omission" {
  rm -f "$SDIR/env/dist/etc/modules/playerbots.conf.dist"
  use_backup_stub
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  grep -q -- "--databases acore_characters acore_auth --single-transaction --quick" "$FIXTURE/calls.log"
  [ "$(grep -c "acore_playerbots" "$FIXTURE/calls.log")" = "0" ]
  # narration mirror (backup.rs dump_narration): no bots promised, warn line
  echo "$output" | grep -q '"text":"backing up characters and accounts..."'
  [ "$(echo "$output" | grep -c '"text":"backing up characters, bots')" = "0" ]
  warn_line="$(echo "$output" | grep '"level":"warn"')"
  [[ "$warn_line" == *"will not include bot data"* ]]
}

# ---------- (i) the narration mirror, resolved side ----------

@test "resolved playerbots: the copy promises bots and there is no omission warn" {
  use_backup_stub
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  grep -q -- "--databases acore_characters acore_playerbots acore_auth --single-transaction --quick" "$FIXTURE/calls.log"
  echo "$output" | grep -q '"text":"backing up characters, bots and accounts..."'
  [ "$(echo "$output" | grep -c "will not include bot data")" = "0" ]
}
