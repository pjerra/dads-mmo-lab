# T7 — The direct-SQL guard reaches the app's own buttons, and "Stop, then install" can actually be done

**Status:** OPEN
**Filed:** 2026-09-09 11:50 by the lead (Fable), from T2's live press
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/apply.py`, `pylauncher/yulon/install_wiring.py`, `pylauncher/yulon/docker.py` only if `start_database()`'s signature must change, `pylauncher/tests/test_apply.py`, `pylauncher/tests/test_install_wiring.py` (or the file those tests live in — name it), `pyplan/write-ledger.md` if a write site moves, and a NEW `pyplan/gates/8.7a-guard-wired-yulon-ubuntu2-2026-09-09/` for the live half. **Not** `controller_view.py` (T5 holds it), not `pyplan/checklist.md`.
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
