# T22 — The WotLK password-repair callback defaults the account scheme to AzerothCore

**Status:** MERGING (hand DONE `4079dfaf`, Codex ACCEPT; gate behind the merge on m910q)
**Filed:** 2026-09-10 15:06 CEST by the lead (Fable), from the owed Codex adversarial pass on T12 (`eea840b6`), one high finding, confirmed at the tip
**Hand:** Sonnet (one binding and its test), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/ui/controller_view.py` **only** at the WotLK `reset=` lambda (`:972` at `a8391aeb`; the `create=` binding two lines above is the pattern); `pylauncher/yulon/controller_wow_wotlk/accounts.py` **only** if `reset_own_password`'s default must go (T12's rule: an entry that declares no scheme is refused, not defaulted — consider making `scheme` required and letting mypy find every caller); the controller test file T12 used (name it). Not the three CMaNGOS trees (their wrappers already pass a scheme).
**Box:** none; unit only.

## The finding

`create` refuses an entry with no scheme through `wotlk_accounts.checked_scheme(entry.accounts.scheme, entry.id)`; the repair callback beside it calls `reset_own_password(sql, name, pw)` and inherits the writer's `scheme="azerothcore"` default. On a stale credential file (the 401 path) an entry with no declared scheme therefore writes AzerothCore's `salt`/`verifier` columns — a failure during recovery on a schema without them, or unusable credentials on one with look-alike columns. T12's own test exercises the create callbacks only, so the omission survives every mutation.

## What to build

- Bind `scheme=wotlk_accounts.checked_scheme(entry.accounts.scheme, entry.id)` on the reset callback, evaluated at call time as `create` does.
- Decide whether the writer keeps its default: T12's rule says no scheme is refused by name everywhere; if the default goes, mypy (`--checks` runs it three ways) lists every caller — fix them or say why each is right.
- TDD: a controller-level test that an entry with `scheme=None` cannot reach `reset_own_password` through the repair path (`channel_setup._reset` or whatever public path the 401 repair takes — find it, cite it) and gets the T12 refusal sentence; its mutation is the old lambda.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first; `YULON_TEST_BOX=m910q` while the Hyper-V host is down); the test red first. One commit, `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T22/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the binding as written (quoted), the default's fate, the test and its mutation, deviations, status DONE.

## Report (hand, Sonnet, 2026-09-10 15:24 CEST)

- `4079dfaf` on `hand-t22` (base `85853e17`); gate ALL GREEN on m910q (3937 passed, 6 skipped, mypy x3, ruff, black); 2 files +46/-1 (`controller_view.py` +12/-1, `test_controller_view.py` +35).
- Binding: `reset=lambda name, pw: wotlk_accounts.reset_own_password(sql, name, pw, scheme=wotlk_accounts.checked_scheme(entry.accounts.scheme, entry.id))`, evaluated at call time like `create=`.
- The writer's default kept: no production caller reaches it now (TBC passes `mangos_srp6`, Vanilla/Tortoise `scheme()`, WotLK both bindings `checked_scheme`); the bare form's only callers are six sites in `tests/test_accounts.py`, outside the file set, so the hand stopped there and said so.
- Test `test_an_entry_with_no_scheme_is_refused_by_the_repair_seam_too_not_defaulted` through `services.channel_setup._reset(...)` (T12's seam pattern) with `scheme=None`: RED first (`DID NOT RAISE NotImplementedError`), GREEN after; mutation (the old lambda) RED again, `__pycache__` purged both times.
- Deviations: none. Status DONE.

## Review (Codex adversarial, 2026-09-10 15:25 CEST) -- ACCEPT

"The repair callback now rejects an undeclared scheme before reaching the writer; the regression test fails under the old binding. No alternate production repair caller bypasses the guard, UI exceptions are displayed as failures, and remaining production wrappers pass explicit schemes." No findings.
