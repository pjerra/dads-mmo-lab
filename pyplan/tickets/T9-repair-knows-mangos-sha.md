# T9 — Repair cannot repair a Tortoise install: the shared password reset does not know `mangos_sha`

**Status:** OPEN
**Filed:** 2026-09-09 12:40 by the lead (Fable), from T4's live upgrade
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/controller_wow_wotlk/accounts.py` (`reset_own_password` and whatever it shares with `_account_row`), `pylauncher/yulon/controller_wow_tortoise/accounts.py` only if the binding must pass something new, `pylauncher/tests/test_accounts.py`, `pylauncher/tests/test_controller_wow_tortoise.py`. Not the UI, not `channel_setup.py`, not `pyplan/checklist.md`.
**Box:** none for the code half. The live proof belongs to a later press on the m910q's Tortoise install (now on the rebuilt image, stopped) when the lead asks.

## The fact (T4, verified by its cold reviewer on the tree)

`controller_wow_wotlk/accounts.py:176` declares `Scheme = Literal["azerothcore", "mangos_sha", "mangos_srp6"]`. `create_account`'s `_account_row` (`:484`) knows all three — `mangos_sha` writes `sha_pass_hash`. `reset_own_password` (`:440-445`) branches on `mangos_srp6` and treats every other scheme as AzerothCore `salt`/`verifier`. Tortoise's catalog entry is `accounts.scheme = "mangos_sha"` with `level_column = "rank"`; its binding passes `scheme()`. So the channel's Repair path — the thing that runs when the app's own credential is stale or lost — dies on this tree with `ERROR 1054 Unknown column 'salt'` (`pyplan/gates/tortoise-upgrade-m910q-2026-09-09/channel-ask.log`), and the only way to a working channel was to delete the app's account and recreate it. Written up in that folder's README as a product finding.

## What to build

- A `mangos_sha` branch in `reset_own_password` that writes `sha_pass_hash = SHA1(UPPER(name):UPPER(password))` through the same `fold()` the writer uses (`:204`), and **refuses unknown schemes** by name rather than defaulting to AzerothCore's columns — a default that reaches for the wrong table is the whole bug.
- TDD: the failing test first (a `_FakeSql` recording statements; `reset_own_password(..., scheme="mangos_sha")` must issue one `UPDATE account SET sha_pass_hash = … WHERE username = …` and no `salt`/`verifier`), its exact message recorded; a test that an unknown scheme raises `AccountError` naming it; the mutation each catches. The existing vectors in the module docstring (the pair of hashes a live Tortoise server logged 2026-08-26) are the ground for the hash.
- Note in the docstring, dated, that this tree reads the *rank* at startup (`pyplan/gates/tortoise-soap-yulon-arch-2026-09-09/`), so a reset password takes effect at once but a rank change does not — different columns, different rules.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first); the failing test passes; no caller's behaviour on AzerothCore or `mangos_srp6` changes (their tests untouched and green). One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the statement the new branch issues (quoted from the test), the tests and their mutations, deviations, status DONE.
