# DML Launcher — Smoke Tests

Living checklist of everything not yet live-tested by hand. **Untested mutating features
are locked in the UI** (grey + "Untested" badge) until their row here goes green — flip
the lock in Settings ("Enable untested features") while actually testing. When a row
passes, tell Claude: the feature's flag flips to `tested` (in
`launcher/src/lib/features.ts`) and its Status here becomes ✅.

Feature keys in [brackets] match `features.ts`. Rows without a key are read-only
(never locked). Order batches server restarts.

## 0. Setup (once)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Dev install | `powershell -File cli\dev-install.ps1` succeeds; `dml version` prints. |
| ✅ | Launcher build | `npm run tauri dev` opens; sidebar shows all sections/pages; Home is the landing page. |

## 1. Home / server lifecycle

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Boot states (Round A) | With server stopped, open Home → "Server is stopped". Start → card flips to "Starting up…" (amber) during boot (~2 min warm; 10-20 min after a full Stop — cold DB + bot spawn), then "World is up" + players/uptime/latency. Buttons and card never contradict. (Passed 2026-07-18: full stopped→starting→World-is-up cycle user-confirmed.) |
| ⬜ | Health panel (Round A) | Click the server card → panel shows world/auth/DB rows ("Up … (healthy)"), version, uptime, players, latency, ports (game 8085 / auth 3724 / SOAP 7878 / DB — expect 13306), SOAP "reachable". |
| ✅ | [restart] Restart button (Round I) | Click Restart → streams stop+start into the terminal; card returns to "World is up" after boot. (Passed 2026-07-18 — surfaced+fixed two bugs first: dml-start.sh pipefail/grep -q readiness wait, false port-conflict warns on restart.) |
| ⬜ | soap_unreachable diagnostic (Round A) | Next time Docker networking breaks (or force by `sudo iptables`-breaking the forward): card shows "World is running, but the launcher can't reach it" + the restart-Docker hint. |
| ⬜ | Bots line (Round N) | With world up: online card shows `Bots: <n> / <max>` and the expanded health panel a "Bots online" row — numbers match reality (`server info` chars-in-world ≈ bots+you; max = the compose override, e.g. 2000). Playerbots page header shows the same bots-online chip. |

## 2. Console (Round B)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Log tail | Open Console with server running → worldserver log lines appear, auto-refresh follows, no ANSI garbage. Scroll up → autoscroll pauses; scroll down → resumes. (Passed 2026-07-18 user-confirmed.) |
| ✅ | [console-send] Send command | Send `server info` → reply appears in history with real stats. Send `bogus` → the fault text shows inline, not a crash. (Passed 2026-07-18: real stats + inline fault user-confirmed.) |
| ⬜ | Stopped-server state | Stop the server → Console shows "No server logs — is the server installed?" (or stale tail) without erroring. |
| ⬜ | Console persistence (Round N) | Start a stream (e.g. a module conf activate or any action with a terminal) on one page, hop to another page and back → the transcript is intact and still streaming. Console page: send a command, leave, return → command history still there. |
| ⬜ | Clear buttons (Round N) | Terminal Clear (greyed while running) empties + hides the panel; Console Clear empties command history (log tail refills on next poll — expected). |
| ⬜ | Download log (Round N) | Terminal/Console Download opens a native save dialog; the saved file contains the transcript (sections as `== name ==` blocks); cancel does nothing. |
| ⬜ | Console fill (Round N) | Console page: the log fills the free window height, the page itself never scrolls; only log/history scroll internally. Other pages: starting a run auto-scrolls the terminal into view; it grows tall (viewport-capped) instead of forcing page scrolling. |

## 3. Library / titles (Round D)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [title-install] Install a title | Install MapleStory (smallest). Prompts appear in the install terminal; answer them in the input row; installer completes; title appears in Installed. |
| ⬜ | [title-install] **Cancel kills the distro process?** | Start an install, click Cancel (confirm). **FIRST CHECK:** in the distro run `top` / `docker ps` — did the installer bash/docker actually die? If it survives, report it (guest-side kill is the planned fix). UI must recover (buttons re-enable). |
| ⬜ | [title-install] Retry same title | After a failed/cancelled install, click Install on the SAME title again → the terminal reopens and runs (regression: used to soft-lock the page). |
| ⬜ | [title-remove] Remove a title | Remove the test title (typed-id confirm) → server dir + symlink + launcher script gone; `~/.dml` backups untouched. |
| ⬜ | Install session survives nav (Round N) | During a RUNNING install (ideally while it waits at an interactive prompt), hop to another page and back to Library → the panel re-shows with the full transcript, the reply input still works (answer the prompt), and Cancel still works. After an install finishes while you're away → returning shows the finished transcript with correct ok/err styling. (Known cosmetic quirk: a resumed panel may not auto-scroll to new output.) |

## 4. Dashboard / character view (Rounds E, F, G)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Paperdoll + tooltips (E) | Show gear for a real char → slot grid with icons; hover a standard item → wowhead-style tooltip; hover a CUSTOM item → local tooltip (name/ilvl/stats). Second view of the same char is instant. |
| ✅ | 3D model (F) | Model renders in the middle of the gear window, correct SEX and race, wearing renderable gear. Rotate/zoom works. Second view fast. (Passed 2026-07-19 after rebuilding on Wowhead's NEW engine — live-tree viewer + native wrath m2 data; the old mo3 format was retired upstream. Also fixed en route: WH env stub, error surfacing, in-window placement.) |
| ⬜ | **3D model with a custom-displayid item (F1)** | View a char wearing a custom/GM item → model must still render (unrenderable items skipped via the probe fallback; naked model as last resort). |
| ⬜ | Talents card (G) | Talent icons fill in (chunk-at-a-time on first view — batches of 25, NOT broken); hover a maxed multi-rank talent → correct rank tooltip; Dual spec badge on a dual-spec char; only active-spec talents. |
| ⬜ | Achievements card (G) | Total + 10 recent with icons/names/dates; hover → achievement tooltip. |

## 5. Teleport (rounds 1-5 + I)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [teleport-named] Named teleport | Teleport an online char to a named location (two-step confirm) → char moves in-game. |
| ⬜ | [teleport-coords] Coords teleport (I) | With the char LOGGED OUT: Coordinates… → map/x/y/z → confirm ("Overwrite …'s saved position?") → log in → char is there. With the char ONLINE → the CHAR_ONLINE error shows inline. |

## 6. GM Tools (rounds 2-3 + I)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [gm-actions] Level / gold / heal / revive | Online char: gold+heal+revive apply in-game. Offline char: set level → shows on next login. |
| ⬜ | [gm-summon] Summon NPCs | Summon Banker → appears 2yd in front, despawns ~5 min. Summon Casino (990000) → Gasino appears. Bogus entry → clean NOT_FOUND. |
| ⬜ | [gm-atlogin] At-login flags (I) | Apply Rename to a char (confirm) → next login prompts a rename. Spot-check Customize. **Verify the SOAP command is accepted at all (no leading dot) — if it faults, report.** |

## 7. Item Database (rounds 1-5)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Item search | Search "hearthstone" → results with quality colors. |
| ⬜ | [mail-item] Mail items | Mail an item to a char → arrives in-game mailbox. |

## 7b. Commands page (Round M)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Commands reference | With mods installed (e.g. transmog, ahbot) → Commands page lists a card per installed mod with its command reference. With none installed → empty state "No installed mods with commands yet — install mods on the Modules page." |

## 8. Playerbots / My Party (rounds 4 + I)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [party-ops] Add / kick / relogin | Add a bot by class → joins the party in-game; kick removes; relogin cycles it. |
| ⬜ | [party-botcmd] Gear up / Fix talents / Maintain | Buttons whisper the bot; gear/talents visibly change. |
| ⬜ | [bot-level] Set bot level (I) | Set a bot's level → applies (relogin if offline). |
| ⬜ | [party-presets] Presets save/load | Save current party as preset; kick all; load → party rebuilt (replace semantics). |
| ⬜ | [preset-io] Export/import (I) | Export a preset (copy text), delete it, Import with the same name+classes → identical; import over an existing name → overwrite confirm fires. |

## 9. Settings / Module Configs (rounds 1-5 + I)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [settings-save] Curated settings | Change XP rate → save → restart banner → restart → rate active in-game. Motd change applies live (no restart). |
| ⬜ | [config-edit] Raw conf editor | Edit playerbots.conf, save; `.env` and compose override open READ-ONLY (no Save button). Settings↔Module Configs hop keeps unsaved edits. |
| ⬜ | [ale-reload] Reload ALE scripts (I) | Click → reply text appears (note: if mod-ale ISN'T loaded the reply may still show as a success note — eyeball it). |

## 10. Modules (Round C + J)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [modules-cpp] C++ install + banner | Install mod-aoe-loot → "rebuild required" banner lists it. |
| ⬜ | [modules-rebuild] REAL rebuild | Rebuild (backup checkbox ON) → streams 30-90 min build → success clears the banner → in-game AoE loot works. |
| ⬜ | [modules-conf] Conf activate | After rebuild, Activate conf on the module → file appears in env/dist/etc/modules. |
| ⬜ | [modules-lua] ALE script install | Install bmah or paragon (exercises SQL + client copies) → `.reload ale`/restart per hint → works in-game; client files landed in the client folder. |
| ⬜ | [modules-sql] SQL mods | Install a tweak (buff-mobs) → mobs visibly buffed; install nerf-mobs → replaces it. Install+remove portals-capitals; install a hearthstone-cd variant → cooldown actually matches the chosen variant. |
| ⬜ | [client-path] Client folder | Detect finds the client; Save validates; bad folder → NOT_CLIENT error. |
| ⬜ | [module-repair] Repair panel (J) | Break tracking on purpose (Clear a row for an installed module) → restart → db-import re-applies (or fails per SQL type) → use Mark to fix → server starts clean. |
| ⬜ | [docker-clean] Usage (K) | Open Modules → Disk cleanup card's usage `<pre>` shows real Docker disk sizes (not empty/error). |
| ⬜ | [docker-clean] Level-1 clean (K) | Clean (level 1, two-step confirm) → streams cleanup, reports reclaimed space. Rebuild afterwards succeeds (30-90 min full recompile expected). |
| ⬜ | [server-update] Check for updates (L) | Server update card → Check for updates → repo rows show real state for AzerothCore + mod-playerbots (branch `Playerbot`, short sha, correct behind count). |
| ⬜ | [server-update] Update (L) | On a clean tree: Update (backup checkbox ON, two-step confirm) → either pulls (rebuild banner gains `core-update`, note "Update pulled — rebuild required") or reports "Already up to date." if nothing to pull. If pulled, a subsequent rebuild compiles successfully. |

## 11. Backups (round 5 + C)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [backup-create] Create (+world) | Back up now → file listed. With "Include world data" → file marked "includes world". |
| ⬜ | [backup-restore] Restore round-trip | Restore a backup → server stops, safety dump appears, restore, server starts, chars intact. |

## 12. Accounts (Round H)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [accounts] Create + login | Create an account in the launcher → **log into the game client with it**. Duplicate name → "already exist" inline. Set GM 1 → badge shows; GM 3 asks the SOAP warning confirm. |

## 13. Bridges (rounds 2-4)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [bridge-setup] Deploy server bridges | GM Tools → Deploy server bridges → restart → GM/party/summon features work (they depend on these Lua bridges). Do this BEFORE sections 6/8. |

## Known caveats (not tests — expectations)

- ARAC (C++ module): installs but its client-side DBC/MPQ patching is NOT ported yet — don't judge it broken, it's a known gap.
- First view of a high-talent char fills talent icons in batches (~10-15s per 25) — not a hang.
- Full backups share the keep-10 pool; restoring an older full backup while a module is installed re-applies that module's SQL at next start.
- Cold Start after a full Stop: the world can crash-retry for ~2 min while MySQL warms up (Docker self-heals — normal). Rarely, the world then wedges mid-load (log frozen + 0% CPU for 3+ min): that's a hang, not slow loading — click Restart to clear it (observed once, 2026-07-18).
- A cold Start (full Stop first) also re-runs `ac-db-import`, which applies any pending AzerothCore database migrations — the schema can change under features that query it (this broke the character view once, 2026-07-19; fixed schema-adaptively). Restart never does this. If a DB-reading page errors right after a cold start, suspect a migration and report it.
