# 3D Character Model — Design Spec (Round F)

**Date:** 2026-07-18
**Branch:** `feat/dml-launcher-windows`
**Status:** Design review waived (standing user instruction). Deep recon in `.superpowers/sdd/recon-modelviewer.md` (read it before planning/implementing — it carries the verbatim ZamModelViewer invocation, slot mapping, meta paths, CORS probe results, and the ISC license text).

## Answer to the user's question

Yes: Wowhead's ZamModelViewer has a **wrath** environment that renders WotLK character
models and accepts **native WotLK display ids** (verified — no retail-conversion service
needed). We can dress the model from data we already have: `characters.race/gender/
playerBytes/playerBytes2` + each equipped item's `displayid` (already in the paperdoll
query).

## The one hard constraint (drives the architecture)

`wow.zamimg.com/modelviewer/*` enforces a server-side **Origin allowlist** — any browser
Origin that isn't `*.wowhead.com` gets 403 (verified for every Tauri origin). Requests
**without** a browser Origin succeed. Therefore the webview cannot fetch viewer assets
directly; the launcher proxies them:

**Tauri custom URI scheme `zam`** (`register_uri_scheme_protocol`): the page requests
`http://zam.localhost/<path>` (WebView2's form of the scheme); the Rust handler validates
the path, serves from a **disk cache**, else fetches `https://wow.zamimg.com/<path>` via
reqwest (no Origin header → 200), caches, returns. Every viewer URL — `viewer.min.js`,
meta JSONs, models, textures — flows through the proxy via `CONTENT_PATH`, which also
makes repeat views work offline.

**SSRF/scope hard rules:** the handler proxies ONLY to the fixed host
`https://wow.zamimg.com`, ONLY paths starting `modelviewer/` or `images/`, path cleaned
of `..` segments and query strings ignored for the cache key; anything else → 404. Cache
lives under the app cache dir (`launcher-zam-cache/<path>`), written atomically
(`.tmp` + rename), content-type derived from extension.

## Components

1. **CLI — paperdoll appearance fields** (additive): the `paperdoll` query also selects
   `c.race, c.gender, c.playerBytes, c.playerBytes2` and emits decoded fields:
   `race, gender, skin = bytes&0xFF, face = (bytes>>8)&0xFF, hair_style = (bytes>>16)&0xFF,
   hair_color = (bytes>>24)&0xFF, facial_style = bytes2&0xFF`. Existing fields untouched
   (back-compat; bats pins the decode with a crafted playerBytes value).
2. **Rust** — the `zam` scheme handler + `reqwest` (blocking, rustls-tls) dependency;
   pure helper `zam_map_path(uri) -> Option<(url, cache_rel)>` unit-tested for the
   allowlist/traversal rules (cargo tests, no network). PaperdollData additions flow
   through untyped JSON (no Rust change needed for them).
3. **UI** — `launcher/src/lib/model-viewer.ts`: a small **vendored adapter** (ported from
   the ISC-licensed `wow-model-viewer` wrapper per the recon, NOT the npm package —
   we need wrath contentPath + zam proxy + no `WOTLK_TO_RETAIL_DISPLAY_ID_API`):
   loads jQuery (npm dependency, assigned to `window.$`/`window.jQuery`) and
   `viewer.min.js` (script tag pointing at the proxy) once, then constructs
   `ZamModelViewer` with `type:2`, `contentPath: "http://zam.localhost/modelviewer/wrath/"`,
   `models: {type:16, id: race*2-1+gender... exactly per the recon's verbatim formula}`,
   `items: [[viewerSlot, displayid]...]` using the recon's slot mapping (AC inventory
   slot → viewer slot; rings/trinkets/neck are not rendered — skip them), and character
   customization per the recon's mechanism, **degrading gracefully**: if the
   customization meta can't be mapped, render the base model with items anyway.
   - `CharacterModel.svelte` on Dashboard beside the paperdoll grid: ~280×360 canvas
     container, loads after the paperdoll, re-renders on character switch (destroy old
     viewer instance per the recon's teardown notes), shows a muted
     `3D model unavailable (needs internet on first view)` fallback on any load error —
     the paperdoll/tooltips NEVER break because of the model.
4. **Item filter:** only slots the model renders (head 0, shoulders 2, shirt 3, chest 4,
   waist 5, legs 6, feet 7, wrists 8, hands 9, back 14, mainhand 15, offhand 16,
   ranged 17, tabard 18 — final say per the recon's mapping table).

## Testing

- bats: appearance-field decode (crafted playerBytes → expected skin/face/hair values),
  back-compat of existing paperdoll fields.
- cargo: `zam_map_path` allowlist matrix (good modelviewer/images paths; rejected: other
  hosts embedded in path, `..` traversal, absolute URLs, empty, non-allowlisted prefixes).
- vitest: adapter pure helpers (AC-slot→viewer-slot mapping incl. skipped slots;
  character-options builder from paperdoll fields).
- check/vitest/cargo/bats baselines stay green (entering F: bats 339, vitest 26, cargo 18).
- **Live gate (batched):** the model actually renders in the app (WebGL in WebView2 +
  the proxied asset chain can only be fully proven live), gear visibly matches, second
  view offline-fast, custom-displayid items degrade without breaking the viewer.

## Out of scope

Animations/poses UI, undress/dress-item toggles, race-change previews, retail env,
mounts/pets, screenshots. If zamimg ever changes its wrath tree, the cache keeps
working for already-viewed assets.
