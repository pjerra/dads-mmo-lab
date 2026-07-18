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
| ⬜ | Dev install | `powershell -File cli\dev-install.ps1` succeeds; `dml version` prints. |
| ⬜ | Launcher build | `npm run tauri dev` opens; sidebar shows all sections/pages; Home is the landing page. |

## 1. Home / server lifecycle

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Boot states (Round A) | With server stopped, open Home → "Server is stopped". Start → card flips to "Starting up…" (amber) during the ~2-min boot, then "World is up" + players/uptime/latency. Buttons and card never contradict. |
| ⬜ | Health panel (Round A) | Click the server card → panel shows world/auth/DB rows ("Up … (healthy)"), version, uptime, players, latency, ports (game 8085 / auth 3724 / SOAP 7878 / DB — expect 13306), SOAP "reachable". |
| ⬜ | [restart] Restart button (Round I) | Click Restart → streams stop+start into the terminal; card returns to "World is up" after boot. |
| ⬜ | soap_unreachable diagnostic (Round A) | Next time Docker networking breaks (or force by `sudo iptables`-breaking the forward): card shows "World is running, but the launcher can't reach it" + the restart-Docker hint. |

## 2. Console (Round B)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Log tail | Open Console with server running → worldserver log lines appear, auto-refresh follows, no ANSI garbage. Scroll up → autoscroll pauses; scroll down → resumes. |
| ⬜ | [console-send] Send command | Send `server info` → reply appears in history with real stats. Send `bogus` → the fault text shows inline, not a crash. |
| ⬜ | Stopped-server state | Stop the server → Console shows "No server logs — is the server installed?" (or stale tail) without erroring. |

## 3. Library / titles (Round D)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [title-install] Install a title | Install MapleStory (smallest). Prompts appear in the install terminal; answer them in the input row; installer completes; title appears in Installed. |
| ⬜ | [title-install] **Cancel kills the distro process?** | Start an install, click Cancel (confirm). **FIRST CHECK:** in the distro run `top` / `docker ps` — did the installer bash/docker actually die? If it survives, report it (guest-side kill is the planned fix). UI must recover (buttons re-enable). |
| ⬜ | [title-install] Retry same title | After a failed/cancelled install, click Install on the SAME title again → the terminal reopens and runs (regression: used to soft-lock the page). |
| ⬜ | [title-remove] Remove a title | Remove the test title (typed-id confirm) → server dir + symlink + launcher script gone; `~/.dml` backups untouched. |

## 4. Dashboard / character view (Rounds E, F, G)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Paperdoll + tooltips (E) | Show gear for a real char → slot grid with icons; hover a standard item → wowhead-style tooltip; hover a CUSTOM item → local tooltip (name/ilvl/stats). Second view of the same char is instant. |
| ⬜ | 3D model (F) | Model renders beside the paperdoll wearing the right gear, correct SEX and race. Rotate/zoom works. Second view fast/offline. |
| ⬜ | **3D model with a custom-displayid item (F1)** | View a char wearing a custom item → model must still render (item skipped) — if the whole model dies, report it (fix is pre-planned). |
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
