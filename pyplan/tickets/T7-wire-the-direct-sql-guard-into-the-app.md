# T7 — The direct-SQL guard reaches the app's own buttons, and "Stop, then install" can actually be done

**Status:** DONE, code half (hand reported 2026-09-09 16:48; awaiting review); live half queued for the box
**Filed:** 2026-09-09 11:50 by the lead (Fable), from T2's live press
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/apply.py`; the four factory sites T2's reviewer located — `pylauncher/yulon/controller_wow_wotlk/modules.py` (`applier()`, near line 205), `pylauncher/yulon/controller_wow_tbc/modules.py`, `pylauncher/yulon/controller_wow_vanilla/modules.py`, and `pylauncher/yulon/controller_wow_tortoise/autoupdate.py` (`GuardedApplier` sets the *public* `world_running` near line 461, a different attribute from the private one the guard reads); `pylauncher/yulon/ui/controller_view.py` **only** at the module-Applier construction near lines 919-924 (T5 owns `_build_my_party_group` and the `MyPartySeam` Protocol — do not touch those, and start only after the lead says T5's code half is merged); `pylauncher/yulon/docker.py` only if `start_database()`'s signature must change; their test files (name them); `pyplan/write-ledger.md` if a write site moves; and a NEW `pyplan/gates/8.7a-guard-wired-yulon-ubuntu2-2026-09-09/` for the live half. **Not** `install_wiring.py` — T2's reviewer read it: it holds no `Applier` and wires no seam of this kind; the one existing `world_running` wiring is My Party's at `controller_view.py:948`, fail-closed on a blank inspect, which is the shape to copy. Not `pyplan/checklist.md`.
**Box:** `yulon-ubuntu2` for the live half — **only when the lead says so** (T3 holds it now, T5's live half is also queued). Do the code half first and report DONE for it.

## Why

T2 pressed the guard (`apply.py::_refuse_direct_sql_into_a_running_world`, `f8cd55df`) against a running world and it held: refused, the step named, sixteen readings identical. Then two things it found (`pyplan/gates/8.7a-direct-sql-yulon-ubuntu2-2026-09-09/README.md`):

1. **The seam is unwired in the shipped app.** `Applier` takes `world_running` as a seam; the press wired it by hand. The app's own construction (`install_wiring.py`, and wherever the Modules tab builds its `Applier`) passes nothing, so `_world_running` is `None` and the guard never fires — *today's Modules tab would have written 7219 rows into a live world without a word*. Verify the exact site (`apply.py` around line 1920 per the hand; grep for `Applier(`).
2. **The refusal's own instruction cannot be followed.** It ends *"Press Stop, then install again."* Stop through the app brings the database down with the world, and a direct SQL step then fails with `container … is not running`. `docker.start_database()` exists and put the database back alone in 6.6 s. The route lacks a caller, not a primitive.

Exit clause 3 forbids a capability reachable only from a script; 8.7a's second clause is met by the engine and by no button until both of these are fixed.

## What to build

- **Wire the seam** where the app builds every `Applier`, reading the install's actual world state the way `install_wiring` already knows it (the ready/status seams the Server tab uses — reuse, do not re-derive). Every construction site, not one: enumerate them with a grep and say the count.
- **Make the stopped-world path work**: when a direct SQL step runs with the world down and the database down, start the database alone (`docker.start_database()`), run the step, and say so in the report text; do not start the world. If the world is *up*, refuse as now. If the database cannot be started, refuse with the daemon's sentence.
- **The refusal sentence** stays true: after this change "Press Stop, then install again" is a thing a user can do and have succeed.
- Tests, TDD, each naming the mutation it catches: an `Applier` built through the app's wiring refuses with the world up (the seam arrives, not a `None`); the stopped path starts the database and applies; the database-cannot-start path refuses; no construction site is left with `world_running=None` (a test that enumerates them, so the next site added is caught).
- `applied_by` defaults to `direct` — T2 counted 43 steps across 18 manifests on this path; your enumeration test should say the number it found.

## Live half (later, on the lead's word)

Repeat T2's press through the app's own surface: the refusal with the world up, then Stop, then install — and it succeeds this time with the database started alone — then remove and restore the six tables from a fresh dump as T2 did. Same rules as T2 for the box, the owner's things, and the realm row. README leads with whether 8.7a's second clause is now met **by a button**.

## Definition of done

- Code half: `--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first); the enumeration test green with its count stated.
- Live half: the three captures above, counts before and after, `test_no_secrets_in_evidence.py` green locally before any log is committed.
- One commit per half, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours.

## Report format (final message)

`## Report (code half)` — sha, gate last line, diff stat, the construction sites (file:line, count), the tests and their mutations, deviations, status DONE (code half).

## Report, code half (hand, 2026-09-09 16:48)

- sha `ef0cee36` on `worktree-agent-ae3ae26e084b062b7` (base `583a61ce`); gate ALL GREEN 3787; diff 15 files +991/-36.
- **13 construction sites wired**, found by an AST audit that asserts the count and what each site binds: the `Applier(...)` / `apply_module()` pairs in the TBC, Vanilla and WotLK factories, `GuardedApplier(...)` and the Tortoise pair, and **the four shipped Modules-tab appliers in `controller_view.py`** (`:937`, `:1224`, `:1382`, `:1503` -- four, not the one the ticket named). `world_running` is a required keyword-only parameter on all four factories; the audit also pins that each site binds `docker.world_running` (a site rewired to `.settled` is caught).
- The seam: new `docker.world_running(container, *, wsl_distro)`: blank status -> `None` (fails closed), `running`/`restarting` -> `True`, else `False`; deliberately not `.settled`. My Party's wiring untouched.
- Tortoise fixed at `autoupdate.py:458`: `GuardedApplier.__init__` forwards `world_running=` to `super().__init__`; `_guard()` narrows with `is True`.
- The stopped-world path: `Applier._start_the_database_for_direct_sql()` runs from `_sql()` after the refusal, only with a `start_database` seam, a runner and a direct step in this action; the view binds `docker.start_database(...)`, which starts the database alone; `start_database()` returns `bool` and the report says `started the database alone; the world server was left stopped` only when it did; a failure raises `ApplyError` with Docker's sentence before anything runs.
- Tests: 7 behavioural + 3 audit/census in `test_apply.py`, 2 in `test_docker.py`, 2 each in the TBC/Vanilla/Tortoise module tests, one call site in `test_catalog_view.py`; ten mutations caught (listed with their tests); one not caught and not real (`is True` vs `bool()` on `bool | None`). Census: 43 steps / 18 manifests / 4 games (the 44th direct step is `paragon.json`'s into `ale`).
- Deviations: four `controller_view.py` sites edited, not one; `docker.py` gained a function; one annotation widened in `native.py:1378`; one call site in `test_catalog_view.py`; the test through `ControllerServices.for_entry()` belongs in `test_controller_view.py` (T5's, now merged) -- proposed, not written; write-ledger untouched; six laptop-only pre-existing failures.
