# Tortoise `tw_world` reimport — rehearsed on a copy of the live volume, m910q, 2026-09-08

Owner answer 1 (`pyplan/phase8-owner-answers-2026-09-08.md`): move the existing Tortoise install to
the fork's head by reimporting the world database at their baseline, tested before the live
install is touched. This folder is the test. **The live install was not touched**: a new volume
(a byte copy of `yulon-wow-tortoise-58c6fd1c_db-data`), a throwaway compose project
(`~/tortoise-rehearsal`, its own container names, no published ports), the live `./data` mounted
read-only, the live `./etc` copied. Fresh dumps of the three live databases were taken first with
the server stopped (`~/tortoise-backup-2026-09-08b`, `counts-before.txt`: 903 characters, 109
accounts, 158 world migration rows, 1 character migration row).

Why a copy on m910q and not a VM: a faithful VM rehearsal needs a Tortoise build on the VM, 3.6 GB
of extracted data copied over and the dumps restored, on a host with 12 GB free and three VMs
running the other lanes; the copy is the same bits on the same box with the live volume as the
checkpoint. The deviation is the owner's to accept (morning question 2).

## What was measured

| Step | Log | Result |
|---|---|---|
| the head image | `rehearsal.log` §1 | the 24.04 build of 04:21Z survived the night's rollback untagged (`656b8e4949f8`, 1.07 GB); tagged `:head-2404-2026-09-08`; `strings` finds `ns1__executeCommand` and `soap_serve`; `Ubuntu 24.04` |
| their baseline | §5 | `sql/base/*.sql` at head `3a8472e`: 191 files, all imported; the migrations table they ship is **empty** (0 rows at the snapshot too) — the updater is meant to apply all 173 world files to it |
| the updater | `AutoUpdater.cpp` | hash-keyed: SHA1 of the file, recorded as `(Name, Module, Hash)`; a file whose bytes change is a new migration |
| first start | §7 | `Found 173 possible migrations for world`; 157 applied; **`20260903063722_world` failed: `[1062] Duplicate entry '44070' for key 'PRIMARY'`** in `spell_proc_event`; `DB AutoUpdater FAILED, cancelling server` |
| the collision | `rehearsal-part2.log` | the base does NOT hold 44070 (`grep -c "(44070," base/tw_world_spell_proc_event.sql` = 0): the file's own second `INSERT INTO spell_proc_event` collides with its first, or with a sibling migration's — **their head cannot be installed fresh by their own updater**, which is a different fact from the one the night's incident suggested (an existing install too far behind) |
| the second start | part 2, round 2 | that one migration recorded as applied under the hash the updater computes (`34F86966…`, = `sha1sum` of the file); the remaining 15 applied; **`World server is up and running! Loading time: 1 minutes 1 seconds`**; 173 world rows, 2 character rows, **903 characters** |
| character side | §6 | their `character_updates/20260708055500` index (`idx_owner_bot_event`) added first, because their auto migration `20260903211500` `DROP INDEX`es it without `IF EXISTS`; then `20260817151028_character` and `20260903211500` applied by the updater |

## What is not proved

* SOAP was not exercised: the conf on the box does not enable it, and turning it on is the
  catalog's operations block, not this rehearsal's.
* Statements of `20260903063722_world` after the failing one did not run on the copy in round 2
  (the migration was marked applied, not re-applied). The live script applies the whole file under
  `INSERT IGNORE` before recording it, so the live world gets every statement; the copy did not.
* No client logged in; "up and running" and the row counts are the evidence.

## The live run, staged and not pressed

`tortoise-reimport-live.sh` does the same on the live volume (head image onto
`native-58c6fd1c`, Dockerfile to 24.04, base import, the one file under `INSERT IGNORE`, the char
index, `compose up`, watch), and `tortoise-rollback.sh` undoes it (image `rollback-2026-09-08`,
`tw_world` from the dump, the unique index back to a plain one, the character migration rows
removed, Dockerfile back). Claude Code's auto-mode classifier refused the live script — it drops
the live world database — so the press is the owner's.
