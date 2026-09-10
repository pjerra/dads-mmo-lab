# T22 — The WotLK password-repair callback defaults the account scheme to AzerothCore

**Status:** OPEN (waiting for a lane)
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
