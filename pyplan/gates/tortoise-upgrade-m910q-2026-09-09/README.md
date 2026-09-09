# UPGRADED — m910q's Tortoise install is at the fork's head, reimported at their baseline, 2026-09-09

The owner authorised the rebuild in writing this morning (*"do all 4"*, ~08:30, against the lead's
recommendation 4). This is T4: the image rebuilt from the current tree through the app's own
Rebuild, then `tw_world` reimported from the fork's `sql/base` with **nothing pre-applied at all**,
and the updater left to apply all 173 world migrations **in order**. It came up.

| | before (07:57Z, server stopped) | after (08:56Z) |
| --- | --- | --- |
| `tw_char.characters` | 903 | **903** |
| `tw_logon.account` | 109 | **110** — the app's own channel account, `YULON_58C6FD1C` at rank 4, and nothing else |
| `tw_world.migrations` | 158 | **173** — every file the tree ships, none left over |
| `tw_char.migrations` | 1 (`20260817151028_character`) | **2** (`20260903211500` applied by the updater) |
| image on `native-58c6fd1c` | `ff4de2e90500` (2026-09-07, ubuntu 22.04) | **`50ed3f49b637`** (2026-09-09, ubuntu 24.04) |
| `tw_char` tables | 109, `character_pvp_currency` and `character_inventory_copy` both present | **109**, both still present |
| `tw_world` tables | — | 203 after the base import, **205** after the migrations |

The updater's own tally, from `reimport.log`:

```
[DB Auto-Updater] Found 2 possible migrations for character.
[DB Auto-Updater] Found 173 possible migrations for world.
World server is up and running! Loading time: 1 minutes 9 seconds
update statements attempted: 173
```

There is no "N applied, 0 failed" line to quote: `AutoUpdater.cpp` prints `Found %u possible
migrations`, one `Attempting to execute update %s` per file and `Migration %s … failed to apply` on
a failure, and nothing else.

### The transcript, and why the counts above are not it (round 2)

`reimport.log` filtered the 173 `Attempting to execute update` lines out and kept the aggregates, and
a reviewer was right that **the aggregates are not a transcript**: "173 rows afterwards" and "0
attempts on the next start" are both consistent with one file applied twice and another skipped.

The container that ran it was still on the box, stopped and never recreated (`e716a18c5d8a`, created
08:48:14Z by the reimport's own `compose up`, exited 08:56:51Z), so the updater's own output was
recovered from it: **`updater-transcript.log`**, `docker logs -t` filtered to the `[DB Auto-Updater]`
lines and the ready banner, nothing redacted — the install's generated password does not occur in
that log at all (`grep -c -F` against `.db_password` = 0, and no `…;3306;…` line).

`check-updater-order.sh` re-derives the claim from that transcript and from the file list inside the
image, so nobody has to count 173 lines by eye (`check-updater-order.log`):

```
the updater said:      Found 173 possible migrations for world
attempt lines:         173
distinct names:        173
*.sql in the image:    173
failed to apply:       0
order:                 attempted in exactly the image's sorted file order
first attempted:       20260504194945_world
last attempted:        20260906190000_world
VERDICT: 173 distinct migrations, in the image's own order, none failed
```

The second start in the same transcript finds the same 173 and attempts 0, which is the same claim
from the other side.

## What was in hand before anything moved

`ground.log`. Nothing of ours was running (only `r6`, four days up, not ours); 29 G free. Fresh
dumps of **all four** databases with the server stopped, into `~/tortoise-backup-2026-09-09-T4`
(`tw_char` 62,318,473 B, `tw_world` 171,361,088 B, `tw_logon` 114,576 B, `tw_logs` 3,177 B, sha256
of each in the log). The four counts were then read **out of the dumps** rather than only out of the
database, by dumping the four counted tables a second time with `--skip-extended-insert` so one row
is one `INSERT`: 903 / 109 / 158 / 1, matching the live reads to the row. `tw_char.sql` carries
`CREATE TABLE` for `characters`, `character_pvp_currency` and `character_inventory_copy` — the two
that went missing on 2026-09-08 are IN this dump, which is the whole reason the rollback plan below
restores `tw_char` whole.

## The rebuild, and the one thing it could not do by itself

Pressed through `install_wiring.rebuild_for_app()` — the callable the Server tab's Rebuild button
drives (`t4_press.py rebuild`), not a `docker build`. **2525 s (42 min)**: 40 m 58 s compile, 7 s
recreate, 61 s to the ready banner. Its own lines are in `rebuild.log` — the press's whole
capture with the 1,187 `Building CXX object` progress lines dropped and everything else, warnings
included, kept; the ones that matter are also pulled out into `rebuild-summary.log`.

**A bare Rebuild press could not have carried the fix, and that is a finding.**
`native.StagedInstaller.rebuild_stages()` (`pylauncher/yulon/catalog/native.py:1763`) is `build,
recreate, ready` and deliberately excludes `write-dockerfile` and `generate-compose` — *"rewrites
files a running server is using"*. So a rebuild compiles **the Dockerfile already on disk**, and
this install rendered its Dockerfile on 2026-09-07, before `3a1ed6ee` put the `INSERT IGNORE`
rewrite into `wow-tortoise/native/Dockerfile.tmpl:175-176`. Measured on the box: the Dockerfile in
`~/tortoise-server` had `FROM ubuntu:22.04` twice and **zero** `INSERT IGNORE` lines.

So the family's own `write-dockerfile` stage was run first, through the same renderer, and only then
the rebuild (`dockerfile-render.log`):

```
before: FROM ['FROM ubuntu:22.04 AS builder', 'FROM ubuntu:22.04'] | INSERT IGNORE lines 0
after:  FROM ['FROM ubuntu:24.04 AS builder', 'FROM ubuntu:24.04'] | INSERT IGNORE lines 1
the generated database password is absent from the Dockerfile: True
```

**The image carries the rewrite** (`image-rewrite-proof.log`, which now holds the exact command
beside its output):

```
INSERT IGNORE INTO: 14
bare INSERT INTO: 0
sha1 of the rewritten file: 7a3a97e96c19e5b1188da8dbe1018c9b4be61791
PRETTY_NAME="Ubuntu 24.04.4 LTS"
```

**Where those numbers did NOT come from: the script's step 1.** Read `reimport.log` and step 1 is
there in the form it RAN, and it is broken twice over — the pre-flight asked for the app's
`native-58c6fd1c-rollback` tag and got `Error response from daemon: No such image` (that tag exists
only during a press), and its `grep -c` lost its pattern through the quoting and printed
`…20260903063722_world.sql:0` for **both** counts. The 14 / 0 were read by hand, from the command now
recorded in `image-rewrite-proof.log`, against the same image and the same file. **Step 1 was
repaired after the run and has not been re-run end to end**; what has been re-run, read-only in round
2, is that command. The repair also gave step 1 something it never had: the counts are read through a
bare `ssh` into a variable and compared, so a wrong count now exits 3 instead of scrolling past —
`onbox` pipes into `tee`, so nothing it runs could ever have stopped the script.

The app kept its rollback as it says it does — *"Kept the build you have now as a rollback (1 images
tagged -rollback)"* — and **let it go after the rebuild came up** (`_let_go`, `native.py:2090`), so
`native-58c6fd1c-rollback` exists only DURING a press. The pre-rebuild image is still on the box
under `rollback-2026-09-08` (`ff4de2e90500`), which is what the rollback plan names.

### The rebuild's own start upgraded the install before the reimport ran

Unplanned and worth recording: `ready` starts the world, and the world on the new image found its
database 16 migrations behind and applied them **in order**, `20260903063722_world` among them,
without a duplicate-key failure — `158 → 174` world rows, `1 → 2` character rows, up in 1 m 0 s.
That is the same fix the reimport then proved from the baseline, arriving by the ordinary upgrade
path. It also produced the one number that needs explaining: **174 = 173 files + 1 row for
`20260721013813_world`**, a migration the fork has since deleted from the tree, which the updater
itself reports as *"exists in DB but not as file, old migration?"*. After the baseline reimport the
table holds exactly the 173 it ships.

## What was removed from the live script, and why

`../tortoise-reimport-rehearsal-m910q-2026-09-08/tortoise-reimport-live.sh`, step 2, lost this:

```sh
F=src/tortoise-wow/sql/database_updates/world/20260903063722_world.sql
H=$(sha1sum $F | cut -c1-40 | tr a-f A-F)
sed "s/^\(\s*\)INSERT INTO/\1INSERT IGNORE INTO/" $F | $M tw_world \
  && $M -e "INSERT INTO tw_world.migrations (Name, Module, Hash, AppliedAt) VALUES ('20260903063722_world','','$H',NOW())"
```

— the pre-application of that one migration and the hash row that recorded it. The audit's sentence
for why it can never work (T4, from the 01:06Z failure):

> the live script pre-applies `20260903063722_world.sql` ahead of the other 172 migrations, and
> twelve of that file's statements `UPDATE spell_template SET script_name …`, a column the fork's
> base does not define — so it can never run first.

Kept, exactly as the ticket said: the `tw_world` drop/recreate, the base import (191 files), the
playerbots world SQL (8 files), the character index.

Two further edits the ticket did not name and the press required:

* **Step 1 no longer moves a tag.** It tagged `head-2404-2026-09-08` onto `native-58c6fd1c` — the
  image built at 04:21Z on 09-08, the one WITHOUT the rewrite. Run after this rebuild it would have
  put the broken image back over the fixed one. It now only proves what is on the tag (the 14 / 0
  above, asserted with an `|| exit 3`) and reads the Dockerfile's `FROM` lines, and it no longer
  `sed`s the Dockerfile, which the app's own renderer now writes. **Both of step 1's repairs landed
  after the run in `reimport.log`, which shows it failing** — see above.
* The pre-flight asks for `rollback-2026-09-08` and the four fresh dumps, not for the app's
  `-rollback` tag, which does not exist between presses (above); `strings` is not in the 24.04
  runtime image, so the SOAP-symbol check is `grep -c -a` on the binary.

## The character index: step 2 put back something head had already taken away

`index-shape.log`. Head's `20260903211500_playerbot_event_atomic_upsert.sql`, read out of the image,
does two things to `ai_playerbot_random_bots`:

```
ALTER TABLE `ai_playerbot_random_bots` DROP INDEX `idx_owner_bot_event`;
ALTER TABLE `ai_playerbot_random_bots` ADD UNIQUE INDEX `uq_owner_bot_event` (`owner`, `bot`, `event`);
```

At ground (07:57Z) the install had `idx_owner_bot_event` and no `uq_owner_bot_event` — that migration
had never run. It ran during the **rebuild's own start** at 08:41 (above), dropping `idx` and adding
`uq`. Step 2 of the reimport then applied `character_updates/20260708055500` unconditionally at
08:47, as the ticket said to keep, and **re-created the index head had just dropped**. Read back on
the box in round 2, with the database started alone and stopped again:

```
idx_owner_bot_event  non_unique=1  columns=owner,bot,event
uq_owner_bot_event   non_unique=0  columns=owner,bot,event
```

So **`tw_char` does not match head's shape**: it carries one redundant non-unique index on the same
three columns as the unique one, on a table of 8,959 rows. Nothing is broken by it — the uniqueness
head wanted is enforced by `uq_owner_bot_event`, and the migration is recorded applied so it will not
run again — but it is a difference from a fresh install at this pin, and it costs a second index
write on every insert into that table.

What would put it right, with the world stopped, is one statement:

```sql
ALTER TABLE tw_char.ai_playerbot_random_bots DROP INDEX idx_owner_bot_event;
```

**Not done here** — this ticket's file set is the record, and a schema change on a live install is a
decision, not a tidy-up. The general form is the same one the catalog gap below names: step 2 applies
that file because nothing else ever does, and the ordering only works while head's migration has not
run yet. The script now says so in a comment rather than silently depending on it.

## The channel: `server info` answered, through the app's own seams

`channel-full.log`, wired exactly as `ui/controller_view.py::_for_tortoise` does it —
`channel_setup.InstallChannel`, `controller_wow_tortoise.accounts.create_account`,
`channel.SoapChannel`. The account is made with the world DOWN, because this fork reads
`account.rank` at startup (`../tortoise-soap-yulon-arch-2026-09-09/`), then the world is started and
the same object asks:

```
after prove (world down): state=Pending account=YULON_58C6FD1C
world: up and running
after prove (world up):   state=Verified account=YULON_58C6FD1C
--- server info -> outcome=yes ---
Core revision: unknown / 1970-01-01 00:00:00 +0000 / Linux_x64 (little-endian)
Players online: 0. Max online: 0.
Server uptime: 1 Minute 1 Second.
```

`server motd` came back `outcome=no`, *"There is no such subcommand"* — this tree has no such
command, and an honest refusal through the channel is a working channel, not a failed one.

### A second finding, met while doing it: **Repair cannot repair this tree**

Pressing the two halves as two processes (`arm` then `ask`) fails, and the failure is a product
defect rather than an artifact of the driver. Only a VERIFIED credential is written to disk, so the
second process minted a new password, `create_account` correctly refused to re-salt an existing row,
and the state went to `Refused` — which is exactly what the Repair button is for. It raised:

```
UPDATE account SET salt = X'…', verifier = X'…' WHERE username = _utf8mb4 X'59554C4F4E5F3538433646443143'
ERROR 1054 (42S22) at line 1: Unknown column 'salt' in 'SET'
```

`controller_wow_wotlk.accounts.reset_own_password:440-445` knows two schemes — `mangos_srp6` writes
`v`/`s`, everything else writes AzerothCore's `salt`/`verifier` — and Tortoise's scheme is
`mangos_sha`, one column, `sha_pass_hash`. `controller_wow_tortoise.accounts.reset_own_password:180`
passes the right scheme into a function that cannot use it. Its own comment two lines above the
branch says why this shape recurs: *"a parameter that is accepted and ignored is worse than no
parameter"* — `create_account()` learned the third scheme on 2026-09-08 and this did not. **Not
fixed here** (T4 touches no product code). The gate's press worked around it by deleting the app's
own account and running both halves in one process, which is how the running app holds that state
anyway.

## The catalog gap, confirmed from the catalog

`pylauncher/yulon/catalog/catalog.json:1113-1167` is `wow-tortoise`'s whole `sql.phases` list, six
phases: `schemas`, `app user and grants`, `world base`, `playerbots characters`, `playerbots world`,
`realm row`. **None of them names `src/tortoise-wow/sql/character_updates/`** — and
`grep -rn character_updates pylauncher/yulon/catalog/catalog.json pylauncher/catalog/` finds nothing
at all. The fork ships three files there:

| file | what it costs to skip it |
| --- | --- |
| `20260708055500_ai_playerbot_random_bots_index.sql` | their `20260903211500` `DROP INDEX`es it without `IF EXISTS`; this gate's step 2 applies it by hand, as the rehearsal did |
| `20260731160000_guild_bank_money_unsigned.sql` | not yet met |
| `20260812142512_character_inventory_copy.sql` | **this morning's crash-loop**: honor maintenance fell due for the first time and `ObjectMgr::BackupCharacterInventory` truncates a table nothing had created (`../7.9-rerun-m910q-2026-09-09/tortoise-crash-probe.txt`, 17 deaths) |

Named on a fresh install on yulon-arch and again on this established one; **not fixed here**, per the
ticket.

## `after.log` lists only `tortoise-db` stopping — what stopped the other two

Nothing else did: the capture ran `docker compose stop 2>&1 | tail -2`, and **`tail -2` is the whole
answer**. Compose stops in dependency order — `tortoise-mangosd`, then `tortoise-realmd`, then
`tortoise-db`, two lines each — so the last two lines of that output are the database's, and the
four lines above them were cut off by the pipe. The `docker ps` in the same capture, which is not
filtered, already showed all three gone, and `docker ps -a` in round 2 says how they went:

```
tortoise-mangosd   Exited (0) 19 minutes ago
tortoise-realmd    Exited (0) 19 minutes ago
```

Exit 0 for both, at 08:56:51Z, one second apart from the same press — a clean stop, not a crash and
not something else stopping them. A capture that truncates its own evidence is the smaller version
of the mistake this folder's round 1 made twice; `tail` is gone from the round-2 captures.

## What is NOT proved

* **The SOAP password change.** Still exactly where 8.3d left it: the fork's account-lockout bug
  (an empty-name hash for any account that has logged in) was not pressed here, and nothing in this
  gate touched `account set password`. Open question.
* **No client logged in.** This fork takes only the Turtle-WoW 1.18.1 client, build 7272 — the
  catalog says so in as many words (*"any other build will not connect"*) — and `~/clients` on m910q
  holds 1.12.1 and 2.4.3 only. So there is no client-visible fact from this box; the ready banner,
  the listening ports (`0.0.0.0:3724`, `0.0.0.0:8090`) and the row counts are the evidence.
* **Bots.** Nobody was online during the press; `Players online: 0`. The bot population was not
  exercised and is not claimed.
* **That `tw_char` equals a fresh install at this pin.** It does not: it carries
  `idx_owner_bot_event` beside head's `uq_owner_bot_event` (section above), and `tw_world` carries a
  migration row for a file the fork has deleted. `tw_world`'s tables and rows ARE from their base
  plus their 173; `tw_char` was never reimported and was never meant to be — it holds the 903
  characters.
* **The Repair path** above is measured broken, not fixed, and nothing here proves what it would do
  once it writes the right column.

## Files

| file | what it is |
| --- | --- |
| `t4-ground-and-dump.sh`, `ground.log` | ground and the four fresh dumps, with the rows counted inside them |
| `t4_press.py`, `dockerfile-render.log`, `rebuild.log`, `rebuild-summary.log` | the app's `write-dockerfile` stage and `rebuild_for_app` press, and what they produced |
| `image-rewrite-proof.log` | 14 / 0, inside the image on the tag compose uses, with the exact command that read them |
| `../tortoise-reimport-rehearsal-m910q-2026-09-08/tortoise-reimport-live.sh` | the live script, with its migration half removed |
| `reimport.log` | the reimport press, base import to `VERDICT: UP` — including step 1 failing, kept as it ran |
| `updater-transcript.log` | the updater's own 173 attempt lines, recovered from the container that ran them |
| `check-updater-order.sh`, `check-updater-order.log` | that transcript checked against the image's own file list: 173, distinct, in order |
| `index-shape.log` | head's index migration, and the two indexes the install actually has |
| `t4_channel.py`, `channel-arm.log`, `channel-ask.log`, `channel-full.log` | the channel, including the two-process failure kept on purpose |
| `t4-rollback-plus.sh` | the rollback that was in hand and was not needed: their script, then `tw_char` restored WHOLE |
| `after.log` | the counts after, and the box left with nothing of ours running |

The database password is generated and never appears in any of these; the channel account's password
is never printed and lives only in the app's own credential file.
