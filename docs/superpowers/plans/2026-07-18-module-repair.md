# Module Repair Tools Implementation Plan (Round J)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `wow module tracking` + `wow module repair` (mark/clear on the `updates` tables) + a Repair panel on the Modules page. The spec (docs/superpowers/specs/2026-07-18-module-repair-design.md) carries the binding contracts — read it first for every task.

## Global Constraints

- Branch `feat/dml-launcher-windows`; NO merge. `cli/dml` committed artifact. `set -euo pipefail`; NO `local` in dispatch arms. Validators BEFORE any SQL/path use: key `_valid_cpp_key` + installed check; db closed set `world|characters|auth` (→ `acore_<db>`); mode closed set; EVERY filename `^[A-Za-z0-9._-]+\.sql$` (no slash — reject with BAD_ARG naming the offending file).
- FOURTH sanctioned write = `updates` tables only. New `_db_write_stmt <acore_db> <stmt>` in 30-db.sh (db name validated against the three acore names inside the helper as defense-in-depth); `_chars_write_stmt` becomes `_db_write_stmt acore_characters "$1"` (behavior identical — its bats keep passing). Update the sanctioned-write comment blocks (30-db.sh, 60-backup.sh, README security posture) to enumerate FOUR writes.
- mark: file located `find "$sdir/modules/$key" -name "$f" | head -1`; sha1 = `sha1sum "$file" | awk '{print toupper($1)}'`; INSERT…ON DUPLICATE exactly per spec (state RELEASED, NOW(), speed 0). clear: COUNT then DELETE by exact name. Reads via db_chars_query-style helpers pointed at the right DB — check how db_world_query/db_chars_query differ and add `db_auth_query` use if it exists (it does — grep) ; discovery = top-level `*.sql` of `modules/K/data/sql/db-<short>/` else `modules/K/sql/<short>/`.
- UI copy exact (spec): mode labels, confirm `This edits the database's update-tracking records. Continue?`, success hint `Restart the server to apply.`
- Gates: full bats; npm test; npm run check; cargo test. Entering J: bats 370, vitest 37, cargo 25, check 0/0. Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

### Task 1: CLI `tracking` + `repair` + `_db_write_stmt` + bats (~10 per the spec's list; full suite expect ≈380). Arms live in the `module` sub-case (after `conf-activate)`). Commit `feat(cli): module tracking + repair (updates-table mark/clear)`.

### Task 2: Rust + api.ts — `wow_module_tracking(key: String)`; `wow_module_repair(key: String, db: String, mode: String, files: Option<String>)` (`--files` only when Some); register; api types `ModuleTracking`/`RepairResult` + wrappers matching the CLI payloads exactly (cross-check emissions). Gates cargo/vitest/check. Commit `feat(launcher): module repair commands`.

### Task 3: ModuleManager Repair panel — per the spec's UI section verbatim (toggle on installed cpp rows, diagnosis view from tracking, db+mode selects with the exact labels, two-step confirm, per-file results, restart hint). Reuse the page's busy/error/confirm patterns; no {@html}; keyed eachs. Gates npm test (37) + check 0/0. Commit `feat(launcher): module repair panel`.
