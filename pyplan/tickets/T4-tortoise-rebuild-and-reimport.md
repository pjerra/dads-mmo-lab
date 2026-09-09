# T4 — Rebuild the Tortoise image on the m910q, then the reimport with no pre-application at all

**Status:** OPEN
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
