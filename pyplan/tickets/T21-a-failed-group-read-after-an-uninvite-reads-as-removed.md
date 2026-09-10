# T21 — A failed group read after a genuine uninvite is reported as "removed"

**Status:** REWORK round 1 of 2 (rejected by the lead 2026-09-10 15:29 CEST); the file set widened to `remove_all` and `_mass_sentence`
**Filed:** 2026-09-10 15:05 CEST by the lead (Fable), from the T13 round-2 Opus review (note 5)
**Hand:** Sonnet (a parser-sized fix), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/party.py` **only** at `dismiss()`'s poll after the logout whisper (`:1356-1365` at `1a446392`) and `_rows_only` (`:1760`) if a sibling for the dismiss direction is needed; `pylauncher/tests/test_party.py`. Round 2 widens this to `remove_all` and `_mass_sentence` (the round-1 file set walled off the batch path the finding lives on); still not the Lua, not the panel.
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

## Report (hand, Sonnet, 2026-09-10 15:27 CEST)

- `80bd2998` on `hand-t21` (base `78db5c87` after the ff-merge); gate ALL GREEN on m910q (3938 passed, 6 skipped, mypy x3, ruff, black); 2 files +85/-4 (`party.py` +50/-4, `test_party.py` +39).
- The poll rule: "`removed` is the group table read AFTER, never the uninvite's own `yes`... A failed read here neither confirms nor denies, so it keeps the poll going the same as an unchanged row would; only the LAST read decides what expiry reports." `dismiss()`'s `members` widened to `tuple[Member, ...] | str`; `InstallParty.remove` hands it the unfolded `self.members(master)`; `add_bot` and `remove_all` untouched, still through `_rows_only`.
- The sentence: `"<bot> may or may not have left: the group table could not be read (<the string>)"`, `removed=False`.
- Tests: `test_a_group_read_that_keeps_failing_is_reported_as_unknown_not_removed` -- mutation (fold each read through `_rows_only`, the old path) RED with `removed=True`; `test_a_group_read_that_fails_once_does_not_fail_the_dismiss` -- mutation (`continue` -> `break`) RED; both RED first against the old code.
- Deviations: none. Status DONE.

## Review (Codex adversarial, 2026-09-10 15:29 CEST) -- REWORK, two

1. **high** `party.py:1763-1768`: `InstallParty.remove_all` still passes every post-uninvite read through `_rows_only`; a failure string becomes `()` and `dismiss()` reports `removed=True` for every bot in the batch during a database outage -- the false success T21 exists to delete, on the batch route. The new tests call `dismiss()` directly, so folding `remove` back through `_rows_only`, or leaving the batch path, survives them.
2. **medium** `party.py:1465-1477`: `_mass_sentence` treats every `removed=False` as "stayed"; an unknown outcome ("may or may not have left") is summarised as "None ... left" / "Still here", asserting what no read established.
Confirmed right: the panel's per-row Remove goes through `InstallParty.remove` with the unfolded answer; `add_bot` stays on `_rows_only`; `dismiss()` keeps polling after a transient failure.

## Rejection (lead, round 1)

The ticket's file set caused finding 1; it is widened above. Must-fixes: (1) `remove_all` (and `dismiss_all`'s `members` callable) hands `dismiss()` the unfolded `self.members(master)` -- no `_rows_only` anywhere on the dismiss direction; seam-level tests for BOTH `remove` and `remove_all` with every post-uninvite read failing, asserting no `Dismissal` has `removed=True`, each with the mutation (the fold restored). (2) `_mass_sentence` says an unknown outcome as unknown: three buckets (left / stayed / could not be read), the sentence naming each that is non-empty, never "still here" for a bot whose table was never read; tests for all-unknown and for a mixed batch, with the mutation (unknown folded into stayed). Add a commit (do not amend); gate; report in the same format. Round 2 is the last under the cap.

## Report, round 2 (hand, Sonnet, 2026-09-10 15:44 CEST)

- Second commit `10e36d1c` on `hand-t21` (round 1 rebased to `c6d98caf` on the moved tip; ff was no longer possible); gate ALL GREEN on m910q (3944 passed); this commit 2 files +189/-12 (`party.py` +65/-12... net, `test_party.py` +136).
- Must-fix 1: `remove` and `remove_all` (and `dismiss_all`'s widened `members`) hand `dismiss()` the unfolded answer; no `_rows_only` on the dismiss direction. Must-fix 2: `Dismissal.unreadable: bool = False` (a structural signal, not sentence sniffing); `_mass_sentence` buckets left / stayed / could not be confirmed, the third rendered as "Could not be confirmed — <bot>: <sentence>; ..." beside "Still here — ..." when both are non-empty.
- Tests, each RED first and each mutation RED: `test_the_seam_does_not_report_removed_when_every_poll_read_fails` (fold restored on `remove`), `test_the_batch_seam_does_not_report_removed_when_every_poll_read_fails` (fold restored on `remove_all`), `test_a_mass_dismissal_where_every_bot_is_unreadable_says_so_not_stayed` and `test_a_mixed_mass_dismissal_names_each_bucket_in_its_own_words` (unknown folded into stayed).
- Deviations: rebased instead of ff; the new field; the seam tests silence the poll's wait through `dismiss.__kwdefaults__["sleep"]` / `dismiss_all.__kwdefaults__["sleep"]` because `InstallParty.remove`/`remove_all` expose no `sleep`. Status DONE.
