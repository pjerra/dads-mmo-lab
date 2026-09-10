# T20 — A paused worldserver passes the direct-SQL guard

**Status:** IN PROGRESS (Sonnet hand on `hand-t20` since 2026-09-10 15:17 CEST, gate box m910q)
**Filed:** 2026-09-10 15:05 CEST by the lead (Fable), from the owed Codex adversarial pass on T7 round 2 (`a6e2aff6`), one high finding
**Hand:** Sonnet (a one-function fix and its tests), worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/docker.py` **only** at `world_running()` (`:2539-2564`) and its docstring; its test (`test_world_running_is_three_valued_and_an_unreadable_inspect_is_not_a_no`, find the file with `git grep -n`); one app-level test beside T7's button test in `pylauncher/tests/test_controller_view.py` (or the apply-seam test file T7 used — name it). Not `apply.py`, not `controller_view.py`.
**Box:** none; unit only.

## The finding (Codex, on `a6e2aff6`)

`world_running()` answers `False` for every non-empty container status except `running` and `restarting` (`docker.py:2564`), so Docker's `paused` reads as *down*. A paused worldserver keeps its database-backed state resident in memory and can be unpaused and write it over whatever the direct SQL changed. `False` is the guard's only allow value, so both readings in the applier's `_sql()` pass and the SQL reaches the database. The existing test pins `paused: False`, preserving the gap.

## What to build

- `paused` counts as running. Decide and document the rule: either name `paused` beside `running`/`restarting`, or invert the table so only the terminal statuses (`exited`, `dead`, `created`) read as down and everything else — `paused`, `removing`, an unknown word — reads as running. The second fails closed on a status Docker adds later; say in the docstring which was chosen and why. `None` for an unreadable inspect stays as it is.
- Flip the pinned expectation in the three-valued test and add `removing` and an unknown status if the second rule was chosen.
- One app-level test: a paused WotLK world reaches the guard through the same seam T7's button test uses and produces the refusal (`ApplyError`, the 8.7a sentence) with zero runner calls. Its mutation: the old table (`paused` -> `False`) must make it go red.
- Check `git grep -n "world_running\|container_state" pylauncher/yulon` for any other reader that treats `paused` as down for a different reason (observability's restart counter, My Party's group) and state in the report whether each is affected; do not change them.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on the gate box first; `YULON_TEST_BOX=m910q` while the Hyper-V host is down); the new tests red first; the mutation named. One commit, `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours. Scratch under `<scratchpad>/T20/`.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the rule as written (quoted), the tests and their mutations, the other readers and whether they are affected, deviations, status DONE.
