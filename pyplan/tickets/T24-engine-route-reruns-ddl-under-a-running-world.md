# T24 — The engine's install route re-applies a flagged phase without proving the world is down

**Status:** CLOSED (merged `74536843`, gate ALL GREEN on m910q behind it, 2026-09-10 16:01 CEST; one round, the remedy fixed by the lead's hand; worktree and branch removed)
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

## Report (hand, Sonnet, 2026-09-10 15:56 CEST)

- `732292b4` on `hand-t24` (base `92ca092f`); gate ALL GREEN on m910q (3944 passed); 2 files +268/-10 (`families/cmangos.py` +86, `test_families_cmangos.py` +178/-9). `native.py` untouched: `self._seams.ask_world_running(container)` (T7's seam via `StagedInstaller`) was reachable from `cmangos.py` directly.
- The guard: `_refuse_rerun_into_a_running_world()` called immediately before `yield from self._rerun_on_marked(ctx, plan)` in `_import`'s finished-install branch; `False` returns, `None` raises `InstallerError` naming Docker first, `True` raises "…world server is running, and it holds these databases in memory and writes back over whatever it finds in them. Nothing was applied, nothing was imported and nothing was cleared. Press Stop, then press Install again — the database is started on its own for it, and the world server stays down until you start it."; a seam that raises is logged and read as `None`. `_only_the_rerunnable_phases` (T14's route) left alone, it carries T14's guard already.
- Tests: refuses before `up` on True/None through a full `run()` (no SQL, no `up`, no reset); the sentence; Docker named on None; proceeds on False; the seam raising -> refusal. Mutation "guard deleted": 5 red; "None widened to allow": 3 red. Nine existing tests that reach the branch now pin `world_running=a_world_that_is(False)` (two rebuild tests had started shelling to the real docker CLI and tripped the conftest guard). T14's one-probe test and the 25 in `test_database_updates.py` unaffected.
- Deviations: none. Status DONE.

## Review (Codex adversarial, 2026-09-10 15:57 CEST) -- REWORK, one

The guard is correctly placed. One medium: `cmangos.py:1376-1381` -- the route explicitly includes installs adopted through "Use existing…", whose catalog tile is greyed "Installed" (`catalog_view.py:432-439`), so "press Install again" cannot be followed there; the same unfollowable-remedy defect T14's reviewer caught. Recommendation: a reachable remedy (Stop, then the Modules tab's database-updates action, which is enabled exactly for the plans this route fires on), CLI guidance distinguished, and a test grounded in the remedy rather than the words "Install again". Closed by the lead's hand (one sentence and its test).

## Closed by the lead's hand (2026-09-10 15:58 CEST)

Both refusals now end "Press Stop on the Server tab, then apply these files with \"Apply pending database updates…\" on the Modules tab — it starts the database on its own and keeps the world server down — or run the install again from the command line", the label imported from `native.UPDATES_BUTTON_LABEL` so it cannot drift; the None arm the same after "check that Docker is running". The sentence test asserts the label and rejects "press Install again"; mutation (the old remedy back): 1 failed. `test_families_cmangos.py` 166 passed locally; gate on m910q behind the commit, then merge.
