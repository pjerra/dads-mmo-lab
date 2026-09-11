# T15 — The two Dockerfile refusals give advice that is true under Rebuild as well as Install

**Status:** CLOSED -- merged `b375b2d7`, suite green behind it, 2026-09-09 14:26 CEST; hand retired, worktree removed
**Filed:** 2026-09-09 12:55 CEST by the lead (Fable), from T8's reviewers (round 1 Fable note, round 2/3 item 4)
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/catalog/families/dockerfile.py` **only** at the two refusal sentences in `write()`'s verdict handling (`_Verdict.THEIRS` near :630-634, `_Verdict.UNREADABLE` near :635-640 on `yulon-phase8b` at `67eb7d70`; verify), the module docstring at `:11` that repeats the THEIRS sentence, and the test file that pins those sentences (find it: `git grep -n "Point the install at an empty folder" -- pylauncher/tests`). Nothing else.
**Box:** none.

## The fact (T8, three reviewers)

Since T8 (`67eb7d70`) a rebuild renders the recipe again through `dockerfile.write()`, so both refusals are reachable from the Server tab's Rebuild on a folder that is a working install. Their sentences were written for Install:

- THEIRS: "... so it was not touched and nothing was installed. **Point the install at an empty folder**, or move that file aside." — under a rebuild "point the install at an empty folder" means throw the server away.
- UNREADABLE: "... **Nothing was touched and nothing was installed.** Make that file readable, or move it aside." — "nothing was installed" is the install-only half; the remedy is fine.

`:617-621` is the rendered text's own marker check and is not reachable from a user's disk (T8's round-3 reviewer); leave it.

## What to build

Reword the two sentences so each is true on both paths, without a path parameter (the function does not know which button called it and should not learn to): the hand of T8 suggested `"Move that file aside and press again, or point a new install at an empty folder."` for THEIRS, and dropping "and nothing was installed" for UNREADABLE. Keep the sentences' shape (what was found, what was not done, what to do). Update the pinning test; a test that asserts neither sentence contains "point the install" or "nothing was installed" (the install-only phrases), naming the mutation (the old sentence restored).

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first). One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T15/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, both sentences quoted before and after, the test and its mutation, deviations, status DONE.

## Report (Sonnet hand, 2026-09-09 14:24 CEST) and the lead's read

`bd5a1788` on base `7cd6913f`; gate ALL GREEN (3893); 2 files +60/-10. THEIRS now: "{path} was not written by Yu'lon, so it was not touched. Move that file aside and press again, or point a new install at an empty folder."; UNREADABLE now: "... that is not a pass. Nothing was touched. Make that file readable, or move it aside."; the module docstring at `:10-14` and one test docstring updated. New `test_the_refusal_sentences_carry_no_install_only_advice` drives both refusals and asserts neither contains "point the install" or "nothing was installed"; RED against the old wording, GREEN after; mutation (old THEIRS sentence restored) red on that test alone. Trailer `Co-Authored-By: Claude Sonnet 5` (the hand is Sonnet). The lead read the diff (both sentences keep their shape; the assertions are the install-only phrases; no other test pinned the old text) and merged without a cold reviewer.
