# T21 — A failed group read after a genuine uninvite is reported as "removed"

**Status:** IN PROGRESS (Sonnet hand on `hand-t21` since 2026-09-10 15:17 CEST, gate box m910q)
**Filed:** 2026-09-10 15:05 CEST by the lead (Fable), from the T13 round-2 Opus review (note 5)
**Hand:** Sonnet (a parser-sized fix), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/party.py` **only** at `dismiss()`'s poll after the logout whisper (`:1356-1365` at `1a446392`) and `_rows_only` (`:1760`) if a sibling for the dismiss direction is needed; `pylauncher/tests/test_party.py`. Not `remove_all`, not `_mass_sentence`, not the Lua.
**Box:** none; unit only. The live contract is T13's live half.

## The finding (T13 review, note 5)

`_rows_only` turns a FAILED group read (the `str` answer) into `()`. For `add_bot`'s poll that is right and documented: a read that could not be done adds nothing, the press times out and says so. `dismiss()` reuses it in the other direction: its poll is looking for the bot's guid to be *gone*, and `()` — no rows — reads as gone, so `Dismissal(removed=True, ...)` and the panel says "<bot> left the party" on a read that never happened. T13 round 2 removed the Lua-silence route into this (an empty SOAP reply no longer reaches the poll); what remains is a genuine uninvite whose follow-up read errors (database down, the seam raising, a SQL refusal).

## What to build

- The dismiss poll distinguishes "read failed" from "no rows": a failed read neither confirms nor denies; keep polling until the deadline, and on expiry report the read's own words (`Dismissal(removed=False, logged_out=<as sent>, "<bot> may or may not have left: the group table could not be read (<the string>)")`) — never `removed=True` on a read that did not happen.
- TDD: a `members` callable that returns the failure string every time -> `removed is False` and the sentence carries the string; a callable that fails once then returns rows without the bot -> `removed is True` (a transient failure does not fail the press); the mutation for each (the old `_rows_only` path makes the first go red).
- `add_bot`'s use of `_rows_only` is untouched and its tests stay green.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first; `YULON_TEST_BOX=m910q` while the Hyper-V host is down); the tests red first. One commit, `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T21/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the poll rule as written (quoted), the sentence, the tests and their mutations, deviations, status DONE.
