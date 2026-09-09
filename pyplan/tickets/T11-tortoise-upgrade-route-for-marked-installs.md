# T11 — A Tortoise install that already carries an import marker gets the character updates too

**Status:** DONE (hand reported 2026-09-09 12:33 CEST, two commits; awaiting review)
**Filed:** 2026-09-09 17:00 by the lead (Fable), from T10's Codex review (its [high]) and the Fable reviewer's note 2
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/families/cmangos.py` (the `_import` path and `MarkerGate`, or a new stage beside them), `pylauncher/yulon/catalog/catalog.py` only if a phase needs a "re-run on an established install" attribute, `pylauncher/tests/test_composegen.py` and the file where `MarkerGate.probe`'s table is pinned (name it). Not `catalog.json`'s phase list (T10 landed it), not the UI, not `pyplan/checklist.md`.
**Box:** none for the code half. The live proof is the m910q's Tortoise install, which had the three files applied by hand on 2026-09-09; a later press on the lead's word, and only a press that proves the route is a no-op there (the files are idempotent) and would have applied them on an install that lacks them.

## The fact (T10, both reviewers)

`MarkerGate.probe` reads any marker row — whatever its plan hash — as `imported`, and `_import` then skips every phase. That is a documented decision (`catalog.py:540`, phase7-decisions "Probe"): a plan whose hash moved does not re-import a database that already has data. So T10's new `character updates` phase reaches fresh installs only. An established Tortoise install that was never hand-patched is still one honor-maintenance day from the `TRUNCATE character_inventory_copy` restart loop (`pyplan/gates/7.9-rerun-m910q-2026-09-09/README.md`, finding 1), and there is no button in the app that applies those three files to it.

## What to decide, then build

The phase is idempotent by construction (T10 read each file: `ADD INDEX IF NOT EXISTS`, an absolute column type, `CREATE TABLE IF NOT EXISTS`), so re-running it on a marked install is safe. Two honest shapes; pick one, say why:

1. **A per-phase flag the runner honours on a marked install** — a phase declared re-runnable (`"rerun_on_marked": true` or the attribute name the model prefers) is applied by `_import` even when the probe says `imported`, before the world starts, every time; other phases keep the marker rule. Cheap; the phase's own notes already argue its idempotence.
2. **A Repair action that applies the outstanding phases** — a button-shaped route, but there is no surface to hang it on in this ticket's file set; if you pick this, describe the surface for the lead and build the seam only.

Either way: TDD, the failing test first (a marked install; today `_import` applies nothing; after, the flagged phase applies and the others do not), the mutation each test catches, the marker table's documented rows still true for every unflagged phase.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first); the failing test passes; the marker rule unchanged for unflagged phases. One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch files under `<scratchpad>/T11/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, which shape and why, what a marked install now does at start, the tests and their mutations, deviations, status DONE.

## Report (hand, 2026-09-09 12:27 CEST)

- Base `60f12b62`; one commit `c5e7d78c` on `worktree-agent-a57f3944a63a61195`; gate ALL GREEN (yulon-fedora, 3846 passed); diff 3 files +314/-1 (`test_families_cmangos.py` +190, `catalog/families/cmangos.py` +109/-1, `catalog.py` +16).
- **Shape 1**: `SqlPhase.rerun_on_marked: bool = False`. `MarkerGate` untouched; the probe's five-branch table and its pin (`tests/test_sqlplan.py` J.6) unchanged. `_import` still runs the spine's table and returns without importing; before returning it calls `_rerun_on_marked()`, which applies only the flagged phases on both answers the family treats as finished (`imported` of any hash, and `populated`+complete -- an install made by the shell scripts has no marker row). Writes no marker, re-asks no verify rule, does not reach phase 0 (`create_schemas()` would rewrite `CREATE USER ... IDENTIFIED BY`), does ask `assert_update_level` for the runs it applied; a rejected run raises `sqlplan.apply()`'s sentence before `up`. Two log lines.
- Tests (six, mutations applied and reverted): the route reached on `imported` and on `populated`+complete (the call deleted); unflagged phases still left alone (the filter widened to `phase.name is not None`, which also reds the two older marker-rule tests); no marker written, no verify re-asked (`write_marker` appended); fresh install applies once, not twice (the call hoisted); a refused run stops before the world starts (`InstallerError` softened); update level still asked (`check_update_levels` replaced). RED first recorded.
- Rust: no `main-rust` tree; `origin/rust-main` is AzerothCore-only, no `character_updates`; its analogue is `ac-db-import`'s `updates` ledger plus `modmgr`'s "N SQL file(s) not yet applied" advisory; nothing to lift.
- Deviations: (1) **the flag is armed on no entry** -- `catalog.json` was outside the set, so `wow-tortoise`'s `character updates` phase does not carry `"rerun_on_marked": true` and the route is reachable only by tests; the lead's call. (2) The ticket's paths were wrong: the family is `pylauncher/yulon/catalog/families/cmangos.py`; `test_composegen.py` holds nothing about `_import`, the tests went to `test_families_cmangos.py`; the probe pin is `test_sqlplan.py` J.6. (3) Scope beyond the sketch: `populated`+complete covered; `check_update_levels` asked. (4) `plan_hash` shifts for every entry (a new model field), harmless by the probe's own rule. (5) No box work; the m910q press is the lead's.

## Lead's decision (2026-09-09 12:27 CEST)

Arm it. The ticket's "not `catalog.json`'s phase list" meant do not add or reorder phases; the one key inside the phase T10 landed is the point of the ticket. File set widened to that key; the hand adds a second commit.

## Report, second commit (hand, 2026-09-09 12:33 CEST)

- `d90eae50` on top of `c5e7d78c`: `"rerun_on_marked": true` beside `"on_error": "fail"` on `wow-tortoise`'s `character updates` phase, one sentence appended to note 4 (the flag rests on the 2026-09-09 reading of the three files and nothing else); nothing else in `catalog.json`. Gate ALL GREEN (3847 passed). Both commits 5 files +353/-2 (`test_tortoise_boot_facts.py` +37 new).
- `test_this_is_the_only_phase_in_the_catalog_that_runs_on_an_install_already_imported` enumerates the catalog: `{(entry.id, phase.name) ... if phase.rerun_on_marked} == {("wow-tortoise", "character updates")}`; two mutations (the key deleted; the key moved to `realm row`) each red alone; the docstring says what makes a phase eligible.
- For Tortoise concretely: every install press now streams `character_updates/*.sql` into `tw_char` in name order, on a 903-character install as on a fresh one.
