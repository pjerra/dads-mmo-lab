# Module Repair Tools — Design Spec (Round J)

**Date:** 2026-07-18 · **Branch:** `feat/dml-launcher-windows` · Design review waived. Shortlist item J — the `ac-db-import "Table already exists"` rescue kit, ported non-interactively from the manager's `repair_install_state`/`repair_module` (wow-manage.sh 1633-1892).

## The problem it fixes

AzerothCore's db-import tracks every applied module SQL file in each DB's `updates` table
(name + SHA1 + state). When that tracking desyncs — module reinstalled, file renamed,
hand-restored DB — the next start fails hard ("Table X already exists") or silently
re-applies SQL. The manager's proven fix has two modes: **MARK** (insert the file's SHA1
so AC skips it — for "table exists but no tracking row") and **CLEAR** (delete the row so
AC re-applies — safe only for idempotent SQL). Never touch actual tables.

## CLI

**`wow module tracking --key K --json`** — read-only diagnosis. For each of
`acore_world|acore_characters|acore_auth`: `updates` rows whose name matches the module's
LIKE terms (key minus `mod-`, plus underscored variant — the manager's exact matching),
AND the module's discovered SQL files per DB (`modules/K/data/sql/db-<short>/*.sql`
top-level, fallback `modules/K/sql/<short>/`) each flagged `tracked`/`untracked`.
Shape: `{key, dbs: {world: {tracked_rows: [name…], files: [{name, tracked}]}, characters: …, auth: …}}`.

**`wow module repair --key K --db world|characters|auth --mode mark|clear [--files "a.sql b.sql"] --json`**
— files default to that DB's discovered list; every filename (given or discovered)
validated `^[A-Za-z0-9._-]+\.sql$` (no slashes — path-injection-proof) before use.
- `mark`: per file — locate under `modules/K` (find by name), sha1sum (uppercase — AC's
  UpdateFetcher format), `INSERT INTO updates (name,hash,state,timestamp,speed) VALUES
  (…,'RELEASED',NOW(),0) ON DUPLICATE KEY UPDATE hash=…, state='RELEASED'` — result
  `marked`; file not found on disk → `file_missing`.
- `clear`: per file — `SELECT COUNT(*)` then `DELETE FROM updates WHERE name=…` —
  result `cleared` / `not_tracked`.
- Payload: `{key, db, mode, results: [{file, result}]}`. Key must be an installed cpp
  module (registry or custom, `_valid_cpp_key` + `.git` check). db/mode closed sets.
- **This is the FOURTH sanctioned MySQL write** — scope: the `updates` tracking tables
  ONLY (meta-data about applied SQL, never game tables). Policy comment blocks updated.
- Writes go through a new `_db_write_stmt <db> <stmt>` (generalizing `_chars_write_stmt`
  to a validated db name; chars helper becomes a thin wrapper — no behavior change).

## UI (ModuleManager page)

Installed cpp-module rows gain a `Repair…` toggle revealing an inline panel:
- Fetches `tracking` on open; shows per-DB file lists with `tracked`/`untracked` chips
  and any matching tracked rows (the diagnosis view).
- DB select (world/characters/auth) + mode select with explanatory labels:
  `Mark as applied — fixes "Table already exists" on start` /
  `Clear tracking — makes the server re-apply the SQL (only safe if the SQL is
  re-runnable)`.
- Apply with two-step confirm, copy exactly:
  `This edits the database's update-tracking records. Continue?`
- Results listed per file (`marked` / `cleared` / `not tracked` / `file missing`);
  errors inline; hint after success: `Restart the server to apply.`

Out of scope: the manager's bulk fix-all sweep (maint opt 4), table-existence
introspection, MODULE_UPDATE_FILES known-lists (discovery covers the same files), auth-db
module SQL (rare — but the db select includes auth for completeness).

## Testing

bats (`wow-module-repair.bats`, ~10): tracking shape (fixture module SQL dirs + mysql
ROWS_SEQ for the LIKE queries), fallback sql/<short> dir, key/db/mode/filename
validation incl. `../evil.sql` and `x.sql;DROP` rejked, mark happy (query log: INSERT
with the real sha1 of the fixture file, uppercase, ON DUPLICATE), file_missing, clear
happy (COUNT→DELETE in log) + not_tracked, --files override respected. Gates: full
bats, vitest, cargo, check (entering J: bats 370, vitest 37, cargo 25, check 0/0).
Live gate: break tracking on purpose (delete a row for an installed module), watch
db-import fail, MARK it via the panel, watch the server start clean.
