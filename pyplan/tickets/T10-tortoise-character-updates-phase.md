# T10 — The Tortoise catalog applies the fork's `character_updates/` directory, so an install stops dying on a calendar

**Status:** OPEN
**Filed:** 2026-09-09 12:45 by the lead (Fable), from the 07:30 crash on the m910q and T4's confirmation
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pylauncher/yulon/catalog/catalog.json` (the `wow-tortoise` entry's `sql.phases` only), `pylauncher/yulon/catalog/catalog.py` only if the phase shape needs a field it lacks, `pylauncher/yulon/families/cmangos.py` only if the phase runner cannot apply a directory in name order, `pylauncher/tests/test_catalog_operations.py`, `pylauncher/tests/test_tortoise_boot_facts.py` (or the file that pins this entry's phases — name it), `pylauncher/tests/test_composegen.py` if the runner changes. Not `pyplan/checklist.md`.
**Box:** none for the code half. The live proof is a fresh Tortoise install on `yulon-arch` (the last fresh install there needed these files applied by hand, `pyplan/gates/tortoise-soap-yulon-arch-2026-09-09/README.md`) — a later press on the lead's word.

## The facts, all measured

- **A fresh install crash-loops without them.** On `yulon-arch` on 2026-09-08 the world crash-looped on character migration `20260903211500` until `src/tortoise-wow/sql/character_updates/*.sql` were applied into `tw_char` by hand — `20260708055500` first, for the index the later one needs (`tortoise-soap-yulon-arch-2026-09-09/README.md`, "a fresh install of this fork does not come up without hand-applied character SQL").
- **An existing install dies on a calendar without one of them.** On 2026-09-09 at 07:xx the m910q's Tortoise world entered a restart loop on `TRUNCATE character_inventory_copy`: honor maintenance fell due for the first time (`saved_variables.nextHonorMaintenanceDay` 20705 = that day) and the table its code truncates is created only by `character_updates/20260812142512_character_inventory_copy.sql`, whose own comment describes exactly this morning ("runs fine for days and then dies one morning … a restart loop"). Applied by hand; the world came up and wrote the marker forward.
- **The catalog has no phase for the directory.** `catalog.json:1113-1167` is `wow-tortoise`'s entire `sql.phases`: `schemas`, `app user and grants`, `world base`, `playerbots characters`, `playerbots world`, `realm row`. `grep -rn character_updates` over the catalog data finds nothing; `Database.AutoUpdate.Path` (`:1093`) points the fork's own updater at `database_updates/`, a different directory. Confirmed by T4's hand and its cold reviewer.

## What to build

- A phase in `wow-tortoise.sql.phases` that applies `src/tortoise-wow/sql/character_updates/*.sql` into `tw_char` **in name order** (the two known files depend on that order), after the phase that makes `tw_char` and before the first world start. Read the existing phases and the runner in `families/cmangos.py` first: if a phase can already name a directory, use that; if it can only name files, either list the files (and say the list rots on the next fork sync) or extend the runner to a glob — say which and why.
- **Idempotent** on an install that already has them (the m910q's has both applied by hand): `CREATE TABLE IF NOT EXISTS` / the index's own guard — read each file; if one is not idempotent, say so and refuse to make it so silently.
- TDD: a test that pins the phase's existence, target database and order (`test_tortoise_boot_facts.py` is where this entry's measured facts live — read its style); the mutation each catches.
- Record in the entry's own notes (the catalog has a place for per-tree facts — find it) that this directory is the fork's, that its updater does not apply it, and the two dates above.

## Definition of done

`--checks` ALL GREEN from your worktree's `pylauncher/` (announce on `yulon-fedora` first); the new test fails first. One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours.

## Report format (final message)

`## Report` — sha, gate last line, diff stat, the phase as written (quoted), how order and idempotence are guaranteed, the tests and their mutations, deviations, status DONE.
