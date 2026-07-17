# Character Backups — Design

**Date:** 2026-07-17
**Branch:** `feat/dml-launcher-windows` (stays here; no merge until asked)
**Round:** 5 of 5 (final) of the Lab-parity roadmap (sidebar ✓ → GM tools ✓ → summon ✓ → My Party ph2 ✓ → **backups**)

## Goal

Whole-server character snapshots: one click backs up every character, account, and bot; restoring rolls the whole character state back to that moment (server auto-stopped for the import, auto-restarted after). Backups page in the CONFIG sidebar slot reserved since round 1.

**User-chosen model** (over The Lab's per-character staging/guid-remap machinery, which was recovered from the binary and judged the riskiest option): whole-DB snapshots, manual trigger + an automatic safety backup before every restore. No scheduled backups, no auto-on-stop.

## What a snapshot contains

One `mysqldump --databases acore_characters acore_playerbots acore_auth` — characters/items/mail/pets/auctions, bot state, AND accounts/passwords (restoring rolls accounts back too, so characters can never orphan from their accounts; this is stated in the UI copy). `acore_world` (static game content) is excluded — snapshots stay small.

- Storage: `~/.dml/backups/` in the dml-arch distro (helper `_backup_dir`).
- Filenames: `wow-YYYYMMDD-HHMMSS.sql.gz` (UTC, `date -u`); the automatic pre-restore safety file uses `wow-YYYYMMDD-HHMMSS-prerestore.sql.gz`.
- Filename validation everywhere a `--file` flag is accepted: `^wow-[0-9]{8}-[0-9]{6}(-prerestore)?\.sql\.gz$` — no traversal possible by construction.
- Retention: newest **10** kept (env override `DML_BACKUP_KEEP`, test seam); older files pruned on each successful create, with every pruned name reported in the stream — never silent.

## THE write-policy exception (explicit)

The project's MySQL posture has been strictly read-only since Plan 3. `backup restore` is the ONE sanctioned write path, bounded by three invariants:

1. Writes happen only via `mysql` inside the `backup restore` verb — no other verb gains write access.
2. The worldserver + authserver are ALWAYS stopped first (the import is then safe from the server's in-memory character cache — the same cache problem the GM bridge exists to avoid; the DB container stays up because the import needs it).
3. An automatic safety backup is ALWAYS taken before the import, so a restore can itself be undone.

CLAUDE.md's security-posture bullet gets this exception documented in the same breath as the read-only rule.

## CLI: `dml wow backup` namespace

New `cli/src/60-backup.sh` (helpers: `_backup_dir`, `_valid_backup_name`, `_backup_prune`) + arms in `cli/src/90-main.sh`'s wow dispatch (`backup)` wsub with bsub dispatch). Standard envelopes; bash rules as always.

### `backup create --json` (NDJSON streaming)

Section `backup-create`: preflight the DB container is reachable (docker up + container running; failure → `DB_UNREACHABLE`) → `docker exec ac-database mysqldump --databases acore_characters acore_playerbots acore_auth --single-transaction --quick` piped through `gzip` to a tmp file, then `mv` into place (no partial files on failure) → prune retention (line per pruned file) → `done {"file":"wow-…sql.gz","size":<bytes>,"pruned":[…]}`. Works while the server RUNS (`--single-transaction` = consistent InnoDB snapshot). A failure of the dump/gzip pipeline itself → `ndjson_error BACKUP_FAILED` (stderr tail as hint) and the tmp file is removed. Retention counts ALL files in the dir including `-prerestore` safety files — newest 10 overall.

### `backup list --json`

`{"backups":[{"file":"…","size":N,"created":"YYYY-MM-DD HH:MM:SS"}]}` — created parsed from the filename (no stat-format portability games); newest first; empty/missing dir → `[]`.

### `backup delete --file <name> --json`

Name validated → missing → `NOT_FOUND` → `rm` → `{"deleted":true,"file":"…"}`.

### `backup restore --file <name> --json` (NDJSON streaming)

Section `backup-restore`, phases in order — each phase line-reported:

1. Validate name + file exists (→ `NOT_FOUND` error event) + server dir found.
2. **Stop the game**: `docker compose stop ac-worldserver ac-authserver` (line "server stopped"). Failure → error event, abort (nothing written).
3. **Safety backup** via the same dump pipeline, `-prerestore` suffix (line names it). Failure → error event, abort, and RESTART the server (nothing was written yet, safe to resume).
4. **Import**: `gunzip -c <file> | docker exec -i ac-database mysql` (line "restoring …"). `--databases` dumps carry `CREATE DATABASE IF NOT EXISTS` + `DROP TABLE`/`CREATE TABLE`, so tables are replaced cleanly. **On failure: the server is deliberately LEFT STOPPED** — error event with hint naming the safety file ("Import failed — the server was left stopped. Your pre-restore state is saved as <safety>; restore it or start the server manually once resolved."). Never auto-start into a half-imported DB.
5. **Restart**: `docker compose start ac-worldserver ac-authserver` (line "server starting").
6. `done {"restored":true,"file":"…","safety_backup":"…-prerestore.sql.gz"}`.

## Launcher

**Nav**: CONFIG section gains `{ id: "backups", label: "Backups" }` after Modules (`nav.ts` + `nav.test.ts` pinned arrays — the LAST reserved entry; the round-1 "entries ship with their page" rule closes out).

**New page `launcher/src/lib/pages/Backups.svelte`:**
- Header + Refresh; intro line: "Snapshots of every character, account and bot. Restoring rolls ALL of them back to that moment."
- **Back up now** button — streams `backup create` into the shared Terminal; success note `Backed up — <file> (<size>).` (size humanized); works while the server runs (copy says so).
- Snapshot list (file, date, size, prerestore-tagged rows labeled "safety backup"), each row with:
  - **Restore** — two-step confirm, deliberately scary copy: `This rolls EVERY character back to <date> and restarts the server — sure?`; streams `backup restore` into the Terminal; outcome derived from the stream's `done`/`error` events, NOT promise resolution (the round-4 contract lesson: streaming promises resolve even on CLI failure); success note `Restored — the server is starting back up. Pre-restore state saved as <safety>.`; on a streamed error the error card shows it (and the terminal has the detail).
  - **Delete** — two-step confirm.
- All controls disabled while any backup operation streams (`backingUp || restoring` gating, one flag family; also disabled while `restartState.restarting`).
- List refreshes on mount and after create/restore/delete.

**Rust**: `wow_backup_create(on_event)` + `wow_backup_restore(file, on_event)` via `stream_args`; `wow_backup_list()` + `wow_backup_delete(file)` via `run_json_cmd`. **api.ts**: `BackupInfo {file, size, created}`, wrappers `wowBackupCreate`, `wowBackupList`, `wowBackupDelete`, `wowBackupRestore` (streaming signatures like `wowPartyPresetLoad`).

## Error handling

Envelope → CmdError → error card chains as everywhere. New error code `BACKUP_FAILED` (dump/import pipeline failures with a stderr-tail hint) joins the documented set; `DB_UNREACHABLE`/`NOT_FOUND`/`BAD_ARG` reused. The import-failure path's "server left stopped" state is a deliberate, documented outcome, not a bug — Home's status card will show it, and the hint says what to do.

## Testing & gates

- **bats** (new `cli/tests/wow-backup.bats`; docker stub in `helpers/env.bash` grows mysqldump/mysql/compose-stop/compose-start arms with capture-append support): create pipeline (capture asserts `--databases acore_characters acore_playerbots acore_auth --single-transaction`), tmp-then-mv (no partial file on simulated dump failure), retention prune at `DML_BACKUP_KEEP=2` with pruned names reported, list newest-first + empty `[]`, delete + NOT_FOUND, name validation (traversal attempts → BAD_ARG), restore ordering pinned by line-number assertions on the capture (stop < safety dump < import < start), import-failure leaves server stopped (NO compose start captured after the failed import) + hint names the safety file, stop-failure aborts before any write.
- Gates: full bats suite, `svelte-check` 0/0, vitest (nav pins gain backups), `cargo test`, tauri release build.
- **User live gate (batched with rounds 1–4):** Back up now → note + file listed; delete a test item from Testen in-game → Restore the snapshot → item is back; confirm the pre-restore safety file appeared.

## Out of scope

Per-character export/import (the Lab's staging machinery — declined), scheduled/auto-on-stop backups, world-DB snapshots, cross-server import, backup encryption, off-box copies (files are plain .sql.gz in the distro — users can copy them anywhere).
