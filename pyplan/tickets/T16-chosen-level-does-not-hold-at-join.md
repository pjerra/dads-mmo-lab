# T16 — A chosen level sent at join time is accepted and does not hold

**Status:** OPEN (waiting for a lane)
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
