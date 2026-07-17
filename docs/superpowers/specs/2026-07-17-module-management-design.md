# Module Management — Design Spec (Round C)

**Date:** 2026-07-17
**Branch:** `feat/dml-launcher-windows`
**Status:** User-scoped (all three module kinds in one round; catalog = registry + custom URL; rebuild = prompt-then-stream; backup offered before installs). Design review waived. Reference mechanics: `.superpowers/sdd/recon-modules.md` (from `guides/wow-wotlk/wow-manage.sh`).

## What this builds

A **Modules** page (Server section) managing three module families on the playerbots server, ported from the DML server manager but non-interactive (flags instead of `ask_yes_no`), credentialed (`_db_pw()`, never a hardcoded password), and honest about state:

1. **C++ modules** — registry of 18 (the manager's 17 + `mod-custom-login`) plus a custom git-URL row. Install = shallow clone into `<server>/modules/<key>`; SQL is **NEVER hand-applied** (AzerothCore's `ac-db-import` auto-applies and tracks it in the `updates` table — the manager's load-bearing gotcha, confirmed by a real incident). Needs a 30–90 min worldserver rebuild to take effect.
2. **Lua (ALE) scripts** — the manager's 9-entry `ALE_SCRIPT_REGISTRY`. Gated on `mod-ale` being cloned. Install = clone (sparse for `sod`/`bmah`, which live in the upstream `DadsMmoLab/dads-mmo-lab` repo) + data-driven deploy of `.lua` files into `env/dist/etc/modules/lua_scripts/` (per-key copy rules incl. the activechat rename/require-patch and battlepass require-strip) + per-key SQL applied via `docker exec mysql` (this family's SQL IS hand-applied — normal for ALE) + optional client-side copies.
3. **SQL mods** — the manager's registry minus `xp-rates` (its `conf_xp` type edits worldserver.conf XP rates, which our Settings page already owns via env — two writers would fight) and minus `mod-custom-login` (folded into the C++ registry where it belongs). Typed installers: `clone_sql`, `clone_sql_norevert`, `clone_sql_pick` (hearthstone variants), `clone_dist` (sed-templated SQL), `tweak_world` (inline UPDATEs, mutually-exclusive tweak group, marker-recorded multipliers, arithmetic reversal). Marker files under `<server>/sql_scripts/installed/`.

Plus, per the user's request: **a backup offer before installing** — see Backup gate below.

Delivered as **two sequential plans** (one round, built back-to-back, no pause): **C1** = backup flag + C++ module family + rebuild + conf activation + Modules page; **C2** = ALE + SQL-mod families + client-path management + page expansion.

## Shared design

### State & paths (all inside the server dir, matching the manager so a box that used the manager reads correctly)

- C++ clones: `<server>/modules/<key>/` — installed = `.git` dir exists.
- ALE clones: `<server>/ale_scripts/<key>/`; deployed = per-key file check in `<server>/env/dist/etc/modules/lua_scripts/`.
- SQL mods: `<server>/sql_scripts/{installed,clones,config}/`; installed = `installed/<key>.installed` marker (may hold key=value state, e.g. applied tweak multipliers).
- Rebuild-pending: `<server>/.dml-rebuild-pending` — one C++ module key per line, appended on install/remove, cleared by a successful rebuild. Drives the "Rebuild required" banner.
- Client path: `~/.dml/client-path` (validated before save; see C2).

### Backup gate (user requirement)

`wow backup create` gains `--include-world` (adds `acore_world` to the same dump file). World-inclusive backups are named `wow-<ts>-full.sql.gz` so the list/UI can show what they contain, and they restore through the **existing** `backup restore` verb (the dump carries its own database statements; world+auth are already stopped during import). Restoring a `-full` backup makes the automatic pre-restore safety dump full as well (named `…-full-prerestore.sql.gz`) — otherwise world changes since the backup would be unrecoverable. Every DB-mutating module operation (`module install`/`remove` of ALE-with-SQL and SQL-mod types, and `module rebuild` — the moment C++ module SQL actually lands) **requires an explicit choice**: `--backup` (runs the equivalent of `backup create --include-world` first; abort on backup failure) or `--no-backup`. Neither flag → `BAD_ARG` telling the caller to pick. The UI maps this to a "Back up the server first (recommended)" checkbox, default ON. Clone-only operations (C++ install/remove) don't take the flags — their DB moment is the rebuild.

### CLI shape

New source file `cli/src/70-modules.sh` (registries + helpers) with arms in `90-main.sh` under `wow module …`. Streaming NDJSON verbs (like bridge-setup) for anything multi-step: install, remove-with-deploy-cleanup, rebuild. Request-response `json_ok` for list/conf/client-path. All mysql access via `docker exec ac-database mysql -uroot -p"$(_db_pw)"`; container names are fixed (ours), never discovered. All registry rows are data in `70-modules.sh`, formats identical to the manager's (`key|name|url|sql_dirs`, `key|name|url`, `key|name|url|type`).

- `wow module list --json` → `{families:{cpp:[{key,name,installed,pending_rebuild,conf:{state}}...],lua:[{key,name,cloned,deployed}...],sql:[{key,name,type,installed}...]}, rebuild_pending:[keys], ale_ready:bool}`
- `wow module install --family cpp|lua|sql --key <key> [--url <git-url>] [--backup|--no-backup] [--variant <v>] --json` (NDJSON stream). `--url` only with `--family cpp` and a key not in the registry (custom module; key = repo basename, validated `^mod-[a-z0-9-]+$` after normalization, URL validated `^https://[A-Za-z0-9._~/-]+(\.git)?$`). `--variant` only for `clone_sql_pick` (hearthstone: `1sec|1min|5min|15min|30min`) and `clone_dist` (npc-teleporter level). cpp installs/updates: clone or pull, append key to `.dml-rebuild-pending`, emit "rebuild required" event. lua installs: require mod-ale cloned (`NOT_READY` otherwise), clone/sparse-clone, deploy per-key, apply per-key SQL, note `.reload ale` (or restart for bmah).
- `wow module remove --family … --key … [--backup|--no-backup] --json` (NDJSON). cpp: delete clone, keep DB rows + conf (mod-arac gets the data-only warning event), append to rebuild-pending. lua: delete clone + deployed files, keep DB. sql: typed reversal exactly like the manager (down.sql where it exists; hearthstone reset SQL; npc-teleporter DELETEs; tweak inverse multipliers from marker; `clone_sql_norevert` → error `NO_REVERT` pointing at backups).
- `wow module rebuild [--backup|--no-backup] --json` (NDJSON stream, LONG): `docker compose stop ac-worldserver` → `docker compose up -d --build`, full log tee'd to `<server>/rebuild.log`, filtered lines (`Step|Building|Compiling|Linking|Successfully|ERROR|error:|Created`) streamed as NDJSON; success clears `.dml-rebuild-pending`; failure leaves it and the exit code reflects it.
- `wow module conf --key <key> --json` / `wow module conf-activate --key <key> [--force] --json`: per-key conf name table from the manager (`mod-ah-bot → mod_ahbot.conf`, …); status = `none | needs-rebuild | ready | active`; activate copies `.conf.dist → env/dist/etc/modules/<name>.conf` (existing active + no `--force` → `EXISTS` error). Editing stays on the existing Config page (whose file allowlist gains nothing in C1 — curated files only; a follow-up can widen it).
- `wow client-path get|set --path <p>|detect --json` (C2): `set` validates (dir contains `Wow.exe`/`wow.exe`/`WowT.exe` or `Interface/`), converts `C:\…` → `/mnt/c/…`; `detect` scans the manager's candidate list non-interactively and returns candidates for the UI to pick from. Client copies during lua installs: skip with a warning event when no client path is set.

### UI (Modules page, Server section)

Nav: new `{ id: "modmanager", label: "Modules" }` after Console; the Config section's existing entry renamed `Modules` → `Module Configs` (id stays `modules` — mounts/pins updated). Page cards:

1. **Rebuild banner** (only when `rebuild_pending` non-empty): lists pending keys, backup checkbox (default ON), Rebuild button → two-step confirm ("takes 30–90 minutes; the world stops during the build") → streams into the shared Terminal component.
2. **C++ modules**: registry rows (name, status chip: Not installed / Installed — rebuild pending / Installed) with Install/Update/Remove; custom row: URL input + Install. Install/remove are streamed; remove two-step confirm; mod-arac remove shows the data-only warning.
3. **Lua scripts** (C2): gated card ("Install the ALE module first" when mod-ale absent); rows with Cloned/Deployed state, Install/Remove, backup checkbox on SQL-bearing installs, `.reload ale` hint after install.
4. **SQL mods** (C2): rows with type-appropriate controls (variant picker for hearthstone; the four tweak rows note mutual exclusivity), backup checkbox default ON, Remove disabled with tooltip "no automated reversal — restore a backup" for `clone_sql_norevert`.
5. **Client folder** (C2): current path + Detect (candidate picklist) + manual entry; needed by bmah/paragon/sod client copies.

### Errors & safety

- Free-text reaches nothing: URLs/keys/variants regex-validated; everything travels argv; SQL is only ever repo files or registry-inlined statements (never user text).
- The raw-write lock on `.env`/compose override is untouched; rebuild uses compose as-is.
- `updates`-table repair (the manager's `clear_update_tracking_row`) is OUT of scope — if db-import desyncs, that's a manual fix; we never cause it because we never hand-apply cpp SQL.
- ALE quirks ported as data: activechat basename-collision renames + require patches; battlepass CSMH require-strip; sod stale-file cleanup. `levelupreward` (orphaned in the manager: has code paths but no registry row) is dropped. `battlepass`'s claimed client addon doesn't exist in its install path (recon) — no client copy for it.
- Rebuild failure leaves worldserver stopped or half-built — the stream says so and points at `<server>/rebuild.log`; the Home health panel already shows honest container state.

### Testing

Same harness style as every round: bats with docker/curl/mysql stubs (git stubbed via a `git` stub script logging argv and creating fake clone dirs — new `use_git_stub`), one suite per family (`wow-module-cpp.bats`, `wow-module-rebuild.bats`, `wow-module-lua.bats`, `wow-module-sql.bats`, `wow-client-path.bats`) plus `--include-world` tests in `wow-backup.bats`. Launcher gates: vitest (nav pins change), svelte-check, cargo. Live gate additions (batched): install mod-aoe-loot → banner → rebuild (real 30-90 min build) → conf activate → in-game check; install an ALE script + `.reload ale`; install a tweak SQL-mod with backup and remove it; set client path.
