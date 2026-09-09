# T4 — Rebuild the Tortoise image on the m910q, then the reimport with no pre-application at all

**Status:** DONE (hand reported 2026-09-09 12:08; awaiting review) -- UPGRADED
**Filed:** 2026-09-09 08:40 by the lead (Fable). **The owner authorised this rebuild** ("do all 4", 2026-09-09 ~08:30, against the lead's recommendation 4).
**Hand:** Opus, worktree branched from `yulon-phase8b` (ff-merge `origin/yulon-phase8b` first; report the sha)
**File set (yours alone):** `pyplan/gates/tortoise-reimport-rehearsal-m910q-2026-09-08/tortoise-reimport-live.sh` (its step 2 loses its migration half), and a NEW `pyplan/gates/tortoise-upgrade-m910q-2026-09-09/`. No product code; a catalog gap you confirm is a finding for the report (it is already named, below).
**Box:** `m910q`. One server at a time and stop it when done — the owner's rule. `r6` (`refute5img`, up days) is not yours; leave it.

## Why, from the live attempt at 01:06Z (`…rehearsal-m910q-2026-09-08/LIVE-ATTEMPT-2026-09-09.md` — read it whole)

The reimport failed twice over and rolled back to the row. (1) The live script pre-applies `20260903063722_world.sql` ahead of the other 172 migrations, and twelve of that file's statements `UPDATE spell_template SET script_name …`, a column the fork's base does not define — so it can never run first. (2) The only reason for pre-applying it is a duplicate-key collision the image template already fixes at build time (`pylauncher/catalog/installers/wow-tortoise/native/Dockerfile.tmpl:175-176`, `INSERT INTO` → `INSERT IGNORE INTO` in exactly that file, commit `3a1ed6ee`), and the head image on the box (`head-2404-2026-09-08`, built 04:21Z that day) **predates that fix**: `grep -c "INSERT IGNORE INTO"` inside it → 0.

So: a rebuilt image, and then the updater applies all 173 world migrations **in order**, with the collision handled inside the file it reads.

## Also, from this morning (`…/tortoise-soap-yulon-arch-2026-09-09/README.md` and the 7.9 re-run folder)

The install crash-looped at 07:xx on `TRUNCATE character_inventory_copy` — honor maintenance fell due for the first time and the table its code truncates is created only by `sql/character_updates/20260812142512_character_inventory_copy.sql`, which nothing applies. The lead applied that one file by hand and the world came up. The rollback had also dropped `character_pvp_currency`; restored from the 09-08 dump by hand. **Both are the same catalog gap: `wow-tortoise` has no `sql.phases` entry for the fork's `character_updates/` directory.** Confirm the gap from the catalog, name it in the README, do not fix it here.

## Steps, ground first, each with its own capture

1. **Ground**: `sudo docker ps` (nothing of yours running); the image ids on `native-58c6fd1c`, `rollback-2026-09-08`, `head-2404-2026-09-08`; the four counts (`tw_char.characters`, `tw_logon.account`, `tw_world.migrations`, `tw_char.migrations` — expected 903 / 109 / 158 / 1); `df -h /` (29 G free last night); the dumps in `~/tortoise-backup-2026-09-08b/` (four files; `tw_world.sql` is 171 MB) — **take fresh dumps of all four databases with the server stopped before anything moves**, into a dated directory, and count the rows in them.
2. **The rebuild**, through the app's own control (`install_wiring.rebuild_for_app` — the install at `~/tortoise-server` is app-made, `.yulon-install.json` is there) rather than a bare `docker build`, because that is the press the product makes and the rollback tags it keeps are part of what you are proving. Prove the new image carries the rewrite: `docker run --rm --entrypoint sh <new image> -c 'grep -c "INSERT IGNORE INTO" /opt/tortoise/sql/database_updates/world/20260903063722_world.sql'` → 14, and `^INSERT INTO` → 0. If the rebuild fails, the app's rollback should put the old image back on its own — capture that if it happens, then STOP and report.
3. **Edit the live script's step 2**: keep the `tw_world` drop/recreate, the base import (191 files), the playerbots world SQL, and the character index; **delete** the pre-application of `20260903063722_world` and its hash row. Say in the README what was removed and quote the audit sentence that says why.
4. **Run it.** Watch the updater: the expected line is `Found 173 possible migrations for world` and then **173 applied, 0 failed**, and `Found 2 possible migrations for character`. Capture the updater's own lines. If a migration fails, `tortoise-rollback.sh` is beside the script; **run it, and additionally restore `tw_char` whole from your fresh dump**, because last night's rollback restored `tw_world` whole and `tw_char` only in part (that is how `character_pvp_currency` went missing). Verify the four counts and the two character tables named above after any rollback.
5. **Prove it is the head**: SOAP `server info` through the channel (the app's own `SoapChannel`, account of your own at rank 4 — this fork reads the rank at startup, so make the account BEFORE the world's final start, see the arch README), the four counts (`tw_world.migrations` should be 173 + whatever the base recorded, say the number), and one client-visible fact if a client is on the box (`~/clients` holds one).
6. **Stop the stack.** Leave the box with nothing of yours running.

## Definition of done

- README leads with UPGRADED or ROLLED BACK, the four counts before and after, the updater's own tally, and what is NOT proved (SOAP password change is still 8.3d's open question; say so).
- `--checks` ALL GREEN from your worktree's `pylauncher/`; `test_no_secrets_in_evidence.py` green locally — the database password must not reach a log (`.db_password` is generated; mask it).
- One commit, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only.

## Report format (append `## Report`, mark DONE)

Sha, gate last line, UPGRADED / ROLLED BACK, the counts table, the rebuild's duration, deviations flagged.

## Report (hand, 2026-09-09 12:08)

- sha `500412d9` on `worktree-agent-ac4d760e80e0fa2fc` (base `e2912690`); gate ALL GREEN, 3769 passed; `test_no_secrets_in_evidence` green first; diff 16 files +2933/-19: the live script (55 lines) and a new `pyplan/gates/tortoise-upgrade-m910q-2026-09-09/` (README, 10 logs, 4 scripts).
- **UPGRADED.** Updater: `Found 173 possible migrations for world`, 173 attempted, zero `failed to apply`, `World server is up and running! Loading time: 1 minutes 9 seconds`; the next start finds 173 and attempts 0. Counts: characters 903/903, accounts 109/110 (the app's own channel account `YULON_58C6FD1C`, rank 4), world migrations 158/173, char migrations 1/2. Fresh dumps of all four databases with the server stopped, counted inside the dumps, sha256 recorded, never needed.
- The rebuild through `install_wiring.rebuild_for_app()`: 2525 s (40m58s compile, 7 s recreate, 61 s ready); the `-rollback` tag kept and let go as documented; in-image proof on `native-58c6fd1c` (`50ed3f49b637`, Ubuntu 24.04): `INSERT IGNORE INTO` 14, bare `^INSERT INTO` 0.
- Catalog gap confirmed: `catalog.json:1113-1167` is `wow-tortoise`'s whole `sql.phases`, six phases, none naming `character_updates/`.
- **Deviations, two of them product findings:** (1) **a bare Rebuild cannot carry the fix** -- `native.py:1763 rebuild_stages()` excludes `write-dockerfile`, so it compiles the Dockerfile on disk (rendered 2026-09-07, `FROM ubuntu:22.04`, zero `INSERT IGNORE`); the hand ran the family's `write-dockerfile` stage first. (2) Step 1 of the live script would have tagged the un-rewritten head image over the live tag and undone the rebuild; changed to prove the tag only. (3) The rebuild's own `ready` start applied 16 world + 1 char migration in order before the reimport (158->174; 174 = 173 files + one row for a migration the fork deleted); after the baseline reimport exactly 173. (4) **Repair cannot repair this tree**: `controller_wow_wotlk.accounts.reset_own_password:440-445` knows `mangos_srp6` and AzerothCore only; Tortoise is `mangos_sha`; the press died on `Unknown column 'salt'`; worked around by deleting the app's account and running both channel halves in one process; `server info` then `outcome=yes`. (5) No client-visible fact: only the 1.18.1 client connects and `~/clients` has 1.12.1 and 2.4.3. (6) `tw_logs` dumped a minute after the other three; a temporary worktree on the box, removed.
- Not proved: the SOAP password-change verdict (8.3d); no client; no bots.
- Box left: `r6` only; the Tortoise stack stopped.
