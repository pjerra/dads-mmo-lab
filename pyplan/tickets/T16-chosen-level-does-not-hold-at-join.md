# T16 — A chosen level sent at join time is accepted and does not hold

**Status:** IN PROGRESS (Opus hand on `hand-t16` since 2026-09-10 22:28 CEST, live measurement first on `yulon-ubuntu2`)
**Filed:** 2026-09-09 13:50 CEST by the lead (Fable), from T5's live half (finding, not fixed) and its reviewer
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/party.py` **only** at `add_bot()`'s join poll and the level step that follows it (T5's `_level_note`, the `set_level` call and its `characters.level` readback), `pylauncher/yulon/play.py` only if `InstallPlay.set_level` must learn a readback-and-resend, `pylauncher/tests/test_party.py`, and a NEW `pyplan/gates/8.6-level-holds-yulon-ubuntu2-2026-09-09/` for the live half. Not `party_panel.py`, not the Lua bridge (T13's), not `pyplan/checklist.md`. **Start only after T5's live half and T13 have merged** (both edit `party.py`/`test_party.py`).
**Box:** `yulon-ubuntu2` for the live half, only on the lead's word (T7, then T13 hold it).

## The measurement (T5 live, `pyplan/gates/8.6-spec-level-dismiss-yulon-ubuntu2-2026-09-09/`)

Three panel presses with a chosen level (42, 42, 55): the server answers `You changed level of <bot> to N.` and `characters.level` stays 1 — after `.saveall` too. The same command sent by hand to a bot that joined ~25 s earlier holds (37, 42). `autogear` cleared as the culprit. The readback itself works (the panel lists `Bafossan — level 42` from `characters.level`). The reviewer: a supported inference, not a measured cause — early fails ×3, late holds ×2; the mechanism (playerbots finishing the character after the group row appears, and overwriting the level) cites no module source.

Today `party.py` says so in the panel's own words ("did NOT take") rather than claiming otherwise. This ticket makes the level hold, or measures why it cannot.

## What to build

1. **Measure first, on the box (live half comes first here, on the lead's word)**: with one bot, send the level at +0, +5, +10, +20, +30 s after the group row appears, read `characters.level` 10 s after each, and read the module's own source for what runs after a bot joins (the playerbots repo pinned by the catalog: `RandomPlayerbotMgr`/`PlayerbotFactory` — find where the level is set on a fresh bot; cite file and line from the pinned revision). The README says what the window is and what closes it.
2. **Then the code**: whatever the measurement supports — a readback-and-resend on the join poll's machinery (send, read `characters.level` after the module's own step, resend once if it moved back), or a wait for the marker the module writes when the character is finished, or, if nothing closes it, a refusal that says the level cannot be set at join and offers it as a separate press. The panel's sentence stays true either way.
3. Tests, TDD, each naming its mutation: the resend happens once and only when the readback disagrees; the readback is `characters.level`, not the command's reply; the "did NOT take" sentence appears when the resend also fails.

## Definition of done

Live half first: the timing table and the source citation. Code half: `--checks` ALL GREEN; then a second press on the box showing the level holding (or the refusal). Commits per half, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours; own account and character only, erased after; realm row and authserver as every ticket. Scratch under `<scratchpad>/T16/`.

## Report format

`## Report` — sha, gate last line, diff stat, the timing table, the source citation, what closes the window, the tests and their mutations, deviations, status DONE.

## Hand's report (Opus, 2026-09-11 00:40 CEST, `hand-t16` at `c416827d`; code half `e241fc95`)

Gate `=== --checks: ALL GREEN ===` (4099 passed, one xdist flake in `test_catalog_view.py` named, not smoothed). Finding: AzerothCore's online arm of `.character level` writes no row (`01b-source.log:15-45`, `PlayerSaveInterval = 900000`), so T5 read the last save. Seven fresh-join trials at level 42 (delays 0-30 s, by hand and through the app's own `set_level` seam): the live level moves within one sample and never moves back; `characters.level` moves only after `saveall`, agreeing 2.0-9.2 s later. Fix `party._level_step`: send, `saveall`, read the row, resend once on disagreement, `LEVEL_TRIES = 30` / `LEVEL_SLEEP = 1.0`. Six named mutations red/green (`12-mutations.txt`). Second press live: 8 → 42 in 3.2 s, the "already" arm 0.4 s with nothing sent, 5 → 55 in 4.4 s. Evidence `pyplan/gates/8.6-level-holds-yulon-ubuntu2-2026-09-10/`. Deviations: the panel's Add cannot be pressed on the box (`dml_addclass` needs a non-bot session; joins staged by a real core invite, the press is `_level_step` itself); a harness loaded and removed with a restart; sixteen random bots levelled and re-randomised; one T5 test rewritten (it pinned the overturned sentence). Box left clean (500 bots, T18's conf untouched, owner's things identical, no orphans). Status DONE. Sent to a cold Opus review.

## Cold review, round 1 (Opus, 2026-09-11 00:55 CEST): REWORK

Verified: every README sentence chased has a capture; all seven timing rows match the trial files; the AzerothCore citation is verbatim (online arm writes no row; `SaveAllPlayers` walks every `Player`, bots included); four of six mutations re-applied red then green; full suite 4097 passed / 31 skipped in the worktree; the rewritten T5 test is stronger; no secrets; box left clean. Must-fixes: (1) `party.py:1311` the save's `Answer` is never read — a refused or indeterminate `saveall` still yields `saved=True`, 30 s of polling, a resend and a false sentence; check `.outcome` like every other send, a refused save is a `problem`; (2) the pressed `party.py` (md5 `fca7d004…` in `04-second-press.log:11`) is not the committed one (md5 `7e4562af…`), and `12-mutations.txt` hashes a third — name the post-press docstring edits and print the committed hash; (3) README:369 claims a read-back of absent directories that `07-cleanup.log` does not hold. Notes: measured saveall→agree deltas are 1.4-8.2 s (README derives 2.0-9.2); worst case is ~60 s not thirty; the "already" arm wins even when the written row disagrees; the addclass factory branch (`PlayerbotMgr.cpp:593-594`) is read, not exercised — a bounded open question, since the box cannot press the panel's Add. DoD for "the level holds at join": met at the seam and for the mechanism; the panel's own Add on an addclass bot is argued from source. Round 2 sent to the hand.
