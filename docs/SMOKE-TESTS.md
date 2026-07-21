# DML Launcher — Smoke Tests

Living checklist of everything not yet live-tested by hand. **Untested mutating features
are locked in the UI** (grey + "Untested" badge) until their row here goes green — flip
the lock in Settings ("Enable untested features") while actually testing. When a row
passes, tell Claude: the feature's flag flips to `tested` (in
`launcher/src/lib/features.ts`) and its Status here becomes ✅.

Feature keys in [brackets] match `features.ts`. Rows without a key are read-only
(never locked). Order batches server restarts.

**Sidebar layout note (dropdown sidebar, 2026-07-20):** the sidebar is a
collapsible menu — click a group (**Server · Characters · Items & Bots ·
Config**) to expand it; the group for the page you're on opens automatically.
What used to be in-page tabs are now items inside those groups, so the steps
below write a sidebar path: **Config ▸ Bot World**, **Items & Bots ▸ My Party**,
etc. Tabs survive in exactly ONE place — the character view
(**Characters ▸ Character**), which keeps its Character / Talents /
Achievements tabs. NB **Server ▸ Modules** (installing modules) is a different
thing from **Config ▸ Module files** (editing conf files).

## 0. Setup (once)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Dev install | `powershell -File cli\dev-install.ps1` succeeds; `dml version` prints. |
| ✅ | Launcher build | `npm run tauri dev` opens; sidebar shows all sections/pages; Home is the landing page. |

## 1. Home / server lifecycle

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Boot states (Round A) | With server stopped, open Home → "Server is stopped". Start → card flips to "Starting up…" (amber) during boot (~2 min warm; 10-20 min after a full Stop — cold DB + bot spawn), then "World is up" + players/uptime/latency. Buttons and card never contradict. (Passed 2026-07-18: full stopped→starting→World-is-up cycle user-confirmed.) |
| ✅ | Health panel (Round A) | Click the server card → panel shows world/auth/DB rows ("Up … (healthy)"), version, uptime, players, latency, ports (game 8085 / auth 3724 / SOAP 7878 / DB — expect 13306), SOAP "reachable". |
| ✅ | [restart] Restart button (Round I) | Click Restart → streams stop+start into the terminal; card returns to "World is up" after boot. (Passed 2026-07-18 — surfaced+fixed two bugs first: dml-start.sh pipefail/grep -q readiness wait, false port-conflict warns on restart.) |
| ⬜ | soap_unreachable diagnostic (Round A) | Next time Docker networking breaks (or force by `sudo iptables`-breaking the forward): card shows "World is running, but the launcher can't reach it" + the restart-Docker hint. |
| ✅ | Bots line (Round N) | With world up: online card shows `Bots: <n> / <max>` and the expanded health panel a "Bots online" row — numbers match reality (`server info` chars-in-world ≈ bots+you; max = the compose override, e.g. 2000). Items & Bots ▸ My Party header shows the same bots-online chip. |

## 2. Console (Round B)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Log tail | Open Console with server running → worldserver log lines appear, auto-refresh follows, no ANSI garbage. Scroll up → autoscroll pauses; scroll down → resumes. (Passed 2026-07-18 user-confirmed.) |
| ✅ | [console-send] Send command | Send `server info` → reply appears in history with real stats. Send `bogus` → the fault text shows inline, not a crash. (Passed 2026-07-18: real stats + inline fault user-confirmed.) |
| ✅ | Stopped-server state | Stop the server → Console shows a calm offline note without erroring. (Passed 2026-07-19; message reworded in Round Q to say the server looks stopped rather than questioning the install.) |
| ✅ | Console persistence (Round N) | Start a stream (e.g. a module conf activate or any action with a terminal) on one page, hop to another page and back → the transcript is intact and still streaming. Console page: send a command, leave, return → command history still there. |
| ✅ | Clear buttons (Round N) | Terminal Clear (greyed while running) empties + hides the panel; Console Clear empties the LOG VIEW + history — only lines arriving after the clear render (fixed 2026-07-19: was history-only, looked like a no-op; user-confirmed after fix). |
| ✅ | Download log (Round N) | Terminal/Console Download opens a native save dialog; the saved file contains the transcript (sections as `== name ==` blocks); cancel does nothing. (Passed 2026-07-19.) |
| ✅ | Console fill (Round N) | Console page: the log fills the free window height, the page itself never scrolls; only log/history scroll internally. Other pages: starting a run auto-scrolls the terminal into view; it grows tall (viewport-capped) instead of forcing page scrolling. |

## 3. Library / titles (Round D)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [title-install] Install a title | Install MapleStory (smallest). Prompts appear in the install terminal; answer them in the input row; installer completes; title appears in Installed. |
| ⬜ | [title-install] **Cancel kills the distro process?** | Start an install, click Cancel (confirm). **FIRST CHECK:** in the distro run `top` / `docker ps` — did the installer bash/docker actually die? If it survives, report it (guest-side kill is the planned fix). UI must recover (buttons re-enable). |
| ⬜ | [title-install] Retry same title | After a failed/cancelled install, click Install on the SAME title again → the terminal reopens and runs (regression: used to soft-lock the page). |
| ⬜ | [title-remove] Remove a title | Remove the test title (typed-id confirm) → server dir + symlink + launcher script gone; `~/.dml` backups untouched. |
| ⬜ | Install session survives nav (Round N) | During a RUNNING install (ideally while it waits at an interactive prompt), hop to another page and back to Library → the panel re-shows with the full transcript, the reply input still works (answer the prompt), and Cancel still works. After an install finishes while you're away → returning shows the finished transcript with correct ok/err styling. (Known cosmetic quirk: a resumed panel may not auto-scroll to new output.) |

## 4. Character view (Rounds E, F, G) — sidebar: Characters ▸ Character

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Paperdoll + tooltips (E) | Load a real char (gear now auto-loads on selection; **Reload gear** re-fetches) → slot grid with icons; hover a standard item → wowhead-style tooltip; hover a CUSTOM item → local tooltip (name/ilvl/stats). Second view of the same char is instant. |
| ✅ | 3D model (F) | Model renders in the middle of the gear window, correct SEX and race, wearing renderable gear. Rotate/zoom works. Second view fast. (Passed 2026-07-19 after rebuilding on Wowhead's NEW engine — live-tree viewer + native wrath m2 data; the old mo3 format was retired upstream. Also fixed en route: WH env stub, error surfacing, in-window placement.) |
| ✅ | **3D model with a custom-displayid item (F1)** | View a char wearing a custom/GM item → model must still render (unrenderable items skipped via the probe fallback; naked model as last resort). (Passed 2026-07-19: GM char renders, admin items shown where the CDN has them, Martin's Fury skipped as designed.) |
| ✅ | Talents card (G + Round O trees) | Three in-game-style tree panels side by side (name + points per tree, e.g. "Fury (52)"; summary "NN points — X/Y/Z"): learned talents lit with icons + rank badges (green partial / gold maxed), unlearned as dark empty slots at the right grid positions. Icons fill chunk-at-a-time on first view (NOT broken); hover a maxed multi-rank talent → correct rank tooltip; Dual spec badge on a dual-spec char; only active-spec talents. Check the rank badges don't overlap neighbors. |
| ✅ | Achievements browser (G + Round P) | Achievements TAB: header shows "N points · X of 1320"; category rail (9 roots + indented children) selects scopes; earned rows lit with icons + dates, unearned dimmed; hover → achievement tooltip. First click on a big category (Dungeons & Raids = 460) streams icons in chunks (~19 batches — expected, cached after). A fetch failure shows a red note (not silently-all-unearned). |
| ✅ | Character tabs (Round P) | After loading gear: Character / Talents / Achievements tab strip; switching is instant (no refetch); loading a NEW character lands back on Character. (Passed 2026-07-19 user visual.) |

## 5. Teleport (rounds 1-5 + I)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | [teleport-named] Named teleport | Teleport an online char to a named location (two-step confirm) → char moves in-game. |
| ✅ | [teleport-coords] Coords teleport (I) | With the char LOGGED OUT: Coordinates… → map/x/y/z → confirm ("Overwrite …'s saved position?") → log in → char is there. With the char ONLINE → the CHAR_ONLINE error shows inline. |

## 6. GM Tools (rounds 2-3 + I)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | [gm-actions] Level / gold / heal / revive | Online char: gold+heal+revive apply in-game. Offline char: set level → shows on next login. |
| ✅ | [gm-summon] Summon NPCs | Summon Banker → appears 2yd in front, despawns ~5 min. Summon Casino (990000) → Gasino appears. Bogus entry → clean NOT_FOUND. |
| ✅ | [gm-atlogin] At-login flags (I) | Apply Rename to a char (confirm) → next login prompts a rename. Spot-check Customize. **Verify the SOAP command is accepted at all (no leading dot) — if it faults, report.** |

## 7. Item Database (rounds 1-5)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Item search | Search "hearthstone" → results with quality colors. |
| ✅ | [mail-item] Mail items | Mail an item to a char → arrives in-game mailbox. |

## 7b. Commands page (Round M)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Commands reference | With mods installed (e.g. transmog, ahbot) → Commands page lists a card per installed mod with its command reference. With none installed → empty state "No installed mods with commands yet — install mods on the Modules page." |

## 8. Playerbots / My Party (rounds 4 + I) — sidebar: Items & Bots ▸ My Party

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | [party-ops] Add / kick / relogin | Add a bot by class → joins the party in-game; kick removes; relogin cycles it. |
| ✅ | [party-botcmd] Gear up / Fix talents / Maintain | Buttons whisper the bot; gear/talents visibly change. |
| ✅ | [bot-level] Set bot level (I) | Set a bot's level → applies (relogin if offline). |
| ✅ | [party-presets] Presets save/load | Save current party as preset; kick all; load → party rebuilt (replace semantics). |
| ✅ | [preset-io] Export/import (I) | Export a preset (copy text), delete it, Import with the same name+classes → identical; import over an existing name → overwrite confirm fires. |

## 9. Settings / Module Configs (rounds 1-5 + I) — sidebar: Config ▸ Settings + Config ▸ Module files

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | [settings-save] Curated settings | Change XP rate → save → restart banner → restart → rate active in-game. Motd change applies live (no restart). |
| ⬜ | [config-edit] Raw conf editor | Edit playerbots.conf, save; `.env` and compose override open READ-ONLY (no Save button). Config ▸ Settings ↔ Config ▸ Module files hop keeps unsaved edits. |
| ⬜ | [ale-reload] Reload ALE scripts (I) | Click → reply text appears (note: if mod-ale ISN'T loaded the reply may still show as a success note — eyeball it). |

## 10. Modules (Round C + J)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Module info + GitHub links (Round R, read-only) | Every registry module shows a one-line description under its name; modules with a repo show a "GitHub ↗" link that opens the project page in the browser (tweak-type SQL mods have no link — expected). |
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
| ✅ | [backup-create] Create (+world) | (Passed 2026-07-21: created a real 24 MB backup on disk.) Back up now → file listed. With "Include world data" → file marked "includes world". |
| ⬜ | [backup-restore] Restore round-trip | Restore a backup → server stops, safety dump appears, restore, server starts, chars intact. |

## 12. Accounts (Round H)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | [accounts] Create + login | (Backend-verified 2026-07-21 via CLI+DB: create + set-GM landed in acore_auth; the in-game login is still a nice manual confirm.) Create an account in the launcher → **log into the game client with it**. Duplicate name → "already exist" inline. Set GM 1 → badge shows; GM 3 asks the SOAP warning confirm. |
| ✅ | [account-delete] Delete an account (Round Q+) | (Passed 2026-07-21 via CLI+DB: throwaway deleted from acore_auth; admin-delete refused.) Delete a THROWAWAY account (typed-name confirm) → gone from the list and its characters gone from the DB; the admin account shows NO delete button; deleting a nonexistent name → inline SOAP fault. |

## 13. Bridges (rounds 2-4)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | [bridge-setup] Deploy server bridges | GM Tools → Deploy server bridges → restart → GM/party/summon features work (they depend on these Lua bridges). Do this BEFORE sections 6/8. |

## 14. Tools (Round Q)

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [lan-play] Status | Tools → LAN play card shows the current address on mount and via Refresh. |
| ⬜ | [lan-play] Enable | Fill in this PC's LAN IP → Enable (two-step confirm) → another device on the LAN can reach the realm at that address (or at minimum status reflects the LAN IP — full two-PC check optional). |
| ⬜ | [lan-play] Disable | Disable (two-step confirm) → status reflects localhost again. |
| ⬜ | [unbound-addon] Install / Update | Install (two-step is the interactive session itself) → wizard runs to completion + force-rebuild (30-90 min) → addon active in-game. LONG — batch with the Modules rebuild tests while sitting. |
| ⬜ | [unbound-addon] Uninstall | Uninstall → typed "unbound" confirm → wizard prompts run to completion, tables dropped + rebuild (30-90 min) → addon gone. |
| ⬜ | Doctor | Run → all checks report (Docker, disk, network, WSL). |
| ⬜ | Shell | Open shell → a Windows terminal opens inside the dml-arch distro. |

## 15. Batch 1 — config cluster

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | [rates-live] Rates rows live-apply | (Backend-verified 2026-07-21: Honor→2 wrote worldserver.conf + `applied:"live"`, reverted; the in-game rate-took-effect check is still yours.) Server RUNNING: Config ▸ Settings → change Honor gains (or any new Rates/Cross-faction row) → Save → green "Applied live ✓" note (no restart banner); verify in-game (kill a mob / check honor) that the rate changed WITHOUT a restart. `env/dist/etc/worldserver.conf` on the host shows the new `Rate.… = value` line. |
| ⬜ | [rates-live] Legacy env migration | With an old save still in docker-compose.override.yml (e.g. AC_RATE_XP_KILL): change XP from kills → Save → the AMBER restart banner shows (not "applied live"), the AC_… key is GONE from override.yml, and after one restart the conf value is what the server uses. |
| ⬜ | Server stopped: rates save falls back to restart | With the server stopped, save a Rates row → banner says restart needed (SOAP unreachable is not an error); value shows after the next start. |
| ✅ | [bots-world] Curated Bot World save | (Backend-verified 2026-07-21: bots.per_interval 60→61 wrote playerbots.conf, applied:"restart", reverted.) Config ▸ Bot World → change e.g. "Bots chat" or bot population → Save → restart banner → Restart → the change is live (population count / chat behavior). playerbots.conf on the host shows the key. |
| ⬜ | Bot World browser (read) | Config ▸ Bot World → the all-keys list loads (~hundreds of keys); search "broadcast" filters; hovering a key shows its default. Browsing works while LOCKED (only saving is gated). |
| ⬜ | [bots-world] Browser staged save | Edit 2-3 keys in the browser (e.g. BroadcastChance…) → "Save N changes" → restart banner; keys land in playerbots.conf verbatim. |
| ⬜ | Module Configs dynamic list | Config ▸ Module files → the file picker lists worldserver.conf, authserver.conf and EVERY installed module conf (incl. transmog.conf, which has no .dist) — not just the old 5. A dist-only conf shows "(new — starts from defaults)" and opens read of its dist. |
| ⬜ | [config-edit] Edit worldserver.conf raw | Config ▸ Module files → open worldserver.conf → edit a harmless value → Save → .bak kept → restart applies it. (.env/override still read-only.) |
| ⬜ | [config-reset] Reset to defaults | Config ▸ Module files → open playerbots.conf → Reset to defaults (two-step confirm) → file equals its .dist, previous version kept as .bak, restart banner shows. |
| ⬜ | [bots-flush] Flush & rebuild bots | Config ▸ Bot World → Danger zone → type "flush" → button runs: terminal streams backup → delete-flag restart → restore → rebuild restart → done. Takes MINUTES (two boots, bot recreation). After: bot count rebuilds to the configured population, YOUR characters untouched, `AiPlayerbot.DeleteRandomBotAccounts = 0` in playerbots.conf, a new backup in Backups. |
| ⬜ | [bots-flush] Flush abort safety | (Optional, pairs with the row above) If the flush errors mid-run (e.g. stop the distro's docker): the error shows in the terminal AND playerbots.conf still ends with `AiPlayerbot.DeleteRandomBotAccounts = 0` — a later manual start must NOT wipe bots again. |

## 16. Batch 2 — Windows integration

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [auto-shutdown] Stop when WoW closes | Server running: Tools → enable "Stop the server when WoW closes" → status says "Waiting for WoW…". Launch Wow.exe → status flips to "Armed". Close WoW → within ~15s the server stops gracefully (chip → Stopped) and the card shows "Server stopped automatically because WoW was closed." Toggle survives a launcher restart (re-arms on app start). |
| ⬜ | [auto-shutdown] Never fires when server already down | With the toggle on and the server STOPPED: launch + close WoW → nothing stops/breaks; card notes there was nothing to stop. |
| ⬜ | [keep-awake] Sleep block while online | With the toggle on (Tools → LAN play card): start the server → "keeping PC awake" appears under the sidebar chip and `powercfg /requests` (admin prompt) lists the launcher under SYSTEM. Stop the server → hint disappears and powercfg no longer lists it. Quit the launcher while online → the request is released (powercfg again). |
| ⬜ | [lan-auto-refresh] LAN re-point after start | LAN play ON with a LAN address in the realm DB: restart the server → after boot, `Tools → LAN play → Refresh` (or the DB) shows the realm address is this PC's current IP without clicking anything. Green toast under the chip ONLY if the address actually changed (fake it: `dml lan wow-server-playerbots on <other-private-ip>` first, then restart). |
| ✅ | Realmlist status (read-only) | (Passed 2026-07-21 user-confirmed: green "points at your server" while playing.) Tools → Game realmlist: with the client path set, the card reads the real `Data\<locale>\realmlist.wtf` — green "Points at your server" when it (or the game's Config.wtf fallback) says 127.0.0.1/LAN IP, yellow with the actual address otherwise, grey "no client path" hint when unset. |
| ⬜ | [realmlist-fix] One-click fix | Edit realmlist.wtf to a bogus host (e.g. `set realmlist logon.example.com`) → card goes yellow → "Point at this PC" → file now contains `set realmlist 127.0.0.1`, card green. With LAN play ON, the LAN button writes the LAN IP instead. Game logs in fine afterwards. |
| ⬜ | [realmlist-fix] Protect toggle | Enable "Protect the file" → file becomes read-only (check in Explorer → Properties) and OTHER tools can't overwrite it; the card's own Fix still works (clears + restores the flag). Untick → read-only attribute gone. |
| ⬜ | Crashed verdict (read-only) | With the server running: kill the world hard (`wsl -d dml-arch -u dml -- docker kill ac-worldserver`) → within ~7s chip + Home card show "Server crashed" (pulsing red, distinct from a normal Stop) with the exit code and a Recover button; Recover = normal Start, world comes back. A normal Stop must still read plain "Stopped". |
| ⬜ | [chip-start] Chip ▶ quick-start | With the server stopped (or crashed), a small green ▶ shows next to the sidebar chip on EVERY page → click → lands on Home with the start streaming into Home's terminal. No stop button on the chip, ever. |

## 17. Batch 3 — pages & QoL

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Help & FAQ page (read-only) | Sidebar bottom → Help & FAQ: six accordion sections open/close; every Copy button puts the command on the clipboard ("Copied!" flash); the Tools/Backups deep links navigate; Community opens the Dad's MMO Lab GitHub in the browser. No Discord link anywhere (none exists). |
| ⬜ | AZEROTH IS READY toast + Windows notification | Start the server from stopped and wait for boot → the moment the chip flips to "World is up": green bottom-right toast on whatever page you're on (click or 12s dismisses) AND a Windows notification (first time: Windows asks permission — allow). Restart → fires again after boot. Quit+reopen the launcher while the world is already up → NO notification on first poll. |
| ✅ | Players online card (read-only) | (Passed 2026-07-21 via CLI+DB: `players online` returned only Hypeer, excluded all ~2500 bots.) With the world up and your character logged in: Home shows a "Players online" card with name + lvl + class (bots never listed — compare a busy city). Log out → after a Refresh the card says "Nobody online right now." Card absent while stopped/starting. |
| ⬜ | Core GM cheat-sheet (read-only) | Commands page: "Core commands (always available)" card sits ABOVE the module blocks with 11 rows (.tele → .saveall); try one in-game (e.g. `.server info`) to confirm the wording matches reality. |
| ⬜ | Console command favorites | Console: type `server info` → star it (★ turns gold) → chip appears above the input; click the chip → input FILLS but does not send; star again → removed. Restart the launcher → favorites still there. |
| ⬜ | Teleport favorites | Teleport: star 2-3 locations → they pin to the top of the list (gold star); filter → favorites that match still sort first; unstar → row drops back. Restart the launcher → stars kept. |
| ⬜ | Wowhead links (read-only) | Item Database: search anything → 🔗 on a result row opens wowhead.com/wotlk/item=<id> in the browser (right item!); the Send box header 🔗 does the same. |
| ⬜ | [world-restart] Restart world only | Server running: Home → "Restart world only" → terminal streams the settings-don't-apply warning, saveall, docker restart, readiness wait; chip shows amber during, "World is up" after — noticeably faster than full Restart. THEN verify the caveat is real: change a Settings value, world-restart, confirm the change is NOT live; full Restart applies it. |
| ⬜ | Character switcher | Pick a character on any page's picker → sidebar footer shows "playing as <name>". Click the chip → dropdown lists all accounts' characters; pick another → every open picker (Teleport/GM Tools/Items) follows. Restart the launcher → still selected. Delete that character (or use another account) → pickers fall back to the first char without erroring. |
| ⬜ | [module-fixit] Battle Pass NPC fix | With Battle Pass installed: Modules → Battle Pass row → "Fix missing NPC" → note says placed in Stormwind + Orgrimmar and to restart. Restart the world → NPC "Battle Pass Vendor" stands in the Stormwind trade district (-8819 636) and Org Valley of Strength (1609 -4407) and opens gossip. Click Fix again → "already placed", no duplicate NPC after another restart. |
| ⬜ | [title-remove] Keep game data on remove | Remove an AC-based title with "Keep downloaded game data" CHECKED → stream says keeping the volume; `docker volume ls` in the distro still shows `<title>_ac-client-data`; reinstall skips the big client-data download. Remove again UNCHECKED → stream says removed game data volume and `docker volume ls` no longer lists it. |

## 18. Batch 4 — Auction House, internet play, URL installs, disk tools

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Auction House view (read-only) | Config ▸ Auction House: the curated rows load (seller/buyer, seller character, listings per cycle, duration class, item filters) with values from mod_ahbot.conf(.dist); the Repair card shows the 4 manual steps. Browsing works while locked. |
| ⬜ | [ahbot-page] AH settings save + live apply | Server RUNNING with mod-ah-bot installed: change "Listings added per cycle" → Save → green "Applied live ✓" (no restart banner) and `env/dist/etc/modules/mod_ahbot.conf` shows the new value. With a legacy AC_AUCTION_HOUSE_BOT_* env still in override.yml: save shows the amber restart banner instead and the env key is gone from override.yml. |
| ⬜ | [ahbot-page] Repair AH Bot | Create a throwaway account (Accounts page), log into the game once, create one character, log out. Config ▸ Auction House → pick that character → Repair (two-step confirm) → stream shows lookup + conf write + "reloaded"; mod_ahbot.conf has Account/GUID/EnableSeller=1/EnableBuyer=1; within minutes the AH starts filling (or after the next restart if the stream said restart). Click Repair again → "already configured". Buy one of the bot's auctions from YOUR character to prove the two-account setup works. |
| ⬜ | [internet-play] Apply + friend connects | Tools → Play over the internet: card shows detected public + LAN IPs; forward TCP 3724+8085 on the router per step 2; enter the public IP (or DuckDNS name) → Apply (two-step) → output confirms; realm DB address = that value. A friend OUTSIDE the LAN sets `set realmlist <addr>` and logs in with their own account. Confirm the card NEVER suggests forwarding 3306. |
| ⬜ | [internet-play] Revert | Revert to local play (two-step) → status back to 127.0.0.1; outside connections stop working. LAN card's Enable must still REJECT a public IP (CLI guard: "not a private LAN address"). |
| ⬜ | [title-url-install] Install from URL | Library → Install from URL: paste a trusted DML-convention repo (https) → Install → typed "install" confirm + the runs-their-script warning → InstallTerminal streams `dml run <url>` (clone + install.sh), prompts answerable in the reply row, Cancel works. A non-https or garbage URL never enables the button / errors cleanly. |
| ⬜ | [disk-tools] .wslconfig editor | Tools → Disk & performance: card shows current memory/processors (or empty when no .wslconfig). Set e.g. 12GB + half CPUs → Save → `%USERPROFILE%\.wslconfig` has them under [wsl2] and any UNRELATED lines/sections you had are untouched. After the Restart WSL card runs, `wsl -d dml-arch -- free -h` / `nproc` reflect the caps. |
| ⬜ | [disk-tools] Restart WSL | With the server RUNNING: type "restart-wsl" → button → note says the server stopped gracefully + WSL shut down; `wsl -l --running` shows nothing; characters intact after the next (cold) Start. With the server stopped → note says nothing was running. |
| ⬜ | [disk-tools] Shrink disk script | Stop the server → "Create the shrink script" → Explorer opens at Downloads\dml-shrink-wsl-disk.ps1; right-click → Run with PowerShell AS ADMIN → script finds ext4.vhdx via the registry, fstrims, shuts WSL down, diskpart-compacts, prints before/after GB. Verify the file size actually dropped (grows back over time — normal). |
| ⬜ | Defender exclusion (read-only) | The card shows a copyable `Add-MpPreference -ExclusionPath "<dml-arch disk folder>"` for the real vhdx folder; Copy works; run it in an admin PowerShell → `Get-MpPreference | select -Expand ExclusionPath` lists it. (Optional; undo with Remove-MpPreference.) |

## 19. Batch 5 — stretch batch

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Bot Browser (read-only) | (Passed 2026-07-21 via CLI+DB: total 2500=2500, name/level filters matched the DB exactly, online=1643.) Items & Bots ▸ Browse Bots: Search with no filters → ~2500 bots total, 50 per page, Prev/Next page through; name prefix / class / level range / online-only filters narrow the list; star a few bots → they pin to the top and survive an app restart; Details on a geared bot shows gear names (quality-colored) + talent point split + achievement count; Details on a naked bot shows "No gear saved yet" (NOT an error). |
| 🔶 | [bot-browser] Invite + set level | (Tested 2026-07-21 — 3 findings, stays locked pending fixes: (1) list should auto-load on page open, not wait for Search; (2) Details should open the full character view (paperdoll/3D/talents) like the Character page — bots ARE characters, reuse it; (3) Invite fails in-game with playerbots "you are not allowed to control bot X" — random world bots need a playerbots permission conf (research AiPlayerbot.RandomBotGroupNearby / allow-keys) or the feature needs re-scoping. Set-level not yet retested.) Original steps: Details → Invite to party → bot joins; Set level on an OFFLINE bot → succeeds. |
| ⬜ | [arac-client-patch] ARAC full patch | Modules → install mod-arac (note: NO rebuild banner appears — it's data-only) → "Apply client patch" on its row: stream shows 3 DBC copies into the data volume + Patch-A.MPQ into `<client>/Data/` (Data root, NOT Data/enUS). With no client folder set: warns + only the server half runs; set the folder, re-run → MPQ lands. Cold-start the server once (applies arac.sql), then restart → create e.g. a Night Elf Warrior; the patched client shows all combos at character creation. |
| ✅ | [gear-sets] Save + mail a gear set | (Passed 2026-07-21: user saved "test" + mailed to Testen; DB-verified mail #2588 with all 12 items attached.) Characters ▸ Character → load a geared character → name it + "Save gear set" (unlocked — local save only). Item Database → Gear sets card lists it (source char, item count) → Mail to… → pick a recipient → Mail N items → mailbox has the items as FRESH copies (count 1 each, two mails when >12 items); enchants/gems are NOT carried; a cross-class recipient still receives everything. Delete removes the set (two-step). Sets survive an app restart. |
| ✅ | [party-spec] Role picker + change spec | (Passed 2026-07-21 user-confirmed. UX findings queued: the Role/Class/Spec dropdowns render WHITE — native selects need dark-theme styling app-wide, same family as the scrollbar fix; plus general window polish TBD.) Items & Bots ▸ My Party (bridge deployed, character online): Role → Class → Spec (e.g. Ranged → Mage → frost pve) → Add bot → the bot joins with that spec's talents AND autogears (check its trees in-game or via Bot Browser details). "Any spec" add behaves exactly like the old class buttons. Per-bot "spec… → Change spec" respecs an existing bot (then Gear up). NB: spec names come from playerbots.conf — a wrong/edited name fails SILENTLY (in-game whisper reply only); no DK offered anywhere. |

## 20. Restart save option

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [skip-saveall] Faster restart toggle | Home (server running): the "Save characters before restart (safer; off = faster)" checkbox sits under the Stop/Restart buttons, ON by default. UNTICK it (needs untested features enabled) → Restart → the stream shows `Skipping pre-stop saveall (faster restart)…` instead of `Saving all characters (saveall)…`, and the restart is noticeably quicker. Re-tick → Restart → the `Saving all characters…` line is back. Same behavior applies to **Restart world only**. Setting survives an app restart. |
| ⬜ | Characters still safe with saveall off | With the box UNTICKED: play a bit (gain XP/gold), Restart, log back in → your progress is INTACT — the graceful stop still saved you on shutdown. (This is the whole point: off is faster, not lossy, in normal operation.) |

## 21. Overnight new features

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | [accountwide-config] Account-wide sharing configurator | Prereq: install **Accountwide Systems** from Modules (Lua) so its scripts deploy. Config ▸ **Account-wide**: before install the view says "isn't installed yet" and points at Modules; after install it lists every system (Achievements, Currency, Gold, Mounts, Pets, Playtime, Professions, PvP rank, Flight paths, Titles) all OFF, with the Achievement-progress/Realm-first and Live-gold-sync/alt-bot sub-toggles indented and disabled until their parent is on. Toggle **Mounts** on → the yellow "reload to apply" banner appears → **Reload account-wide scripts** → `.reload ale` result shows. Re-open the view → Mounts reads on (and `AccountMounts.lua` on the server has `ENABLE_ACCOUNTWIDE_MOUNTS = true`). **Reputation** is a dropdown (Off / Default / Custom); pick Default → the other variant file is deleted server-side and the flag flips on. Turn Mounts back off. NB the whole view is locked until "Enable untested features" is ticked. |
| ⬜ | [place-npc] Place NPC in capitals | Prereq: install one of **1v1 Arena / Transmogrification / NPC Beastmaster** (C++, then rebuild) or **Black Market AH** (Lua, then deploy) so its NPC template exists. On that module's Modules row a **Place NPC in capitals** button appears (locked until "Enable untested features" is on). Click it → note "Placed … in 2 capital(s). Restart the world server for it to appear." Restart (Home) → the NPC stands in Stormwind (trade district) AND Orgrimmar (Valley of Strength). Click again WITHOUT restarting → note "already placed in Stormwind + Orgrimmar" and no duplicate spawns. Delete just the Orgrimmar spawn and click again → only Orgrimmar is re-placed (per-map idempotence). On a module whose template is missing, the button errors NO_TEMPLATE pointing you to install/rebuild first. |
| ⬜ | [ahbot-page] AH Bot Plus fork detection | With **mod-ah-bot-plus** installed instead of mod-ah-bot: Config ▸ **Auction House** heading reads "Repair Auction House Bot Plus" and shows a "Detected the Auction House Bot Plus fork" note; the repair button targets it (writes mod_ahbot.conf, NO "mod-ah-bot is not installed" error). With plain **mod-ah-bot** it reads "Repair Auction House Bot". With neither installed the tab shows the "install one first" note and keeps the standard label. (Reuses the existing ahbot-page lock — no new flag.) |
| ✅ | [backup-validate] Verify a backup | (Passed 2026-07-21 via CLI: a real backup validated intact, a corrupt file was caught with the gzip-fail message.) Backups page: each backup row has a **Verify** button (locked until "Enable untested features" is ticked; safe to run anytime, even with the server stopped). Click it on a real backup → a green `✓ Archive is intact and looks like a full character backup.` line appears under the row. Corrupt a copy (e.g. `truncate -s 20 ~/.dml/backups/<file>` in the shell) and Verify → a red `✗ gzip integrity check failed …` line. It never restores anything — pure read-only check. |
| 🔶 | [gm-return-home] (WORKS but change requested 2026-07-21: user wants faction capital — Horde→Orgrimmar, Alliance→Stormwind — instead of hearth/unstuck. Stays locked until reworked + retested.) Send home (unstuck) | GM Tools → pick a character → the Rescue row has a **Send home (unstuck)** button (locked until "Enable untested features" is ticked; enabled for OFFLINE chars too, unlike Revive/Heal). ONLINE test: move a character out into the world, click it → note `Sent <name> home to their hearth.` and in-game the character is teleported to their hearthstone/inn location. OFFLINE test: log the character out somewhere odd, click Send home, log back in → they load at their hearth. If the online character is in combat or on a flight path it errors (SOAP fault) — retry once idle. |
| ⬜ | [gear-sets-io] Gear set TOML export/import | Items page → **Gear sets** card. Save a set first (Characters ▸ Character). Click **Export** on the row → a monospace TOML block appears with `name = …`, `class/level`, and one `[[items]]` table per item; **Copy** puts it on the clipboard. Paste that block into the **Import a gear set** box → **Import** (locked until "Enable untested features" is ticked) → note `Imported "<name>" — N items.` and the set appears in the list (importing the same name replaces it). Paste garbage or an empty block → a red error like `That doesn't look like a gear set…` and nothing is added. Pure local (localStorage) — no server involved. |
| ⬜ | [tailscale-play] Play Together over the internet (Tailscale) | Tools → **Play Together over the internet (Tailscale)** card (whole card locked until "Enable untested features" is ticked). **1. Install** → note "Tailscale installed." (or "already installed"); on a box without passwordless sudo it errors SUDO_REQUIRED pointing at the DML shell. **2. Log in** → for a fresh tailnet it shows a `https://login.tailscale.com/a/…` link (Copy it, open on your phone, sign in), then click **Refresh status** → it flips to "Connected — your address is 100.x.y.z". Already-logged-in boxes go straight to Connected. **3.** Click **Point my realm at 100.x** → note "Your realm now points friends at 100.x"; the `set realmlist 100.x` line + Copy appear. A friend who joined your tailnet (their own Tailscale app) puts that line in `Data\enUS\realmlist.wtf`, makes an account (Accounts page), and connects — no router port-forwarding. **4. Disconnect Tailscale** (two-click confirm) → tailnet goes down. Re-run **Point my realm** back to LAN/localhost from the LAN card when done. NB kernel-TUN boxes also open the 3724/8085 tailnet firewall rule automatically; userspace-mode boxes skip it (ports are open anyway). |
| ⬜ | [port-proxy] Database access / LAN diagnostic + MySQL exposure | (Diagnostic half verified 2026-07-21 via CLI: login/world = 0.0.0.0 LAN-ready, DB port 13306 = this-PC-only; exposure script still untested.) Tools → **Database access / LAN diagnostic** card. With the server running, **Re-check** → "Game ports (login + world) are reachable from other PCs" and a per-port list (login/world/database with `host_ip:host_port` + a LAN-reachable / this-PC-only chip). The **HeidiSQL on THIS PC** line shows Host `127.0.0.1`, Port = the real DB host port (3306, or 13306 if remapped — NOT hardcoded), user `root` / pw `password`; connect HeidiSQL locally with those and confirm it opens the DB. **Create the LAN-exposure script** (locked until "Enable untested features" is ticked) writes `%USERPROFILE%\Downloads\dml-expose-mysql.ps1` and opens Explorer at it — open the file and confirm: the LAN-only/never-the-internet warning, `netsh … portproxy … listenport=<dbport>` with a real LAN IP (never 0.0.0.0), a `New-NetFirewallRule … -Profile Domain,Private` (never Public), and undo instructions. Right-click → Run as Administrator, then from a second LAN PC connect HeidiSQL to Host = this PC's LAN IP, Port = `<dbport>`. Stop the server → Re-check says "isn't running". Undo per the script header when done. |
| ⬜ | [title-remove] Delete server images on remove | Library → Remove an AC-based title. In the confirm dialog tick **Also delete downloaded server images (~3-5 GB)** and type the id → the stream prints `removed server image mysql:8.4`, `removed server image acore/ac-wotlk-worldserver:…` and `docker image ls` in the distro no longer lists them. Reinstall re-pulls the images (slower). Remove another title WITHOUT the box → the stream says `kept the downloaded server images …` and `docker image ls` still lists them (fast reinstall). Reuses the existing [title-remove] lock — the whole Remove flow is gated on it. |
| ✅ | [cache-maint] Cache maintenance | (Passed 2026-07-21/22 user-confirmed: sizes shown → cleared → re-grew on use; also served as the robe-mystery control experiment.) Tools → **Cache maintenance** card. It lists two runtime caches with sizes: **3D models & icons (this PC)** and **Item tooltips & icons** (the WSL `~/.dml/wowhead-cache`). Browse Items / open a 3D model first so both have content, then **Refresh** → non-zero sizes + file counts. **Clear caches** (two-click confirm; locked until "Enable untested features" is ticked) → note `Cleared <size> …`; Refresh shows both back at 0 B / 0 files, and in the distro `~/.dml/wowhead-cache` is gone while the rest of `~/.dml` (backups, client-path) is intact. Re-open an item/model → it re-downloads and the cache grows again. The built-in talent-tree and achievement data are NOT affected (still work with the cache empty). |
| ⬜ | [guided-config] Guided module tuning | Config ▸ **Module tuning**. Cards appear for **NPC Beastmaster**, **Learn Spells on Level-up**, **Unlimited Ammo** and **Sit Means Rest**; a module you haven't installed shows a "Not installed — install from the Modules page first" note but still lists its knobs at defaults. Save is locked until "Enable untested features" is ticked. **Conf module** (install NPC Beastmaster: C++, rebuild): flip **Hunters only** off / set **Minimum level** → Save → the yellow "restart to apply" banner appears; `mod_npc_beastmaster.conf` on the server shows the new values, comments intact. **Lua module** (install Unlimited Ammo: Lua, deploy): turn **Enable unlimited ammo** on / change **Ammo to keep stocked** → Save → the "reload the Lua scripts" banner appears → **Reload Lua scripts** → `.reload ale` result shows; `UnlimitedAmmo.lua` reads `ENABLED = true` with its inline comment preserved. Re-open the view → every value reads back what you set. **AllowedClasses** takes a comma-separated list (e.g. `3,8`); a bad value (letters/`;`) is rejected with a clear error and nothing is written. |

## 22. Sidebar redesign + look (2026-07-20)

All read-only/cosmetic — no feature flags, nothing to unlock.

| Status | Test | Steps / expected |
|---|---|---|
| ⬜ | Dropdown sidebar (accordion) | Each multi-item group (**Server · Characters · Items & Bots · Config**) is a clickable header with a ▸/▾ caret — click to expand/collapse. Several can be open at once. **Help & FAQ** stays a plain link (no caret). On app start only the group holding the current page (Server, for Home) is open. Navigating from a collapsed group's item — e.g. the sidebar status chip → Home — opens that group automatically. Nothing is orphaned: every page from the old sidebar is still reachable. |
| ⬜ | Tabs moved out of pages | The old in-page tab bars are GONE from Config and Bots. **Config** expands to Settings / Bot World / Auction House / Account-wide / Module tuning / Module files / Backups; **Items & Bots** expands to Item Database / My Party / Browse Bots / Commands. Picking one switches the view directly, and the page header names the view you're on. |
| ⬜ | Config keeps state across views | Type an edit into a **Config ▸ Settings** row (don't save), switch to **Config ▸ Bot World** and back → your unsaved edit is still there (the page stays mounted across sidebar views, exactly as the old tabs behaved). Switching away to Home and back does reset it (page unmounts) — that's expected. |
| ⬜ | Character view keeps its tabs | **Characters ▸ Character** still has the Character / Talents / Achievements tab strip inside the page — the one place tabs remain. |
| ✅ | Gear auto-loads on character select | (Passed 2026-07-21 — user-confirmed auto-load, no click. Finding en route: an ONLINE character's just-changed gear can render stale until a save flushes it — saveall + Reload gear refreshes; candidate fix: saveall-before-paperdoll for online chars.) Open **Characters ▸ Character** with a character already chosen → gear loads on its own, no click. Change character in the sidebar **"playing as"** switcher → the view auto-loads the new character's gear (and lands back on the Character tab). **Reload gear** re-fetches the same character. With no character selected, nothing auto-loads and the button stays disabled. |
| ⬜ | Themed scrollbars + selection | Scroll anywhere with a scrollbar (sidebar, a long page, the Console log, the character dropdown, a conf textarea) → the bar is dark grey and rounded, matching the UI (not the light system bar), and lightens on hover. Select some text → the highlight is the app's blue, not the browser default. |

## 23. Statistics page (2026-07-21, read-only — no flag)

| Status | Test | Steps / expected |
|---|---|---|
| ✅ | Statistics page | (Passed 2026-07-21 user-confirmed after restart. Enhancement queued: All/Family/Bots segment filter.) (Requires a `npm run tauri dev` RESTART first — new Rust command.) **Server ▸ Statistics** with the server running → five sections load in ~a second: World Population (family vs bots, level-spread chart, classes, faction split, top levels, guilds), Economy (gold totals family/bots/total that sum, richest top-5, "auction house shop stock", mail), Family's Journey (one row per family character: playtime/last seen/achievements/quests/kills), Server History (boots, lifetime hours, longest session, peak "includes bot connections", per-boot sparkline), Bot Watch (busiest zones BY NAME e.g. The Barrens, continents, combined bot playtime in years). Numbers should look sane vs. reality (e.g. your gold, ~2500 bots). Refresh re-fetches. With the server stopped → calm "start the server" note, no errors. |

## Known caveats (not tests — expectations)

- ARAC (C++ module): installing the module is server-side only. The client DBC/MPQ patch is now a separate **Apply client patch** step on the module's Modules row (Batch 5, locked behind [arac-client-patch] until row §19 passes) — so a bare install still won't show new race/class combos until you run that patch and cold-start once. Not a bug; it's the two-step design.
- First view of a high-talent char fills talent icons in batches (~10-15s per 25) — not a hang.
- Full backups share the keep-10 pool; restoring an older full backup while a module is installed re-applies that module's SQL at next start.
- Cold Start after a full Stop: the world can crash-retry for ~2 min while MySQL warms up (Docker self-heals — normal). Rarely, the world then wedges mid-load (log frozen + 0% CPU for 3+ min): that's a hang, not slow loading — click Restart to clear it (observed once, 2026-07-18).
- A cold Start (full Stop first) also re-runs `ac-db-import`, which applies any pending AzerothCore database migrations — the schema can change under features that query it (this broke the character view once, 2026-07-19; fixed schema-adaptively). Restart never does this. If a DB-reading page errors right after a cold start, suspect a migration and report it.
