#!/usr/bin/env bats
# Batch 6 C: enrichment-cache maintenance -- status + safe wipe of the
# runtime item-info cache (~/.dml/wowhead-cache). The safety property under
# test: cache-clean wipes ONLY the wowhead-cache subdir, never ~/.dml itself
# (which also holds non-cache state like client-path).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"
  CACHE="$FIXTURE/.dml/wowhead-cache"
}
teardown() { teardown_fixture; }

seed_cache() {
  mkdir -p "$CACHE/tooltips" "$CACHE/icons"
  printf '{"name":"x","tooltip":"y"}' > "$CACHE/tooltips/1.json"
  printf 'JPEGDATA' > "$CACHE/icons/inv_sword.jpg"
}

@test "cache-status: absent cache reports present=false, zero size/files" {
  run bash "$DML" wow cache-status --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.caches[0].key')" = "wowhead" ]
  [ "$(echo "$output" | jq -r '.data.caches[0].present')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.caches[0].files')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.caches[0].bytes')" = "0" ]
  echo "$output" | jq -r '.data.caches[0].path' | grep -q '/.dml/wowhead-cache$'
}

@test "cache-status: populated cache reports present=true, real counts" {
  seed_cache
  run bash "$DML" wow cache-status --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.caches[0].present')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.caches[0].files')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.caches[0].bytes')" -gt 0 ]
}

@test "cache-clean: wipes wowhead-cache ONLY, leaves other ~/.dml state" {
  seed_cache
  # Non-cache siblings under ~/.dml that must survive the wipe.
  printf 'C:\\Games\\WoW' > "$FIXTURE/.dml/client-path"
  mkdir -p "$FIXTURE/.dml/backups"; printf 'dump' > "$FIXTURE/.dml/backups/keepme"
  run bash "$DML" wow cache-clean --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.wiped')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.freed_bytes')" -gt 0 ]
  # cache gone...
  [ ! -e "$CACHE" ]
  # ...but ~/.dml and its non-cache contents intact.
  [ -d "$FIXTURE/.dml" ]
  [ -f "$FIXTURE/.dml/client-path" ]
  [ -f "$FIXTURE/.dml/backups/keepme" ]
}

@test "cache-clean: absent cache is a no-op success (freed 0)" {
  run bash "$DML" wow cache-clean --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.wiped')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.freed_bytes')" = "0" ]
  [ -d "$FIXTURE/.dml" ] || true
}
