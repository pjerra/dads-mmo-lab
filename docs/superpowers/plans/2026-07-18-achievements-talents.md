# Achievements & Talents Implementation Plan (Round G)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `wow char-progress` (achievements + active-spec talents from the DB) and `wow entity-info` (wowhead spell/achievement tooltips+icons), rendered as two Dashboard cards with hover tooltips.

**Architecture:** `char-progress` = three read-only characters-DB queries (guid → achievements → talents with the specMask filter done in SQL). `entity-info` generalizes Round E's fetch/cache/icon machinery by kind (`spell|achievement`), no local fallback, `item-info` untouched (its bats are the regression canary). UI reuses the Round E hover-tooltip + session-cache pattern, keyed `kind:id`, chunked ≤25 ids per call, progressive.

**Tech Stack:** bash+bats (mysql ROWS_SEQ + curl SEQ stubs), Rust thin commands, Svelte 5.

## Global Constraints

- Branch `feat/dml-launcher-windows`; NO merge. `cli/dml` committed artifact. `set -euo pipefail` discipline; NO `local` in dispatch arms; jq test-only (entity JSON embedded verbatim like items).
- char-progress contract mirrors paperdoll: `_valid_charname` before ANY SQL; unknown char → `NOT_FOUND`; DB failure → `DB_UNREACHABLE`; all interpolated values are validated integers or `sql_escape`d.
- Talent filter EXACT: active group from `characters.activeTalentGroup`, spells where `(specMask & (1 << activeTalentGroup))`, ordered by spell id. Only the active spec is returned.
- entity-info: `--kind` closed set `spell|achievement` (BAD_ARG otherwise); `--ids` same validation as item-info (csv regex, `10#`, dedup, max 25); cache files `tooltips/<kind>-<id>.json` (items keep `tooltips/<id>.json` — pinned by the existing item-info bats); same `{…}` + `"name":"` + `"tooltip":"` embed gates and poisoned-cache drop; icons shared with the item icon cache; misses → `{"id":N,"source":"unavailable"}`; the verb never errors on network trouble.
- UI: cards render only when a doll is loaded; entity loading progressive + per-entity degradation; the Dashboard never breaks from card failures. Copy (exact): Talents header `Talents`, summary `<n> talents (active spec)`, badge `Dual spec`; Achievements header `Achievements`, summary `<total> earned`. Dates `YYYY-MM-DD` (UTC) from epoch.
- Gates: full bats; `npm test`; `npm run check`; `cargo test`. Baselines entering G: bats 340, vitest 32, cargo 25, check 0/0. Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 1: CLI `char-progress` + `entity-info` + bats

**Files:** Append helpers to `cli/src/46-iteminfo.sh`; add two arms in `90-main.sh` (`char-progress)` after `paperdoll)`; `entity-info)` after `item-info)`); create `cli/tests/wow-char-progress.bats` + `cli/tests/wow-entity-info.bats`. Commit regenerated `cli/dml`.

- [ ] **Step 1: helpers appended to `cli/src/46-iteminfo.sh`:**

```bash
# --- kind-generalized wowhead entities (Round G: spell|achievement) --------
# Same fetch/cache/icon machinery as items, but NO local fallback -- these
# kinds have no names in the server DB. Cache key carries the kind so ids
# can't collide across kinds (items keep their legacy un-prefixed files).
_entity_one() {
    local kind="$1" id="$2" cache tj raw code icon="" iconfile iconjson=null b64json=null
    cache="$(_iteminfo_cache)"
    tj="$cache/tooltips/$kind-$id.json"
    if [[ ! -f "$tj" ]]; then
        code="$(_iteminfo_fetch "$(_wowhead_base)/wotlk/tooltip/$kind/$id?dataEnv=8&locale=0" "$tj.tmp")"
        if [[ "$code" == 200 ]]; then mv "$tj.tmp" "$tj"; else rm -f "$tj.tmp"; fi
    fi
    if [[ -f "$tj" ]]; then
        raw="$(cat "$tj")"
        if [[ "$raw" == \{*\} && "$raw" == *'"name":"'* && "$raw" == *'"tooltip":"'* ]]; then
            if [[ "$raw" =~ \"icon\":\"([A-Za-z0-9_.-]+)\" ]]; then icon="${BASH_REMATCH[1]}"; fi
            if [[ -n "$icon" ]]; then
                iconfile="$cache/icons/$icon.jpg"
                if [[ ! -f "$iconfile" ]]; then
                    code="$(_iteminfo_fetch "$(_zamimg_base)/images/wow/icons/large/$icon.jpg" "$iconfile.tmp")"
                    if [[ "$code" == 200 ]]; then mv "$iconfile.tmp" "$iconfile"; else rm -f "$iconfile.tmp"; fi
                fi
                if [[ -f "$iconfile" ]]; then
                    b64json="\"$(base64 -w0 < "$iconfile")\""
                fi
                iconjson="\"$icon\""
            fi
            printf '{"id":%s,"source":"wowhead","icon":%s,"icon_b64":%s,"wowhead":%s}' \
                "$id" "$iconjson" "$b64json" "$raw"
            return 0
        fi
        rm -f "$tj"   # poisoned cache entry
    fi
    printf '{"id":%s,"source":"unavailable"}' "$id"
    return 0
}
```

- [ ] **Step 2: the two arms in `90-main.sh`.** `char-progress)` (after `paperdoll)`'s `;;`):

```bash
      char-progress)
        char=""
        [[ "${1:-}" == "--char" ]] && { _need_flag_val "$1" $#; char="$2"; shift 2; }
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        cguid="$(db_chars_query "SELECT guid FROM characters WHERE name='$(sql_escape "$char")' LIMIT 1;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        [[ "$cguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "No such character: $char" ""; exit 1; }
        atrow="$(db_chars_query "SELECT activeTalentGroup, talentGroupsCount FROM characters WHERE guid=$cguid;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        IFS=$'\t' read -r agroup gcount <<< "$atrow"
        [[ "$agroup" =~ ^[0-9]+$ ]] || agroup=0
        [[ "$gcount" =~ ^[0-9]+$ ]] || gcount=1
        atotal="$(db_chars_query "SELECT COUNT(*) FROM character_achievement WHERE guid=$cguid;")" || atotal=0
        [[ "$atotal" =~ ^[0-9]+$ ]] || atotal=0
        arecent='['; first=1
        while IFS=$'\t' read -r aid adate; do
          [[ -z "$aid" ]] && continue
          [[ "$aid" =~ ^[0-9]+$ ]] || continue
          [[ "$adate" =~ ^[0-9]+$ ]] || adate=0
          [[ $first -eq 0 ]] && arecent+=','
          arecent+="{\"id\":$aid,\"date\":$adate}"
          first=0
        done < <(db_chars_query "SELECT achievement, date FROM character_achievement WHERE guid=$cguid ORDER BY date DESC LIMIT 10;" || true)
        arecent+=']'
        tspells='['; first=1
        while IFS= read -r sid; do
          [[ -z "$sid" ]] && continue
          [[ "$sid" =~ ^[0-9]+$ ]] || continue
          [[ $first -eq 0 ]] && tspells+=','
          tspells+="$sid"
          first=0
        done < <(db_chars_query "SELECT spell FROM character_talent WHERE guid=$cguid AND (specMask & (1 << $agroup)) ORDER BY spell;" || true)
        tspells+=']'
        json_ok "{\"achievements\":{\"total\":$atotal,\"recent\":$arecent},\"talents\":{\"groups_count\":$gcount,\"active_group\":$agroup,\"spells\":$tspells}}"
        ;;
```

`entity-info)` (after `item-info)`'s `;;`):

```bash
      entity-info)
        ekind=""; eids=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --kind) _need_flag_val "$1" $#; ekind="$2"; shift 2 ;;
            --ids) _need_flag_val "$1" $#; eids="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow entity-info --kind spell|achievement --ids 1,2 --json"; exit 1 ;;
          esac
        done
        case "$ekind" in spell|achievement) ;; *) json_err BAD_ARG "--kind must be spell or achievement" ""; exit 1 ;; esac
        if [[ ! "$eids" =~ ^[0-9]+(,[0-9]+)*$ ]]; then
          json_err BAD_ARG "--ids must be comma-separated ids" ""; exit 1
        fi
        IFS=',' read -r -a eidarr <<< "$eids"
        if (( ${#eidarr[@]} > 25 )); then
          json_err BAD_ARG "--ids max 25 per call" ""; exit 1
        fi
        mkdir -p "$(_iteminfo_cache)/tooltips" "$(_iteminfo_cache)/icons"
        declare -A _ee_seen=()
        eout='['; first=1
        for eid in "${eidarr[@]}"; do
          eid=$((10#$eid))
          [[ -n "${_ee_seen[$eid]:-}" ]] && continue
          _ee_seen["$eid"]=1
          eobj="$(_entity_one "$ekind" "$eid")"
          [[ $first -eq 0 ]] && eout+=','
          eout+="$eobj"; first=0
        done
        eout+=']'
        json_ok "{\"entities\":$eout}"
        ;;
```

- [ ] **Step 3: bats.** `cli/tests/wow-char-progress.bats` (setup: make_fixture + use_mysql_stub + HOME; uses `DML_STUB_DB_ROWS_SEQ` — space-separated row files consumed per query, sticky last, state via `DML_STUB_DB_SEQ_STATE`):

```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/dbseq"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/queries.log"
}
teardown() { teardown_fixture; }

@test "char-progress: full shape (guid, groups, achievements, active-spec talents)" {
  printf '7\n' > "$FIXTURE/r1"                       # guid
  printf '1\t2\n' > "$FIXTURE/r2"                    # activeTalentGroup=1, groups=2
  printf '42\n' > "$FIXTURE/r3"                      # total achievements
  printf '1234\t1700000000\n4567\t1690000000\n' > "$FIXTURE/r4"
  printf '11111\n22222\n' > "$FIXTURE/r5"            # talent spells
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1 $FIXTURE/r2 $FIXTURE/r3 $FIXTURE/r4 $FIXTURE/r5"
  run bash "$DML" wow char-progress --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.achievements.total')" = "42" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent[0].id')" = "1234" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent[0].date')" = "1700000000" ]
  [ "$(echo "$output" | jq -r '.data.talents.groups_count')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.talents.active_group')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.talents.spells | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.talents.spells[0]')" = "11111" ]
  grep -q 'specMask & (1 << 1)' "$FIXTURE/queries.log"
}

@test "char-progress: unknown character -> NOT_FOUND" {
  printf '' > "$FIXTURE/r1"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1"
  run bash "$DML" wow char-progress --char Nobody --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "char-progress: invalid name -> BAD_ARG before any SQL" {
  run bash "$DML" wow char-progress --char 'x;drop' --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  [ ! -f "$FIXTURE/queries.log" ]
}

@test "char-progress: DB down -> DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow char-progress --char Testchar --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'DB_UNREACHABLE'
}

@test "char-progress: empty achievements/talents -> zeros and empty arrays" {
  printf '7\n' > "$FIXTURE/r1"
  printf '0\t1\n' > "$FIXTURE/r2"
  printf '0\n' > "$FIXTURE/r3"
  printf '' > "$FIXTURE/r4"
  printf '' > "$FIXTURE/r5"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1 $FIXTURE/r2 $FIXTURE/r3 $FIXTURE/r4 $FIXTURE/r5"
  run bash "$DML" wow char-progress --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.achievements.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.talents.spells | length')" = "0" ]
}
```

`cli/tests/wow-entity-info.bats` (setup like wow-item-info.bats: mysql stub NOT needed; curl stub + HOME + CURL_SEQ_STATE; a `wh.json` spell fixture `{"name":"Icy Veins","quality":0,"icon":"spell_frost_coldhearted","tooltip":"<b>Icy Veins</b>"}` — note entity JSON needs `"name":"` and `"tooltip":"` only):

```bash
@test "entity-info: kind validation + ids validation" — bad kind item → BAD_ARG; bad ids abc → BAD_ARG; 26 ids → max 25.
@test "entity-info: spell happy path + kind-prefixed cache file" — SEQ wh.json + icon.jpg → source wowhead, wowhead.name "Icy Veins", icon name, icon_b64 matches, [ -f "$FIXTURE/.dml/wowhead-cache/tooltips/spell-12472.json" ].
@test "entity-info: achievement kind caches under achievement- prefix" — same fixtures, --kind achievement --ids 2336 → [ -f ...tooltips/achievement-2336.json ].
@test "entity-info: 404 -> unavailable (no local fallback)" — DML_STUB_HTTP=404 → source unavailable; envelope ok:true.
@test "entity-info: cache hit skips curl" — first call, then CURL_LOG export, second call → no log file.
@test "entity-info: item-info regression canary" — run wow item-info --entries 19019 with SEQ fixtures → cache file at tooltips/19019.json (NO kind prefix).
```

Write these six as full bats tests following the sketches exactly (mirror wow-item-info.bats mechanics).

- [ ] **Step 4: run both new files (5/5 + 6/6) then FULL — expect 351 (340 + 11). Step 5: commit** `feat(cli): char-progress + entity-info (achievements/talents via wowhead)`.

---

### Task 2: Rust + api.ts

- lib.rs (after `wow_item_info`): `wow_char_progress(char_name: String)` → `["wow","char-progress","--char",char_name]` via run_json_cmd; `wow_entity_info(kind: String, ids: Vec<u32>)` → `["wow","entity-info","--kind",kind,"--ids",csv]`. Register both.
- api.ts (after `wowItemInfo`):

```ts
export interface AchievementEntry {
  id: number;
  date: number;
}
export interface CharProgress {
  achievements: { total: number; recent: AchievementEntry[] };
  talents: { groups_count: number; active_group: number; spells: number[] };
}
export async function wowCharProgress(charName: string): Promise<CharProgress> {
  return await invoke("wow_char_progress", { charName });
}
export interface EntityInfo {
  id: number;
  source: "wowhead" | "unavailable";
  icon?: string | null;
  icon_b64?: string | null;
  wowhead?: WowheadTooltip;
}
export async function wowEntityInfo(kind: "spell" | "achievement", ids: number[]): Promise<EntityInfo[]> {
  const d = await invoke<{ entities: EntityInfo[] }>("wow_entity_info", { kind, ids });
  return d.entities;
}
```

- Gates: cargo 25, vitest 32, check 0/0. Commit `feat(launcher): char-progress + entity-info commands`.

---

### Task 3: Dashboard cards

**Files:** `launcher/src/lib/pages/Dashboard.svelte`; optionally extract pure helpers into `launcher/src/lib/progress.ts` + `progress.test.ts`.

**Binding requirements** (read the current Dashboard first — reuse its tooltip/hover state and entity plumbing patterns from Round E):
- After `loadDoll` succeeds, also fire `wowCharProgress(charName)` (guarded catch → cards show a muted `Couldn't load progress.` line; never an error card).
- Extract pure helpers (in `progress.ts`, vitest-tested): `chunkIds(ids: number[], size = 25): number[][]` and `formatEpochDate(epoch: number): string` (UTC `YYYY-MM-DD`; epoch 0/invalid → `""`). ~2-4 tests → vitest 34+.
- Entity session cache: extend/generalize the module-level Map to key `` `${kind}:${id}` `` (items keep working — either a second Map or a shared one; items currently key by number: adapt carefully, Round E behavior unchanged).
- Progressive load: for talents + recent achievements, filter uncached ids, `chunkIds(...)`, fire the ≤25-id `wowEntityInfo` calls sequentially (await each, merge, bump the version signal) — all inside a caught async fn.
- **Talents card**: header `Talents`, summary `<n> talents (active spec)` (n = spells.length), `Dual spec` badge when `groups_count > 1`; body = flex-wrap grid of 28px tiles (icon `data:image/jpeg;base64,…`; dim placeholder tile with `title={String(spellId)}` when unavailable/pending); hover/focus on a tile with wowhead data → the existing sanitized tooltip (same positioning/flip machinery as item slots — reuse, don't duplicate, refactoring the hover state to accept any EntityInfo/ItemInfo tooltip source is in-scope).
- **Achievements card**: header `Achievements`, summary `<total> earned`; body = up to 10 rows: 20px icon (or dim placeholder), name from `wowhead.name` (fallback `#<id>`), right-aligned muted date `formatEpochDate(date)`; hover → sanitized tooltip.
- Both cards render only when `doll` is non-null (beneath the model+paperdoll row); busy/error states per card, Dashboard never breaks.
- Gates: `npm test` (34+) + `npm run check` (0/0). Commit `feat(launcher): talents + achievements cards on the character view`.
