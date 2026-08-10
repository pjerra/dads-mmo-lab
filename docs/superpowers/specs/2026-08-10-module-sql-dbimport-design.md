# Round 5.9 — module SQL auto-apply: rebuild builds `ac-db-import` too

**Date:** 2026-08-10
**Status:** approved (user picked the rebuild-builds-db-import approach over
the modules-mount alternative and "both")
**Branch:** feat/core-family
**Origin:** found live on the VM 2026-08-10 (roadmap Round 5.9); diagnosis in
that roadmap entry and the root `CLAUDE.md` module-rebuild-round section.

## Problem

Module SQL auto-applies only via the `ac-db-import` one-shot service, whose
image is frozen at install time. Proof from the VM's ledgers: mod-playerbots
(baked in at install) has its SQL recorded across all three DBs
(`Updates.AllowedModules = "all"` works); NONE of the nine modules installed
afterwards has a single ledger entry. The rebuild arm — including the
2026-08-09 fixed one — builds only the `ac-worldserver` target, so db-import
never learns about new modules' SQL, and the worldserver's own updater
demonstrably does not fill the gap (it boots with `./modules` mounted and
records nothing). Result: every post-install C++ module runs with its SQL
missing or hand-applied outside the ledger — the class that produced 19
mod-city-bots stage-cast bots with invalid race/class pairs.

## Decision

**The native rebuild builds `ac-db-import` alongside `ac-worldserver`.**
SQL and compiled code land together at the next rebuild — the moment a C++
module needs anyway. The rejected alternative (mounting `./modules` into
db-import) would apply SQL for modules whose C++ is not compiled yet and
touches the generated compose in both the install and migrate engines; the
chosen shape changes one argv and nothing structural.

## Design

### 1. Build step gains the second target (Rust native arm only)

`modmgr::module_rebuild_stream_with`'s build step changes from
`compose <resolved -f set> build ac-worldserver` to
`compose <resolved -f set> build ac-worldserver ac-db-import`.

- The db-import target shares almost all layers with the worldserver build;
  the added wall-clock is small.
- `BuildProgress` needs no change: the largest-total-vertex rule already
  handles parallel image builds (recorded pct lesson).
- The subsequent plain `compose up -d` recreates db-import (image changed),
  compose's `depends_on: service_completed_successfully` ordering runs it
  before the worldserver starts, and the updater applies + ledgers every
  module SQL file it has not seen.
- **No bash change** — WSL-era composes carry `build:` for all services in
  the base file, so bash's `up -d --build` already rebuilds db-import there.
  Recorded one-sided exception, same shape as the overlay-build asymmetry.

### 2. Post-rebuild advisory: unledgered module SQL

After a successful build + up, the rebuild arm compares each installed
module's shipped SQL files against the update ledgers and emits one `warn`
line per module whose SQL is still unrecorded:

- Disk side: for each `modules/<key>/data/sql/<db>/updates/*.sql` (the
  AzerothCore module convention; also accept the `db_world`-style underscore
  spelling if present), collect (db, filename).
- DB side: `SELECT name FROM <schema>.updates` per resolved schema (reuse
  the existing native DB reader; schemas from the resolved names, the Task-6
  seam — never guessed literals).
- The comparison is a PURE function (files × ledger rows → missing list)
  with its own unit tests; the wiring is best-effort and tri-state: if the
  DB cannot answer, emit the existing-style warn ("could not read the update
  ledger -- skipping the module-SQL check.") and never fail the rebuild.
- Advisory copy names the module and the count, e.g.
  `mod-ah-bot-plus: 3 SQL file(s) not yet applied by the updater -- they
  land on the next rebuild + restart.` After THIS round's change the list
  should be empty right after a rebuild; a non-empty answer is the early
  smoke-signal this round exists to give.

### 3. Retroactive healing + the hand-applied caveat (documented behavior)

The first rebuild after this lands bakes ALL currently installed modules'
SQL into db-import, so the backlog (ah-bot-plus, autobalance, custom-login,
junk-to-gold, npc-enchanter, quest-loot-party, multibot-bridge, transmog,
mod-city-bots' full series) applies and ledgers in one pass.

Files previously applied BY HAND are not in the ledger, so the updater runs
them once more. Re-runnable files (`CREATE TABLE IF NOT EXISTS`,
DELETE-then-INSERT, absolute UPDATEs — city-bots' and transmog's shape)
settle idempotently. A non-re-runnable file makes db-import stop with an
error naming the file and the worldserver waits — visible, named, and
covered by the rebuild's safety backup. This is expected behavior, not a
bug; the remedy is a one-liner (ledger the file as applied, or clear the
duplicate rows).

**Rollout step (user-supervised, recorded):** the first post-fix rebuild on
the VM is watched live; any tripping file is resolved on the spot
(realistically zero to two files).

## Error handling

- Build failure: unchanged `BUILD_FAILED` shape (`rebuild.log` pointer) —
  now also covers a db-import target build failure.
- db-import runtime failure on next start: surfaced by the existing start
  path (worldserver never becomes ready; db-import's error names the SQL
  file). No new error codes.
- Advisory check: tri-state; a ledger that cannot be read warns and skips.
  The check NEVER changes the rebuild's outcome.

## Testing

- Fake-docker argv assertion extends to require BOTH targets in the build
  call (`build ac-worldserver ac-db-import` after the `-f` set); mutation:
  dropping `ac-db-import` from the argv goes red while the other rebuild
  tests stay green.
- Existing rebuild tests updated for the new argv; ordering assertions
  (config → stop → build → up) unchanged.
- Advisory classification: pure-function tests — missing files reported,
  ledgered files not, underscore/hyphen dir spellings, empty module list,
  module with no sql dir.
- Tri-state wiring: ledger-unreadable → warn line, rebuild outcome
  unchanged.

## Docs

- `docs/cli-contract.md` + `cli/README.md`: rebuild description gains the
  second build target and the advisory line vocabulary.
- `crates/CLAUDE.md`: one-liner on the db-import build + why (ledger
  evidence from the VM).
- Roadmap Round 5.9 marked spec'd → in progress.

## Out of scope

- The modules-mount alternative (rejected; revisit only if a real need for
  SQL-before-rebuild appears).
- Backfilling the VM's ledger for hand-applied files ahead of time (the
  supervised first rebuild handles it).
- WSL/bash arm changes (already covered by its base-compose `build:`).
