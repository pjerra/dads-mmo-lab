# Server Self-Update — Design Spec + Plan (Round L)

**Date:** 2026-07-18 · **Branch:** `feat/dml-launcher-windows` · Design review waived. Port of the manager's `update_server_source` (wow-manage.sh:7018-7193). New feature lands LOCKED (`server-update`).

## CLI

**`wow update-check --json`** — read-only-ish (does a `git fetch --quiet origin` per repo to compute behind-counts; worktree untouched): for the AC server dir and `modules/mod-playerbots`: `{repos:[{label:"AzerothCore"|"mod-playerbots", url, branch, head, dirty (count of modified tracked files), behind (commits behind origin/<branch>; null if fetch failed)}]}`. Missing module dir → omitted with a note field. NOT_FOUND/GIT_MISSING as applicable.

**`wow update --backup|--no-backup --json`** — NDJSON streamed, ports `_pull_repo` faithfully but FAIL-CLOSED where the manager asked interactively:
1. Gates: server dir (NOT_FOUND); `.git` present (GIT_MISSING); AC origin URL must contain `mod-playerbots/azerothcore-wotlk` OR `liyunfan1223` else `REMOTE_MISMATCH` (hard error, no override — pulling upstream AC would break playerbots); same check for the module dir (`mod-playerbots/mod-playerbots`|liyunfan1223); AC branch must be `Playerbot` else `BRANCH_MISMATCH`.
2. Backup choice REQUIRED (`--backup` → `_module_backup_now` — new core revisions can run DB migrations at next start; abort on backup failure).
3. Per repo (AC then module; module dir missing → warn + skip): dirty tracked files → `git diff > local-changes-<ts>.patch` + `git stash push` (either failing → `EDIT_BACKUP_FAILED`, nothing pulled); `git pull --ff-only` (streamed; failure → stash pop restore + `PULL_FAILED` with the manager's "diverged branch?" hint); stash pop — conflict → `checkout -f -- .` + `reset --hard HEAD`, warn lines naming BOTH recovery locations (patch file path + `git stash pop` instructions); before/after shas → changed detection.
4. When anything changed: `_rebuild_pending_add "$sdir" core-update` — the existing Modules rebuild banner picks it up (`core-update` is display-only in the pending list; the cpp custom-scan ignores it since it fails `_valid_cpp_key`... VERIFY: `_valid_cpp_key` requires `mod-` prefix so `core-update` never renders as a module row — pending list shows it, rebuild clears it. Pin with a test.)
5. `ndjson_done {"changed":bool,"ac":"<before> -> <after>"|"up to date","playerbots":"…"|"skipped"}` + info line `Rebuild required to compile the update — use the rebuild banner on this page.` (only when changed).
DELIBERATE DEVIATIONS from the manager (document in code comments): no interactive overrides (fail closed); no chained auto-rebuild (the banner + existing rebuild flow own that); backup is the launcher's full DB backup, not skipped.

## Plumbing

`wow_update_check()` (run_json_cmd) + `wow_server_update(backup: bool, on_event)` (stream_args, `--backup`/`--no-backup`). api.ts: `UpdateRepo`/`UpdateCheck` types + `wowUpdateCheck()`, `wowServerUpdate(backup, onEvent)`.

## UI (ModuleManager, card `Server update` above Disk cleanup)

- `Check for updates` button (read-only, UNLOCKED) → repo rows: label, branch, short sha, `<n> behind` chip (green `up to date` when 0), `<n> local edits` chip when dirty.
- `Update` button gated `featureLocked("server-update")`, backup checkbox default ON, two-step confirm copy EXACTLY `Pulls the latest AzerothCore + mod-playerbots. Local edits are preserved (conflicts saved as patch files). New revisions can run DB migrations at next start. Continue?` → streams via runStream; after done with changes, note `Update pulled — rebuild required (see the banner above).`
- Flag `server-update` registered `"untested"`; SMOKE-TESTS §10 rows: check shows repo state; update on a clean tree pulls (or reports up-to-date) and the rebuild banner gains `core-update`; post-update rebuild compiles.

## Tasks

1. **CLI + bats** (`wow-server-update.bats`, ~9): git stub extended with seams (`DML_STUB_GIT_URL`, `DML_STUB_GIT_BRANCH`, `DML_STUB_GIT_HEAD_SEQ` space-sep rev-parse outputs, `DML_STUB_GIT_DIRTY` status output, `DML_STUB_GIT_PULL_EXIT`, `DML_STUB_GIT_STASH_POP_EXIT`, fetch/rev-list arms for update-check) — existing arms byte-compatible. Tests: remote-mismatch + branch-mismatch fail-closed (no pull in log); backup-choice required; clean pull up-to-date (changed:false, NO pending marker); changed pull (HEAD_SEQ before≠after) → marker contains `core-update` + done changed:true; dirty flow (patch file exists, stash push+pop in log); pull-fail → stash pop restore + PULL_FAILED; stash-pop conflict → checkout -f + reset --hard in log + both recovery warns; update-check shape (+ behind count via stubbed rev-list). Expect bats 391+9=400. Commit `feat(cli): update-check + server update (fail-closed, edit-preserving)`.
2. **Rust + api** (cargo 25 / vitest 41 / check 0-0). Commit `feat(launcher): server update commands`.
3. **UI + flag + smoke rows** (vitest 41 / check 0-0). Commit `feat(launcher): server update card (locked until smoke-tested)`.
