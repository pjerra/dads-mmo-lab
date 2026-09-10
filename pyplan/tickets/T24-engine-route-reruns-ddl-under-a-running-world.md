# T24 — The engine's install route re-applies a flagged phase without proving the world is down

**Status:** OPEN (waiting for a lane; shares `families/cmangos.py` with nothing open today)
**Filed:** 2026-09-10 15:08 CEST by the lead (Fable), from the owed Codex adversarial pass on T11 (`c5e7d78c`+`d90eae50`); the same fact as T11's code-review note 3, which T14 closed on the button's route only
**Hand:** Sonnet (one interlock and its test; the primitives exist), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/catalog/families/cmangos.py` **only** at `_import`'s finished-install branch (`:1165-1169` at `d90eae50`; find it at the tip) and `_only_the_rerunnable_phases` if the check belongs in the shared spot; `pylauncher/yulon/catalog/native.py` only if the world reading must be handed in through `StageContext` (T14's `updates_only` is the shape; T7's `Seams.world_running` is the seam); their tests (name them). Not `controller_view.py`, not the button (T14 owns that route and it already refuses).
**Box:** none; unit only.

## The finding

On the ordinary install route, `_import` on an install the probe reads as finished goes straight to `_rerun_on_marked()`, which streams Tortoise's `character updates` DDL. Nothing before it proves this install's world is down: `start-db` starts the database alone, `up` comes afterwards. Reachable today through the CLI harness (`install_wiring.py:342`) and any path that runs `engine.run()` on a remembered or "Use existing..." folder whose world is running. The tests drive `_import()` with a recorder and cannot see the order.

## What to build

- Before `_rerun_on_marked()` on this route, read the world through the same three-valued seam T7 wired (`world_running`: `False` is the only allow value; `True` and `None` refuse) and refuse in the 8.7a sentence shape ("Press Stop, then …" / the daemon named on `None`), the databases untouched, nothing imported, nothing cleared. Do not stop the world on the user's behalf — a stop is its own consent (T7's rule).
- Reuse, do not re-derive: the reading and the sentence exist in T7's applier guard and T14's `update_databases`; call the same function or share it.
- TDD: a stage-order test from a stack whose world reads `True` asserting the recorder sees no SQL write and no `up`; the same for `None`; `False` proceeds as today; the mutation for each (the guard deleted; `None` widened to allow). Confirm T14's route still makes exactly one probe (its test pins that).

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first; `YULON_TEST_BOX=m910q` while the Hyper-V host is down); the tests red first. One commit, `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T24/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the guard as written (quoted), the sentence, the tests and their mutations, deviations, status DONE.
