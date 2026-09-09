# T11 — A Tortoise install that already carries an import marker gets the character updates too

**Status:** OPEN
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
