# T19 — The updates button cannot reach a marker-less install, and prints the install's cancel note

**Status:** second half MERGED `e2a305c8` (2026-09-10 18:35 CEST; two rounds, the last finding closed by the lead's hand; the proper mutation, with the import kept, failed exactly the two intended tests); first half OPEN, the owner chose teaching the probe -- an Opus hand next, then the m910q press
**Filed:** 2026-09-10 15:14 CEST by the lead (Fable), from T14's live half (findings 1-2, confirmed by the evidence reviewer against the source)
**Hand:** Sonnet for the second half alone; Opus if the first half becomes a probe change (it touches the import gate every CMaNGOS install goes through). Worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** second half: `pylauncher/yulon/catalog/native.py` **only** at `update_stages()` (`:2165-2168` at `1f44a995`) and its test file. First half, once decided: `pylauncher/yulon/catalog/sqlplan.py` at `MarkerGate.probe` (`:1251-1279`) and `pylauncher/yulon/catalog/native.py` at `import_reads_as_finished` (`:401`), or a new adopt action beside the updates button in `controller_view.py` — not both; their tests; the T11 ticket's claim corrected by the lead, not the hand.
**Box:** `m910q` for the first half's press, only on the lead's word and the owner's; the install there is the owner's.

## The facts (T14 live, `pyplan/gates/tortoise-updates-button-m910q-2026-09-09/`)

1. **The second arm of `import_reads_as_finished` is dead through the CMaNGOS gate.** `MarkerGate.probe` returns `ImportState("populated", ...)` with `complete` at its default False (`sqlplan.py:1279`); `complete=True` is set on the `imported` branch alone (`:1265`). The owner's m910q Tortoise install has no `yulon_install` table in any schema while its state file records `import` completed, so it reads `populated` incomplete and the button refuses — the install the T10 -> T11 -> T14 chain was built for. T11's claim that the flagged phase reaches a shell-script install "which reads `populated`" is false on this gate. Two remedies, the owner's call: (a) the probe answers completeness on the `populated` branch from per-schema table counts it already asks elsewhere; (b) an explicit "Adopt this install as imported" action that writes the marker row with the owner's consent (a write nobody has consented to yet — the hand was right not to make it).
2. **The press prints `IMPORT_CANCEL_NOTE`** ("Databases left half-written are detected and cleared…"): `cmangos.py:247` builds `Stage("import", ..., cancel_note=IMPORT_CANCEL_NOTE)`, the spine yields any `cancel_note` after `--- <name>` (`native.py:2044-2046`), and `update_stages()` reuses the stage changing only `recorded`. The note is false on this route: `_only_the_rerunnable_phases` never calls `stage_import()`, so `gate.reset()` is unreachable.

## What to build

- Second half, now: `replace(stage_named("import"), recorded=False, cancel_note="")` at `native.py:2167`; a test pinning the panel's lines for an updates press (no cancel note); the mutation is the old `replace`.
- First half, after the owner picks (a) or (b): the chosen change with the tests its side needs — for (a) a probe test per schema-count answer and the T11 fixture green; for (b) the confirmation naming the row it writes and a refusal when the world is up; then the m910q press again, the same captures as T14's live half, and this time the two route lines.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first; `YULON_TEST_BOX=m910q` while the Hyper-V host is down); the tests red first. One commit per half, `Co-Authored-By: Claude <model> <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T19/`.

## Report, second half (hand, Sonnet, 2026-09-10 16:46 CEST)

- `3d674bea` on `hand-t19` (base `6b6db7ab`); gate ALL GREEN on m910q (3974 passed); 2 files +41/-1 (`native.py` +12/-1, `test_database_updates.py` +30).
- `replace(self.stage_named("import"), recorded=False, cancel_note="")` in `update_stages()`, the docstring extended with why.
- Test `test_an_updates_press_never_prints_the_import_stage_cancel_note`: `update_databases()` on a finished-import fixture, `IMPORT_CANCEL_NOTE` in no yielded line, `--- start-db` and `--- import` both present. RED against the old code; mutation (the old `replace` without `cancel_note=""`) RED; restored GREEN, 26 in the file; T14's one-probe test green.
- Deviations: none. Status DONE (second half).

## Review, second half (Codex adversarial, 2026-09-10 16:47 CEST) -- REWORK, one high

The stage heading is fixed; a Stop during the updates press is not: `_rerun_on_marked()` passes the cancel event into `sqlplan.apply()`, whose between-run check raises "The import was stopped. {IMPORT_CANCEL_NOTE}" -- the same clearing promise, and false here since `gate.reset()` is unreachable on this route: completed statements or a partially executed file stay and are not cleared before a retry. Clearing the stage note also removed the only advance warning of that cost. Must-fix: route-aware cancellation wording -- `sqlplan.apply()` takes the note to append (the install route keeps its clearing promise; the updates route says truthfully that finished statements stay and the flagged phase is re-run whole on the next press), and a test that sets the event between re-runnable SQL runs on the updates route and asserts `IMPORT_CANCEL_NOTE` is absent from the yielded lines AND the raised error, with the truthful sentence present.

## Rejection (lead, round 1)

File set widened to `sqlplan.apply()`'s cancellation wording (one parameter, the install route's default unchanged) and `_rerun_on_marked()`'s call into it in `families/cmangos.py`. Must-fix as the reviewer states it; keep the successful-press test. Consider giving the updates stage a truthful `cancel_note` of its own instead of the empty string, said once by the spine as every other note is. Add a commit (do not amend); gate; report in the same format. Round 2 is the last under the cap.

## Report, second half round 2 (hand, Sonnet, 2026-09-10 17:02 CEST)

- Second commit `ef3e2b78` on `hand-t19` (rebased onto `88d1b679`, round 1 now `fba814ec`); gate ALL GREEN on m910q (3975 passed); whole branch 4 files +171/-21 (`native.py` +42/-14, `sqlplan.py` +24/-7, `cmangos.py` +12, tests +114/-2).
- `sqlplan.apply(..., cancel_note=IMPORT_CANCEL_NOTE)`; `_check_cancel(cancel, note)` raises "The import was stopped. <note>"; `_rerun_on_marked()` passes `RERUN_CANCEL_NOTE` = "A stop here leaves the statements that already ran in place and clears nothing; the flagged phase is applied whole again the next time this is pressed." (the ordinary resume path and the button both funnel through it and the gate has already read a finished import, so `reset()` is unreachable from either); `_import()`'s fresh-import call keeps the default; `update_stages()` gives the stage that note.
- Tests: the successful press -- install note absent, rerun note exactly once, right after `--- import` (mutation: the stage note back to "" -> red); a Stop set from inside the exec seam after the first of two statements -- the error and every line carry the rerun note and never the install's, the second statement never ran (mutation: the `cancel_note=` dropped from the call -> red, the install note came back). The install route's own Stop test unchanged and green.
- Deviations: rebased instead of ff. Status DONE (second half, round 2).

## Owner's answer on the first half (2026-09-10 18:29 CEST, through the question tool): teach the probe

Remedy (a). Spec for the Opus hand, after the second half merges (same files): `MarkerGate.probe` answers `complete=True` on the `populated` branch when every schema in the plan reads as fully populated -- the per-schema table counts the gate already asks elsewhere, compared to the plan's own expected set (the tables its files create), never a hand-typed number; `partial` stays `partial`; a populated install with a table missing stays incomplete and says which. `import_reads_as_finished`'s second arm becomes reachable and T11's claim that the flagged phase reaches a shell-script install becomes true on this gate. Tests: a probe test per answer (marked; populated complete; populated with one table missing; partial; absent; unreadable) with the mutation for each; T11's rerun fixture and T14's one-probe test green. Then the m910q press again through the button, the same captures as T14's live half, this time reaching the two route lines, on the lead's word (the world stopped; the owner's install).

## Review, second half round 2 (Codex adversarial, 2026-09-10 18:32 CEST) -- REWORK, one high

The updates route is right. `cmangos.py:1201-1205` (and AzerothCore's tuple): the ordinary install's `import` stage still carries `IMPORT_CANCEL_NOTE`, said by the spine before `_import` probes; on a finished install the route is `_rerun_on_marked()`, which clears nothing, so the user reads the clearing promise up front and the contradicting re-run note on a stop. The tests cover `update_databases()` only.

## Closed by the lead's hand (round cap, 2026-09-10 18:32 CEST)

`IMPORT_STAGE_CANCEL_NOTE` -- "If these databases are half-written from an earlier stop, they are detected and cleared before the import is run again, so nothing has to be undone by hand; if they already read as a finished import, a stop leaves the statements that already ran in place and clears nothing, and the flagged phase is applied whole again next time." -- on both families' `import` stage; `IMPORT_CANCEL_NOTE` stays the fresh-import route's stop-time sentence, `RERUN_CANCEL_NOTE` the re-run's and the updates stage's. `test_a_finished_installs_ordinary_run_never_says_the_bare_clearing_promise`: through a full install on a finished gate, the stage note once, neither single-route sentence in any line; mutation (the stage note reverted): red. Gate on m910q behind the commit, then merge.
