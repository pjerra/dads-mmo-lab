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

## Report (hand, Sonnet, 2026-09-10 15:26 CEST)

- `e4514902` on `hand-t20` (base `85853e17`); gate ALL GREEN on m910q; 3 files +69/-2 (`docker.py` +15/-1, `test_docker.py` +13/-1, `test_controller_view.py` +43).
- The rule, inverted: only the terminal statuses `exited`, `dead`, `created` read as down; everything else -- `paused`, `removing`, an unknown word -- reads as running, so a status Docker adds later fails closed; `None` on an unreadable inspect unchanged. Docstring records the reason (Codex on `a6e2aff6`).
- Tests: the three-valued docker test flipped (`paused: True`, `dead: False`, `removing: True`, a future status `True`), RED first, mutation (the old two-word table) RED; `test_a_paused_world_refuses_direct_sql_on_the_wotlk_modules_tab` patches `container_state` (not `world_running`) so the real mapping runs through the shipped wiring into `applier.install()`: `ApplyError` with the 8.7a sentence, zero `DockerSql` calls; mutation RED (the un-refused guard fell through to `start_database` and the conftest docker-CLI guard failed the run).
- Other readers: the 8.7a applier guard on all four games and `native.py`'s updates guard -- affected as intended; Tortoise `autoupdate.GuardedApplier`'s restart-survivability check shares the same callable, so a paused Tortoise world now reads as up there too (side effect, outside the file set, flagged); My Party, observability, readiness polling and channel_setup read `container_state` fields or UI state, not affected.
- Deviations: none; the first commit lacked the trailer and was amended before the report, one commit on the branch. Status DONE.
