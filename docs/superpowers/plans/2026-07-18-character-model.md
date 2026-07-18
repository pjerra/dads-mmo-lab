# 3D Character Model Implementation Plan (Round F)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A live 3D model of the viewed character wearing their gear, beside the Dashboard paperdoll — ZamModelViewer (wrath env) fed through a Tauri `zam://` asset proxy.

**Architecture:** CLI paperdoll gains decoded appearance fields. Rust registers an asynchronous custom URI scheme `zam` whose handler serves a disk cache or fetches `https://wow.zamimg.com/<path>` (no browser Origin → passes zamimg's allowlist) — host fixed, path-prefix allowlisted, traversal-cleaned. The UI vendors a small adapter (ported from the ISC `wow-model-viewer` wrapper per `.superpowers/sdd/recon-modelviewer.md`) driving the wrath viewer with native WotLK display ids; every stage degrades gracefully (the model can never break the paperdoll).

**Tech Stack:** bash+bats, Rust (tauri custom protocol, reqwest blocking/rustls), Svelte 5 + jQuery (bundled npm dep), vitest.

## Global Constraints

- Branch `feat/dml-launcher-windows`; NO merge. `cli/dml` committed artifact. `set -euo pipefail` discipline.
- **THE RECON FILE IS REQUIRED READING for Tasks 2-3:** `.superpowers/sdd/recon-modelviewer.md` — verbatim ZamModelViewer invocation, model-id formula, slot mapping, meta paths, teardown notes, ISC license. Where this plan says "per the recon", the recon is authoritative.
- SSRF hard rules (Task 2): proxy ONLY `https://wow.zamimg.com`; ONLY paths starting `modelviewer/` or `images/`; reject any `..` segment, empty path, embedded scheme (`:` before first `/`), backslashes; query strings stripped for both fetch and cache key. Cache under `<app_cache_dir>/zam-cache/<path>`, atomic write (`.tmp` + rename). Anything rejected → 404 response, NEVER a fetch.
- Paperdoll additions are ADDITIVE — existing fields/shape untouched (back-compat pinned by existing tests staying green).
- Decode (exact): `skin = playerBytes & 0xFF`, `face = (playerBytes >> 8) & 0xFF`, `hair_style = (playerBytes >> 16) & 0xFF`, `hair_color = (playerBytes >> 24) & 0xFF`, `facial_style = playerBytes2 & 0xFF`.
- UI degradation ladder (each step independent): viewer script fails to load → fallback note; customization meta unmappable → base model + items; an item's displayid missing on zamimg → viewer renders the rest (do not gate the whole model on any single item). Fallback copy exactly: `3D model unavailable (needs internet on first view)`.
- Only model-rendered slots are passed to the viewer: AC slots 0,2,3,4,5,6,7,8,9,14,15,16,17,18 (neck/rings/trinkets skipped) — mapped to viewer slots per the recon's table.
- Gates: full bats; `npm test`; `npm run check`; `cargo test`. Baselines entering F: bats 339, vitest 26, cargo 18, check 0/0. Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 1: CLI — paperdoll appearance fields

**Files:** Modify `cli/src/90-main.sh` (paperdoll arm, ~line 1479); create `cli/tests/wow-paperdoll-model.bats`. Commit regenerated `cli/dml`.

- [ ] **Step 1: bats first** (`cli/tests/wow-paperdoll-model.bats`; setup = make_fixture + use_mysql_stub + HOME):

```bash
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

@test "paperdoll: appearance fields decoded from playerBytes" {
  # playerBytes = skin 3 | face 5<<8 | hairStyle 7<<16 | hairColor 9<<24
  pb=$(( 3 | (5 << 8) | (7 << 16) | (9 << 24) ))
  # columns: name level class money race gender playerBytes playerBytes2 slot entry item-name Quality ItemLevel displayid
  printf 'Testchar\t80\t1\t123450000\t2\t1\t%s\t11\t0\t40001\tHelm\t4\t200\t5001\n' "$pb" > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow paperdoll --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.race')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.gender')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.skin')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.face')" = "5" ]
  [ "$(echo "$output" | jq -r '.data.hair_style')" = "7" ]
  [ "$(echo "$output" | jq -r '.data.hair_color')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.facial_style')" = "11" ]
  [ "$(echo "$output" | jq -r '.data.name')" = "Testchar" ]
  [ "$(echo "$output" | jq -r '.data.gold')" = "12345" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].displayid')" = "5001" ]
}
```

NOTE the row column order matches the NEW select order defined in Step 3: `name, level, class, money, race, gender, playerBytes, playerBytes2, slot, entry, item-name, Quality, ItemLevel, displayid` — the new character columns go together right after `money`, BEFORE the per-item columns.

- [ ] **Step 2: run — FAIL. Step 3: modify the paperdoll arm:** change the SELECT to

```
SELECT c.name,c.level,c.class,c.money,c.race,c.gender,c.playerBytes,c.playerBytes2,ci.slot,it.entry,it.name,it.Quality,it.ItemLevel,it.displayid
```

(joins/WHERE unchanged), the read line to

```bash
        while IFS=$'\t' read -r nm lvl cls money crace cgender pbytes pbytes2 slot entry iname q ilvl disp; do
          [[ -z "$nm" ]] && continue
          cname="$nm"; clevel="$lvl"; cclass="$cls"; cmoney="$money"
          crace_s="$crace"; cgender_s="$cgender"; cpb="$pbytes"; cpb2="$pbytes2"
```

(initialize `crace_s=0; cgender_s=0; cpb=0; cpb2=0` beside the other pre-loop defaults), and the final `json_ok` to add, after `"class":$cclass,`:

```bash
        cskin=$(( cpb & 0xFF )); cface=$(( (cpb >> 8) & 0xFF ))
        chairs=$(( (cpb >> 16) & 0xFF )); chairc=$(( (cpb >> 24) & 0xFF ))
        cfacial=$(( cpb2 & 0xFF ))
        json_ok "{\"name\":\"$(json_escape "$cname")\",\"level\":$clevel,\"class\":$cclass,\"race\":$crace_s,\"gender\":$cgender_s,\"skin\":$cskin,\"face\":$cface,\"hair_style\":$chairs,\"hair_color\":$chairc,\"facial_style\":$cfacial,\"gold\":$((cmoney/10000)),\"note\":\"last_saved\",\"equipped\":$eq}"
```

(replacing the existing `json_ok` line — everything after `class` that existed before stays in the same order after the new fields).

- [ ] **Step 4: rebuild; new file 1/1; FULL suite — expect 340. Step 5: commit** `feat(cli): paperdoll appearance fields (race/gender/playerBytes decode)`.

---

### Task 2: Rust — `zam` asset proxy

**Files:** Modify `launcher/src-tauri/Cargo.toml` (add `reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls"] }`); create `launcher/src-tauri/src/zam.rs`; modify `launcher/src-tauri/src/lib.rs` (mod + registration).

- [ ] **Step 1: `zam.rs`** — the pure mapper + tests (complete), the handler (adapt signatures to the tauri 2.x version in Cargo.toml — read the existing `lib.rs` builder chain and tauri docs as needed):

```rust
use std::io::Read;
use std::sync::OnceLock;

/// Maps a zam-scheme request path to (upstream URL, cache-relative path).
/// Security: fixed host, prefix allowlist, no traversal, no embedded schemes.
pub fn zam_map_path(raw: &str) -> Option<(String, String)> {
    let path = raw.trim_start_matches('/');
    let path = path.split('?').next().unwrap_or("");
    if path.is_empty() || path.contains('\\') || path.contains("://") {
        return None;
    }
    if !(path.starts_with("modelviewer/") || path.starts_with("images/")) {
        return None;
    }
    if path.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return None;
    }
    Some((format!("https://wow.zamimg.com/{path}"), path.to_string()))
}

pub fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("reqwest client")
    })
}

/// Serve one request: cache hit -> bytes; else fetch (NO browser Origin
/// header -- that is the whole point), cache atomically, return. Any
/// failure -> None (handler answers 404).
pub fn zam_serve(cache_root: &std::path::Path, raw_path: &str) -> Option<(Vec<u8>, &'static str)> {
    let (url, rel) = zam_map_path(raw_path)?;
    let ct = content_type_for(&rel);
    let cached = cache_root.join("zam-cache").join(&rel);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Some((bytes, ct));
    }
    let resp = client().get(&url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    resp.take(64 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    if let Some(parent) = cached.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = cached.with_extension(format!(
        "{}.tmp",
        cached.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &cached);
    }
    Some((bytes, ct))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_allowlisted_paths() {
        let (url, rel) = zam_map_path("/modelviewer/wrath/viewer/viewer.min.js").unwrap();
        assert_eq!(url, "https://wow.zamimg.com/modelviewer/wrath/viewer/viewer.min.js");
        assert_eq!(rel, "modelviewer/wrath/viewer/viewer.min.js");
        assert!(zam_map_path("/images/wow/icons/large/inv_sword_39.jpg").is_some());
    }

    #[test]
    fn strips_query_strings() {
        let (url, _) = zam_map_path("/modelviewer/wrath/meta/armor/1/123.json?v=2").unwrap();
        assert!(!url.contains('?'));
    }

    #[test]
    fn rejects_bad_paths() {
        assert!(zam_map_path("/etc/passwd").is_none());
        assert!(zam_map_path("/modelviewer/../images/x.png").is_none());
        assert!(zam_map_path("/modelviewer//x").is_none());
        assert!(zam_map_path("/images/https://evil.example/x").is_none());
        assert!(zam_map_path("/modelviewer/wrath\\x").is_none());
        assert!(zam_map_path("/").is_none());
        assert!(zam_map_path("/other/x.js").is_none());
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type_for("a/b.js"), "application/javascript");
        assert_eq!(content_type_for("a/b.json"), "application/json");
        assert_eq!(content_type_for("a/b.mo3"), "application/octet-stream");
    }

    #[test]
    fn cache_hit_without_network() {
        let dir = std::env::temp_dir().join(format!("zamtest{}", std::process::id()));
        let rel = "modelviewer/wrath/meta/x.json";
        let f = dir.join("zam-cache").join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"{\"cached\":true}").unwrap();
        let (bytes, ct) = zam_serve(&dir, "/modelviewer/wrath/meta/x.json").unwrap();
        assert_eq!(bytes, b"{\"cached\":true}");
        assert_eq!(ct, "application/json");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: register in `lib.rs`** — `mod zam;` (module lives at `src/zam.rs`; note the existing crate layout — `dml` module is a dir, follow suit) and on the builder chain (before `.invoke_handler`):

```rust
        .register_asynchronous_uri_scheme_protocol("zam", |ctx, request, responder| {
            let cache = ctx
                .app_handle()
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let resp = match crate::zam::zam_serve(&cache, &path) {
                    Some((bytes, ct)) => tauri::http::Response::builder()
                        .status(200)
                        .header("content-type", ct)
                        .header("access-control-allow-origin", "*")
                        .body(bytes)
                        .unwrap(),
                    None => tauri::http::Response::builder()
                        .status(404)
                        .body(Vec::new())
                        .unwrap(),
                };
                responder.respond(resp);
            });
        })
```

(Exact closure signature/types: adapt to the tauri version — compile errors here mean signature drift, fix against the installed tauri's docs; `tauri::Manager` / `path()` imports as needed.)

- [ ] **Step 3: `cargo test`** — expect 23 (18 + 5). `npm run check` unaffected. **Step 4: commit** `feat(launcher): zam asset proxy — cached, allowlisted wowhead model assets` (stage Cargo.toml + Cargo.lock + zam.rs + lib.rs).

---

### Task 3: viewer adapter + CharacterModel component

**Files:** `npm i jquery` in launcher/ (runtime dep); create `launcher/src/lib/model-viewer.ts` + `launcher/src/lib/model-viewer.test.ts`; create `launcher/src/lib/CharacterModel.svelte`; modify `launcher/src/lib/pages/Dashboard.svelte` (mount beside the paperdoll) and `launcher/src/lib/api.ts` (`PaperdollData` gains `race: number; gender: number; skin: number; face: number; hair_style: number; hair_color: number; facial_style: number;`).

**Binding requirements** (the recon file is the API authority — read it FIRST):
- `model-viewer.ts` exports:
  - `AC_TO_VIEWER_SLOT: Record<number, number>` — ONLY the rendered AC slots (0,2,3,4,5,6,7,8,9,14,15,16,17,18) mapped per the recon's table; neck/rings/trinkets absent.
  - `buildViewerItems(equipped: {slot:number; displayid:number}[]): [number, number][]` — maps + filters (skips unmapped slots and displayid 0).
  - `buildCharacterModelId(race:number, gender:number): number` — the recon's formula verbatim.
  - `loadViewerScripts(): Promise<void>` — idempotent (module-level promise): assigns bundled jQuery to `window.$`/`window.jQuery` (import jquery), sets `window.WOTLK_TO_RETAIL_DISPLAY_ID_API = undefined` BEFORE the viewer script, then injects `<script src="http://zam.localhost/modelviewer/wrath/viewer/viewer.min.js">` and resolves when `window.ZamModelViewer` exists (reject on error/15s timeout).
  - `createCharacterViewer(containerId: string, doll: PaperdollData): Promise<unknown>` — per the recon's invocation: `type: 2`, `contentPath: "http://zam.localhost/modelviewer/wrath/"`, `container: $("#"+containerId)`, `aspect` per recon default, `models: {type: 16, id: buildCharacterModelId(...)}`, `items: buildViewerItems(...)`, plus character customization per the recon's mechanism WITH graceful fallback (customization failure → construct without it). Returns the viewer instance (for `destroy()` per the recon's teardown notes).
- `CharacterModel.svelte`: props `{ doll: PaperdollData }`; a fixed-size container (`width:300px; height:380px`, dark card styling matching the app); `$effect` on `doll.name`: destroy previous instance (guarded try/catch), then `loadViewerScripts().then(create...)`; loading state = muted `Loading model…`; ANY failure → muted `3D model unavailable (needs internet on first view)`; component NEVER throws (all awaits caught).
- Dashboard: render `<CharacterModel {doll} />` in a flex row with the paperdoll grid (model left, grid right; wraps on narrow widths). Everything else untouched.
- vitest `model-viewer.test.ts` (pure helpers only — no DOM): slot mapping pins (each rendered AC slot maps to the recon's viewer slot; 1/10/11/12/13 absent), buildViewerItems filters displayid 0 + unmapped slots, model-id formula matches the recon for a couple of (race,gender) pairs. ~5 tests → vitest 31.
- Gates: `npm test` (31) + `npm run check` (0/0) + `cargo test` unchanged. Commit `feat(launcher): 3D character model beside the paperdoll` (stage package.json, package-lock.json, the three new/changed src files, api.ts).
