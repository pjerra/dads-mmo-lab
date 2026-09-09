# T12 — No account path defaults an unrecognised scheme to AzerothCore's columns

**Status:** OPEN
**Filed:** 2026-09-09 17:00 by the lead (Fable), from T9's hand (finding 3) and its two reviewers
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/controller_wow_wotlk/accounts.py`, `pylauncher/tests/test_accounts.py`, and `pylauncher/yulon/ui/controller_view.py` **only** at the two `create_account` call sites that pass `scheme=entry.accounts.scheme or "azerothcore"` (T9's reviewer: near lines 886 and 996 on `yulon-phase8b`; verify) plus the test that pins them. Not the Tortoise binding, not `pyplan/checklist.md`.
**Box:** none.

## The fact (T9, both reviewers)

T9 removed the fall-through in `reset_own_password` — an unrecognised scheme is refused by name. The same shape survives in three more places in the same module: `_account_row`'s final `else` (writes `salt, verifier, expansion, reg_mail, email`), `_grant_gm` and `_gm_level` (test `scheme in ("mangos_sha", "mangos_srp6")` and otherwise go to `account_access`). And on the UI side, two `create_account` call sites pass `scheme=entry.accounts.scheme or "azerothcore"` — the same default, for an entry whose scheme is `None` (the Tortoise binding refuses that case; the shared UI path does not).

Both reviewers read these as **not live for validated catalog data**: `catalog.py` types `accounts.scheme` as `Literal["azerothcore","mangos_sha","mangos_srp6"] | None`, so no shipped entry can carry a fourth string. The route to the bug is a code change adding a fourth `Scheme` value without adding branches — on a mangos-shaped table the create dies loudly with `Unknown column`, on a table that happens to have `salt`/`verifier` it writes a row that never authenticates. Low priority; the same defect class T9 fixed, closed everywhere it appears.

Also from T9's reviewer: the `Known: azerothcore, mangos_sha, mangos_srp6` list in the refusal message is hand-typed and the test pins it verbatim, so a fourth `Scheme` value leaves the message stale with no test noticing — `typing.get_args(Scheme)` keeps it honest.

## What to build

- Explicit branches in `_account_row`, `_grant_gm` and `_gm_level`, each ending in a refusal that names the scheme and the known list; the known list derived from `Scheme` once and shared with `reset_own_password`'s message.
- The two UI call sites: an entry whose `scheme` is `None` is refused with the app's sentence (which the Tortoise binding already has — read `controller_wow_tortoise/accounts.py` for the wording), not defaulted.
- TDD: for each of the three functions, one test with a `_Recorder` and `scheme="mangos_srp7"` asserting `AccountError` naming it and `statements == []`; a test that the known list equals `get_args(Scheme)`; the mutation each catches. AzerothCore, `mangos_sha` and `mangos_srp6` behaviour unchanged, their tests untouched.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first). One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch files under `<scratchpad>/T12/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the three refusals quoted from their tests, the UI sentence, deviations, status DONE.
