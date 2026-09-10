# T27 — Activating a module's conf never recommends a restart, though the world reads it only after one

**Status:** CLOSED (merged `5981acb5`, 2026-09-11)
**Filed:** 2026-09-10 22:37 CEST by the lead (Fable), from T18's live half and its evidence reviewer
**Hand:** Sonnet (one rule and its tests), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/apply.py` **only** where `restart_recommended` is derived (`:2207-2212` at `c0386f9f`) and the report's sentence for it; its tests (name them). Not the Modules tab, not `module_source.py`.
**Box:** none; unit only. The live measurement is T18's folder.

## The fact (T18, `pyplan/gates/8.6-spec-takes-effect-yulon-ubuntu2-2026-09-10/`)

The activation of `mod-playerbots`' conf through the app's own seam reported `restart_recommended = False` (`03-activate.log:38`); the same folder shows the running world did not read the conf until the restart of step 04 (`03-activate.log:66`, `04-restart-and-list.log:27-32`: "Config::LoadFile: Failed open file" before, "Loading TalentSpecs" after). `apply.py:2207-2212` derives the flag from `build.restart`, `npcs`, direct SQL and `server_dbc` only, so no conf activation on any route ever recommends a restart: a user pressing the Modules tab's Install is told nothing further is needed, and the spec still cannot take effect.

## What to build

- A conf step that wrote or replaced a file the running world would read (`_conf()`'s `done: activate …` outcome, and a `set N key(s)` outcome) sets `restart_recommended`; a conf that was already identical (nothing written) does not. The report's sentence names the file and says the world reads it at its next start.
- TDD: an activation that writes -> recommended, with the file named; an activation that writes nothing -> not recommended; a module with no conf -> unchanged; the mutation for each (the clause dropped; the identical case folded into the written one).

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first); the tests red first. One commit, `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T27/`.

## Hand's report (Sonnet, 2026-09-11 04:05 CEST, `hand-t27` at `ccd26fb5`)

Gate `=== --checks: ALL GREEN ===` (4152 passed). `_Log.conf_restart` set only at the two sites in `_conf()` that write bytes (the template copy on activate, the key write), read into `restart_recommended` as a fifth clause of the OR; both `done` lines now end "— the world reads <file> at its next start". Three tests plus one rewritten (`test_a_manifest_can_declare_the_restart_it_needs` had pinned the bug: "the derivation cannot see a conf write"); mutations: the clause dropped → red on the two write tests; the identical case folded into the written one → red on the nothing-written test only. Diff +120/−22 in `apply.py` and `tests/test_apply.py`. No deviations. Status DONE. One Codex review running.

## Review, round 1 (Codex adversarial, 2026-09-11 04:20 CEST): needs-attention, one high

`apply.py:2162-2168` sets `conf_restart` for every nonempty `writes` list, and `_set_conf_key` writes on its replace path without comparing bytes, so an already-identical keyed conf still reports a write and recommends a restart; the no-write test covers only the template-copy skip, so the "identical folded into written" mutation is not isolated for the keyed path. Round 2 sent: report whether any key changed the file, set the flag and the sentence only then, a test for the identical keyed apply.

## Round 2 and the lead's check (2026-09-11 04:45 CEST)

`b5f2d00c` (the one commit amended): `_set_conf_key` answers `unchanged` when the substituted text equals the file's, the caller sets `conf_restart` and the sentence only when some key changed the file; new test `test_reapplying_an_identical_keyed_conf_does_not_recommend_a_restart`; three mutations red on their own tests, green restored; gate ALL GREEN (4153). Lead's check in the worktree: 136 apply tests green; the identical-key guard removed → exactly 1 failed; restored → 136. ACCEPT.

## Closed (lead, 2026-09-11 05:00 CEST)

Merged `--no-ff` as `5981acb5`; gate on m910q behind the merge `=== --checks: ALL GREEN ===`; pushed. Worktree and branch removed.
