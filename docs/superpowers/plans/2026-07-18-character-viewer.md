# Character Viewer Upgrade Implementation Plan (Round E)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `dml wow item-info` (wowhead tooltips + icons, disk-cached, local fallback) + an in-game-style paperdoll on Dashboard with hover tooltips.

**Architecture:** New `cli/src/46-iteminfo.sh`: per-entry pipeline cached-tooltip → wowhead fetch → icon fetch/b64 → local `item_template` fallback → `unavailable`; the verb NEVER errors on network/DB trouble. Wowhead's raw JSON is embedded verbatim in the envelope (no runtime jq — only an icon-name regex extract). UI: slot-grid paperdoll, one batched `wowItemInfo` call, DOMParser-allowlist sanitizer for the remote HTML, WoW-styled tooltip.

**Tech Stack:** bash + bats (curl stub gains `-o`/sequence modes), Rust thin command, Svelte 5, TypeScript (+ jsdom devDep for sanitizer tests).

## Global Constraints

- Branch `feat/dml-launcher-windows`; NO merge. `cli/dml` committed artifact (build.sh). `set -euo pipefail` discipline; NO `local` in dispatch arms. jq is TEST-ONLY — never a runtime dependency (hence the verbatim-embed + regex-icon design).
- URLs (exact): tooltip `<base>/wotlk/tooltip/item/<entry>?dataEnv=8&locale=0` with base `${DML_WOWHEAD_BASE:-https://nether.wowhead.com}`; icon `<base>/images/wow/icons/large/<icon>.jpg` with base `${DML_ZAMIMG_BASE:-https://wow.zamimg.com}`.
- Cache: `~/.dml/wowhead-cache/tooltips/<entry>.json` + `icons/<icon>.jpg`; fetches write to `.tmp` then `mv` on HTTP 200 only (no partial cache files); a cached tooltip that is not `{…}` JSON is deleted (poisoned) and the item falls back local.
- Icon fetches are binary — NEVER captured into a bash variable (NUL loss); `curl -o <file> -w '%{http_code}'` only. All curl calls `</dev/null --max-time 10`, guarded for `set -e`.
- `--entries` `^[0-9]+(,[0-9]+)*$`, each `10#`-normalized, deduped, max 25 → `BAD_ARG` otherwise.
- Output items (three shapes, `source` discriminates): `{"entry","source":"wowhead","icon":name|null,"icon_b64":b64|null,"wowhead":<raw wowhead object>}` / `{"entry","source":"local","name","quality","tooltip_html"}` / `{"entry","source":"unavailable"}`.
- Icon-name extraction regex: `\"icon\":\"([A-Za-z0-9_.-]+)\"` — nothing else is parsed out of wowhead JSON.
- Local fallback HTML: quality-classed name (`<b class="qN">`), `Item Level` line (`class="q"`), optional Armor / `min - max Damage` + `Speed x.xx` / `+V <stat>` lines / `Requires Level` (`class="q1"`); item name HTML-escaped via the existing `_xml_escape`.
- Sanitizer contract (`launcher/src/lib/tooltip.ts`): DOMParser walk; allowed tags `TABLE,TBODY,TR,TD,TH,SPAN,DIV,B,I,SMALL,BR`; `<A>` becomes `<span>`; ALL attributes dropped except `class` matching `/^[\w -]+$/`; disallowed nodes contribute their text content only; `<script>`/`<style>` contribute NOTHING. `{@html}` is fed ONLY through this function.
- Gates: full bats; `npm test`; `npm run check`; `cargo test`. Baselines entering E: bats 329, vitest 20, cargo 18, check 0/0.
- Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 1: CLI `wow item-info` + curl-stub upgrade + bats

**Files:** Create `cli/src/46-iteminfo.sh`; add `item-info)` arm in `90-main.sh` (inside the `wow` case, after `paperdoll)`); REPLACE `use_curl_stub` in `cli/tests/helpers/env.bash` (back-compatible superset); create `cli/tests/wow-item-info.bats`. Commit regenerated `cli/dml`.

- [ ] **Step 1: Replace `use_curl_stub` in env.bash** with (superset — legacy SOAP behavior byte-identical when the new env vars are unset and no `-o` is passed):

```bash
use_curl_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/curl" <<'EOS'
#!/usr/bin/env bash
# Canned responder. Legacy mode (SOAP): emit DML_STUB_SOAP_RESPONSE then
# "\n<code>". -o mode (item-info): write the body to the -o target, print a
# bare code. DML_STUB_CURL_SEQ = space-sep response files consumed one per
# call (sticky last; state file in DML_STUB_CURL_SEQ_STATE);
# DML_STUB_HTTP_SEQ = matching space-sep http codes (sticky last).
# DML_STUB_CURL_LOG captures argv per call.
outfile=""
args=("$@")
for i in "${!args[@]}"; do
  [[ "${args[$i]}" == "-o" ]] && outfile="${args[$((i+1))]}"
done
resp="${DML_STUB_SOAP_RESPONSE:-}"
code="${DML_STUB_HTTP:-200}"
if [[ -n "${DML_STUB_CURL_SEQ:-}" ]]; then
  st="${DML_STUB_CURL_SEQ_STATE:-/tmp/dml_curl_seq.$$}"
  i=0; [[ -f "$st" ]] && i="$(cat "$st")"
  files=($DML_STUB_CURL_SEQ)
  idx=$i; (( idx >= ${#files[@]} )) && idx=$(( ${#files[@]} - 1 ))
  resp="${files[$idx]}"
  if [[ -n "${DML_STUB_HTTP_SEQ:-}" ]]; then
    codes=($DML_STUB_HTTP_SEQ)
    cidx=$i; (( cidx >= ${#codes[@]} )) && cidx=$(( ${#codes[@]} - 1 ))
    code="${codes[$cidx]}"
  fi
  echo $(( i + 1 )) > "$st"
fi
[[ -n "${DML_STUB_CURL_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_CURL_LOG"
if [[ -n "${DML_STUB_CAPTURE_APPEND:-}" ]]; then
  cat >> "$DML_STUB_CAPTURE_APPEND"
elif [[ -n "${DML_STUB_CAPTURE:-}" ]]; then
  cat > "$DML_STUB_CAPTURE"
else
  cat >/dev/null
fi
if [[ -n "$outfile" ]]; then
  if [[ -n "$resp" && -f "$resp" ]]; then cat "$resp" > "$outfile"; else : > "$outfile"; fi
  printf '%s' "$code"
else
  [[ -n "$resp" && -f "$resp" ]] && cat "$resp"
  printf '\n%s' "$code"
fi
exit "${DML_STUB_CURL_EXIT:-0}"
EOS
  chmod +x "$STUB_BIN/curl"
  export PATH="$STUB_BIN:$PATH"
}
```

- [ ] **Step 2: bats `cli/tests/wow-item-info.bats`:**

```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  use_curl_stub
  export HOME="$FIXTURE"
  export DML_STUB_CURL_SEQ_STATE="$FIXTURE/curlseq"
  printf '{"name":"Thunderfury","quality":5,"icon":"inv_sword_39","tooltip":"<table><tr><td><b class=\\"q5\\">Thunderfury</b></td></tr></table>"}' > "$FIXTURE/wh.json"
  printf 'JPGDATA' > "$FIXTURE/icon.jpg"
}
teardown() { teardown_fixture; }

@test "item-info: entries validation" {
  run bash "$DML" wow item-info --entries "abc" --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  run bash "$DML" wow item-info --entries "$(seq 1 26 | tr '\n' ',' | sed 's/,$//')" --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'max 25'
}

@test "item-info: wowhead 200 -> embedded json + icon b64 + cache files" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ "$(echo "$output" | jq -r '.data.items[0].wowhead.name')" = "Thunderfury" ]
  [ "$(echo "$output" | jq -r '.data.items[0].icon')" = "inv_sword_39" ]
  [ "$(echo "$output" | jq -r '.data.items[0].icon_b64')" = "$(base64 -w0 < "$FIXTURE/icon.jpg")" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/tooltips/19019.json" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/icons/inv_sword_39.jpg" ]
}

@test "item-info: second call is served from cache (no curl)" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  bash "$DML" wow item-info --entries 19019 --json >/dev/null
  export DML_STUB_CURL_LOG="$FIXTURE/curl2.log"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ ! -f "$FIXTURE/curl2.log" ]
}

@test "item-info: 404 -> local fallback from item_template" {
  export DML_STUB_HTTP=404
  printf 'Casino Chip\t3\t80\t0\t0\t0\t0\t0\t7\t10\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 990001 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "local" ]
  [ "$(echo "$output" | jq -r '.data.items[0].name')" = "Casino Chip" ]
  echo "$output" | jq -r '.data.items[0].tooltip_html' | grep -q '+10 Stamina'
  echo "$output" | jq -r '.data.items[0].tooltip_html' | grep -q 'Item Level 80'
}

@test "item-info: curl dead + DB empty -> unavailable, verb still ok" {
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow item-info --entries 424242 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "unavailable" ]
}

@test "item-info: dedup" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  run bash "$DML" wow item-info --entries 19019,19019,019019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items | length')" = "1" ]
}

@test "item-info: poisoned tooltip cache is dropped and falls back local" {
  mkdir -p "$FIXTURE/.dml/wowhead-cache/tooltips"
  printf '<html>error page</html>' > "$FIXTURE/.dml/wowhead-cache/tooltips/5555.json"
  export DML_STUB_HTTP=404
  printf 'X\t1\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 5555 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "local" ]
  [ ! -f "$FIXTURE/.dml/wowhead-cache/tooltips/5555.json" ]
}

@test "item-info: weapon damage line renders in local fallback" {
  export DML_STUB_HTTP=404
  printf 'Blade\t2\t20\t0\t15\t10\t20\t2600\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 777 --json
  html="$(echo "$output" | jq -r '.data.items[0].tooltip_html')"
  echo "$html" | grep -q '10 - 20 Damage'
  echo "$html" | grep -q 'Speed 2.60'
  echo "$html" | grep -q 'Requires Level 15'
}
```

- [ ] **Step 3: run — FAIL. Step 4: create `cli/src/46-iteminfo.sh`:**

```bash
# ---------------------------------------------------------------------------
# Item tooltip/icon info (Round E): wowhead-sourced, disk-cached, with a
# local item_template fallback for server-custom items. jq is test-only, so
# wowhead JSON is embedded verbatim; only the icon NAME is regex-extracted.
# The verb never fails on network/DB trouble -- degradation is per-item.
# ---------------------------------------------------------------------------

_wowhead_base()   { echo "${DML_WOWHEAD_BASE:-https://nether.wowhead.com}"; }
_zamimg_base()    { echo "${DML_ZAMIMG_BASE:-https://wow.zamimg.com}"; }
_iteminfo_cache() { echo "$HOME/.dml/wowhead-cache"; }

# Fetch url ($1) into file ($2); echoes the http code ("000" on transport
# failure). Body goes straight to disk (icons are binary -- a bash variable
# would eat NUL bytes). Always returns 0.
_iteminfo_fetch() {
    local code
    if code="$(curl -s -o "$2" -w '%{http_code}' --max-time 10 "$1" </dev/null 2>/dev/null)"; then :; else code=000; fi
    printf '%s' "$code"
    return 0
}

_iteminfo_stat_name() {
    case "$1" in
        3) echo "Agility" ;; 4) echo "Strength" ;; 5) echo "Intellect" ;;
        6) echo "Spirit" ;; 7) echo "Stamina" ;;
        12) echo "Defense Rating" ;; 13) echo "Dodge Rating" ;;
        14) echo "Parry Rating" ;; 15) echo "Block Rating" ;;
        31) echo "Hit Rating" ;; 32) echo "Critical Strike Rating" ;;
        35) echo "Resilience Rating" ;; 36) echo "Haste Rating" ;;
        37) echo "Expertise Rating" ;; 38) echo "Attack Power" ;;
        43) echo "Mana per 5 sec." ;; 45) echo "Spell Power" ;;
        *) echo "Stat $1" ;;
    esac
}

# Minimal in-game-style tooltip from item_template (custom/unknown-to-wowhead
# items). Prints a "local" item object, or "unavailable" when the DB can't
# answer either.
_iteminfo_local() {
    local entry="$1" row
    if row="$(db_world_query "SELECT name,Quality,ItemLevel,armor,RequiredLevel,dmg_min1,dmg_max1,delay,stat_type1,stat_value1,stat_type2,stat_value2,stat_type3,stat_value3,stat_type4,stat_value4,stat_type5,stat_value5 FROM item_template WHERE entry=$entry LIMIT 1;")"; then :; else row=""; fi
    if [[ -z "$row" ]]; then
        printf '{"entry":%s,"source":"unavailable"}' "$entry"
        return 0
    fi
    local name q ilvl armor rlvl dmin dmax delay t1 v1 t2 v2 t3 v3 t4 v4 t5 v5
    IFS=$'\t' read -r name q ilvl armor rlvl dmin dmax delay t1 v1 t2 v2 t3 v3 t4 v4 t5 v5 <<< "$row"
    local hname html pair st sv spd
    hname="$(_xml_escape "$name")"
    html="<b class=\"q$q\">$hname</b><br><span class=\"q\">Item Level $ilvl</span>"
    [[ "$armor" -gt 0 ]] 2>/dev/null && html+="<br><span class=\"q1\">$armor Armor</span>"
    if [[ "$dmax" -gt 0 ]] 2>/dev/null; then
        spd="$(awk "BEGIN{printf \"%.2f\", $delay/1000}")"
        html+="<br><span class=\"q1\">$dmin - $dmax Damage</span> <span class=\"q1\">Speed $spd</span>"
    fi
    for pair in "$t1:$v1" "$t2:$v2" "$t3:$v3" "$t4:$v4" "$t5:$v5"; do
        st="${pair%%:*}"; sv="${pair##*:}"
        [[ -z "$st" || -z "$sv" || "$sv" == 0 ]] && continue
        html+="<br><span class=\"q1\">+$sv $(_iteminfo_stat_name "$st")</span>"
    done
    [[ "$rlvl" -gt 0 ]] 2>/dev/null && html+="<br><span class=\"q1\">Requires Level $rlvl</span>"
    printf '{"entry":%s,"source":"local","name":"%s","quality":%s,"tooltip_html":"%s"}' \
        "$entry" "$(json_escape "$name")" "$q" "$(json_escape "$html")"
    return 0
}

# One item object (wowhead -> local -> unavailable). Never fails.
_iteminfo_one() {
    local entry="$1" cache tj raw code icon="" iconfile iconjson=null b64json=null
    cache="$(_iteminfo_cache)"
    tj="$cache/tooltips/$entry.json"
    if [[ ! -f "$tj" ]]; then
        code="$(_iteminfo_fetch "$(_wowhead_base)/wotlk/tooltip/item/$entry?dataEnv=8&locale=0" "$tj.tmp")"
        if [[ "$code" == 200 ]]; then mv "$tj.tmp" "$tj"; else rm -f "$tj.tmp"; fi
    fi
    if [[ -f "$tj" ]]; then
        raw="$(cat "$tj")"
        if [[ "$raw" == \{*\} ]]; then
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
            printf '{"entry":%s,"source":"wowhead","icon":%s,"icon_b64":%s,"wowhead":%s}' \
                "$entry" "$iconjson" "$b64json" "$raw"
            return 0
        fi
        rm -f "$tj"   # poisoned cache entry (non-JSON body)
    fi
    _iteminfo_local "$entry"
    return 0
}
```

- [ ] **Step 5: the `item-info)` arm in `90-main.sh`** (inside the `wow` case, after `paperdoll)`):

```bash
      item-info)
        entries=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --entries) _need_flag_val "$1" $#; entries="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" "Usage: dml wow item-info --entries 1,2,3 --json"; exit 1 ;;
          esac
        done
        if [[ ! "$entries" =~ ^[0-9]+(,[0-9]+)*$ ]]; then
          json_err BAD_ARG "--entries must be comma-separated item ids" ""; exit 1
        fi
        IFS=',' read -r -a earr <<< "$entries"
        if (( ${#earr[@]} > 25 )); then
          json_err BAD_ARG "--entries max 25 ids per call" ""; exit 1
        fi
        mkdir -p "$(_iteminfo_cache)/tooltips" "$(_iteminfo_cache)/icons"
        declare -A _ii_seen=()
        iout='['; first=1
        for ie in "${earr[@]}"; do
          ie=$((10#$ie))
          [[ -n "${_ii_seen[$ie]:-}" ]] && continue
          _ii_seen["$ie"]=1
          iobj="$(_iteminfo_one "$ie")"
          [[ $first -eq 0 ]] && iout+=','
          iout+="$iobj"; first=0
        done
        iout+=']'
        json_ok "{\"items\":$iout}"
        ;;
```

- [ ] **Step 6: rebuild; run file (8/8) then FULL suite — expect 337 (329 + 8). The curl-stub rewrite must keep every SOAP/console suite green — if anything SOAP-ish fails, the stub broke back-compat; fix the stub, not the tests. Step 7: commit** `feat(cli): wow item-info — wowhead tooltips/icons with local fallback`.

---

### Task 2: Rust command + api.ts

**Files:** `launcher/src-tauri/src/lib.rs`, `launcher/src/lib/api.ts`.

- [ ] **Step 1: lib.rs** (after `wow_paperdoll`):

```rust
#[tauri::command]
async fn wow_item_info(
    entries: Vec<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let csv = entries.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(",");
    run_json_cmd(state, vec!["wow".into(), "item-info".into(), "--entries".into(), csv]).await
}
```

Register `wow_item_info,` after `wow_paperdoll,`.

- [ ] **Step 2: api.ts** (after `wowPaperdoll`):

```ts
export interface WowheadTooltip {
  name: string;
  quality: number;
  icon: string;
  tooltip: string;
}
export interface ItemInfo {
  entry: number;
  source: "wowhead" | "local" | "unavailable";
  icon?: string | null;
  icon_b64?: string | null;
  wowhead?: WowheadTooltip;
  name?: string;
  quality?: number;
  tooltip_html?: string;
}
export async function wowItemInfo(entries: number[]): Promise<ItemInfo[]> {
  const d = await invoke<{ items: ItemInfo[] }>("wow_item_info", { entries });
  return d.items;
}
```

- [ ] **Step 3: gates** — `cargo test` (18), `npm test` (20), `npm run check` (0/0). **Step 4: commit** `feat(launcher): wow_item_info command + api wrapper`.

---

### Task 3: sanitizer + Dashboard paperdoll UI

**Files:** Create `launcher/src/lib/tooltip.ts` + `launcher/src/lib/tooltip.test.ts`; modify `launcher/src/lib/pages/Dashboard.svelte`; `launcher/package.json` gains devDependency `jsdom` (for the DOM-based test file: `npm i -D jsdom`, and the test file starts with `// @vitest-environment jsdom`).

- [ ] **Step 1: `tooltip.ts`:**

```ts
const ALLOWED_TAGS = new Set(["TABLE", "TBODY", "TR", "TD", "TH", "SPAN", "DIV", "B", "I", "SMALL", "BR"]);
const CLASS_RE = /^[\w -]+$/;

// wowhead tooltip HTML is REMOTE content rendered via {@html} — everything
// must pass through this allowlist rebuild. <a> demotes to <span>; only the
// class attribute survives; script/style contribute nothing at all.
export function sanitizeTooltipHtml(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const out = doc.createElement("div");
  const copy = (from: Node, to: Element): void => {
    for (const child of Array.from(from.childNodes)) {
      if (child.nodeType === Node.TEXT_NODE) {
        to.appendChild(doc.createTextNode(child.textContent ?? ""));
        continue;
      }
      if (child.nodeType !== Node.ELEMENT_NODE) continue;
      const el = child as Element;
      const tag = el.tagName;
      if (tag === "SCRIPT" || tag === "STYLE") continue;
      if (ALLOWED_TAGS.has(tag) || tag === "A") {
        const name = tag === "A" ? "span" : tag.toLowerCase();
        const clone = doc.createElement(name);
        const cls = el.getAttribute("class");
        if (cls && CLASS_RE.test(cls)) clone.setAttribute("class", cls);
        to.appendChild(clone);
        copy(el, clone);
      } else {
        copy(el, to); // unknown wrapper: keep the text/children, drop the tag
      }
    }
  };
  copy(doc.body, out);
  return out.innerHTML;
}
```

- [ ] **Step 2: `tooltip.test.ts`** (first line `// @vitest-environment jsdom`): tests — script tag AND its text vanish; `onerror`/`onclick`/`style`/`href` attributes stripped while class survives; `<a class="q4">X</a>` → `<span class="q4">X</span>`; unknown tag (`<img>`, `<iframe>`) dropped but inner text kept (img has none — assert iframe text kept, img gone); nested table structure preserved; malicious class (`q4" onmouseover="x`) dropped. Run `npm test` — expect 26 (20 + 6).

- [ ] **Step 3: Dashboard.svelte rework — binding requirements** (read the current file + ModuleManager for patterns first):
  - Keep: CharPicker + Show gear flow, the four-state server card, the "last save" note. Replace the gear `<table>` with the paperdoll grid.
  - Slot layout (AC slot → label): LEFT column top-to-bottom `0 Head, 1 Neck, 2 Shoulders, 14 Back, 4 Chest, 3 Shirt, 18 Tabard, 8 Wrists`; RIGHT column `9 Hands, 5 Waist, 6 Legs, 7 Feet, 10 Ring, 11 Ring, 12 Trinket, 13 Trinket`; BOTTOM row centered `15 Main Hand, 16 Off Hand, 17 Ranged`. Character summary (name, level+class via `className`, gold) sits between the columns.
  - Slots: 40px boxes, `#0d1117` bg, border `1px solid` quality color when filled (use `QUALITY_COLORS`), `#30363d` dashed when empty. Filled + icon: `<img src="data:image/jpeg;base64,{icon_b64}">` (36px, centered). Filled without icon: quality-colored square showing the item name's first letter. Alt/aria: item name.
  - After `wowPaperdoll` succeeds: fire ONE `wowItemInfo(entries)` (non-blocking — grid renders immediately, icons pop in). Module-level `const infoCache = new Map<number, ItemInfo>()`; only fetch entries missing from the cache; merge results in.
  - Hover tooltip: mouseenter/focus on a filled slot shows a positioned tooltip `<div class="wow-tooltip">`; content per source: `wowhead` → `{@html sanitizeTooltipHtml(info.wowhead.tooltip)}`; `local` → `{@html sanitizeTooltipHtml(info.tooltip_html)}`; `unavailable`/not-yet-loaded → plain `<b>` with the paperdoll name in its quality color + `ilvl X` line. Hide on mouseleave/blur.
  - Tooltip styling (the in-game look): `background: linear-gradient(#0a0a14f2, #10102af2)`, `border: 1px solid #8f8f66`, `border-radius: 5px`, `padding: 10px 12px`, `max-width: 320px`, `font-size: 13px`, positioned right of the slot (flip left when within 340px of the right edge; clamp vertically). CSS for wowhead classes (`:global` within the tooltip container): `.q { color:#ffd100 }`, `.q0 {#9d9d9d} .q1 {#ffffff} .q2 {#1eff00} .q3 {#0070dd} .q4 {#a335ee} .q5 {#ff8000} .q6 {#e6cc80} .q7 {#00ccff}`, `table { border-collapse: collapse }`, `td,th { padding:0; text-align:left }`, `th { text-align:right; color:#9d9d9d; font-weight:normal }`, default text `#ffffff`, `.whtt-extra { color:#9d9d9d }`.
  - No layout shift: tooltip is `position: fixed`, rendered only while hovered.
- [ ] **Step 4: gates** — `npm test` (26) + `npm run check` (0/0). **Step 5: commit** `feat(launcher): in-game paperdoll with wowhead tooltips + icons`.
