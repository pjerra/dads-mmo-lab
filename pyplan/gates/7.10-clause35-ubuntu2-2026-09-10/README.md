# The 7.10 clause-35 runner's two fail-open paths, closed and watched failing — `yulon-ubuntu2`, 2026-09-10

**T23.** The owed Codex adversarial pass on T3's round 4 (`a4454ec0`) found two high findings, both
in this gate's own runner and neither in shipped code. Nothing under `pylauncher/` is touched by this
ticket, and nothing needed to be.

1. **The restart window still opened before the restart.** Round 4's `--since` stamp came off the
   clock and `docker restart` ran afterwards, so everything in between was inside the window. An
   `Added realm` line written into that gap read as the restarted server's own announcement. The
   zero-count precheck could not see it, because the precheck ran *before* the gap opened.
2. **A failed firewall backup did not stop the Apply.** The two privileged `cp`s and their `chown`
   were unchecked and the script has no `set -e`, so the driver that presses Apply ran regardless;
   and the exit trap read a missing `ufw-user.rules.before` as proof that nothing had changed and
   wrote `[RESTORED]`.

Both are closed here, and both are **watched failing on the live box** rather than argued. The
2026-09-09 folder next door is left exactly as it was merged — it is the record of what round 4
shipped, and this folder is what replaces it.

**Six runs**, all on `yulon-ubuntu2` between 19:13 and 19:18 CEST, all from the same `run-t3.sh`
that is committed here, all through `invoke-t3.sh` so that every exit status has a file behind it.
Every stamp in this folder was read from the box's own clock by the script that wrote the line; none
was typed. Every line below names the file that shows it.

---

## The two rules, as they are written in the runner

Both are stated in the file that enforces them, at the point of enforcement.

**THE BOUNDARY RULE** — `run-t3.sh:339-349`, enforced at `:364-398`:

```
# THE BOUNDARY RULE. The only window this check may read is the restarted
# container's own `StartedAt`, taken from `docker inspect` AFTER `docker
# restart` has returned, and used only if it DIFFERS from the value read
# before the restart. If it is unreadable, or unchanged, this check reads no
# window at all and records a RESTORATION FAILURE: a line found in some other
# window is not evidence that THIS restart announced anything. `StartedAt` is
# already an RFC3339 stamp in UTC with a `Z` -- it is the container's own
# account of when the process behind these log lines began, so nothing written
# before the restart can be inside it, whoever wrote it and whenever they did.
# The stamp handed to Docker is printed verbatim, because a window nobody can
# read is a window nobody can check.
```

`:364` reads `StartedAt` before, `:382` reads it after `docker restart` returns, `:384-385` refuses
the whole reading if it did not move, `:387-390` hands Docker that exact value and prints it, and
`:392` is the query. The precheck stays at `:356-359` and now labels itself
`precheck (NOT load-bearing since T23)`.

**THE BACKUP RULE** — `run-t3.sh:498-508`, enforced at `:509-524`:

```
# THE BACKUP RULE. Nothing that can press Apply starts until BOTH firewall
# files have been copied aside, chowned so the driver can read them, and read
# back byte for byte against the live files with `cmp`. Any of those three
# failing stops the run here, with the reason, and the driver is never
# invoked. An unbacked firewall is not a thing to press Apply against.
```

and its second half, in the trap — `run-t3.sh:233-243`, enforced at `:245-263`:

```
# THE BACKUP RULE, second half. Round 4 asked one question here -- does the
# copy exist? -- and read "no" as "then nothing was changed, [RESTORED]". That
# is a question with three answers collapsed into two. The copy can be absent
# because nothing that could touch the firewall ever ran, and it can be absent
# because it was never made, or was lost, while something that presses Apply
# was running. In the second case the live firewall may already be altered and
# the one thing that could have told us is gone, so the absence of a copy is
# not evidence that nothing changed -- it is the loss of the evidence. Once
# the Apply-capable phase has been entered, a backup that is missing, empty or
# no longer the size it was verified at is a RESTORATION FAILURE, never
# `[RESTORED]`.
```

The single flag that separates the two cases is `APPLY_PHASE_ENTERED`, set at `run-t3.sh:578` —
after the backup step and before the first driver, so a run that stopped *in* the backup step never
reached a driver and may honestly say nothing was owed.

---

## The six runs

| # | run | what it was for | exit | folder |
|---|---|---|---|---|
| 1 | **mut-backup-refused** | both copies forced to fail → the driver never starts | **70** | `mut-backup-refused/` |
| 2 | **mut-backup-lost** | copies made, Apply phase entered, copies then gone → the trap must call it a FAILURE | **90** | `mut-backup-lost/` |
| 3 | **mut-stale-window** | a matching line planted in the gap, the restarted server silent → RED | **90** | `mut-stale-window/` |
| 4 | **failrestore** | a driver that fails **and** a restoration that cannot succeed | **90** | `failrestore/` |
| 5 | **failclosed** | a driver that fails, every restoration verifying | **1** | `failclosed/` |
| 6 | **green** | the real drivers, all 33 widget clauses | **0** | `green/` |

The order is deliberate: the mutations first, because a rule nobody has watched fail is a claim; the
green run last, because it is the state the box is left in. Every `RUNNER EXIT STATUS` line above is
written by `invoke-t3.sh:61`, not by a person at a prompt.

---

## 1. Mutation — both firewall backups forced to fail, and the driver never starts

`mut-backup-refused.sh` makes both copy *targets* immutable with `chattr +i`. An immutable file
cannot be written by root either, so the runner's own `sudo cp -a` returns non-zero. That is the
"read-only target" the ticket names, and it puts nothing between the script and the `cp` it really
runs — no wrapper, no `PATH` game, and no edit to the runner for the mutation's sake.

| what had to happen | where it is |
|---|---|
| `cp` really failed, as root, on the real target | `mut-backup-refused/mutation.log:19` — `cp: cannot create regular file '…/ufw-user.rules.before': Operation not permitted` |
| **the run stopped there, with the reason** | `mut-backup-refused/run.log:22` |
| **the driver that presses Apply was never invoked** | `mut-backup-refused/mutation.log:24-26` — neither `clause35-falsify.log` nor `widget-run.log` exists, and the driver's shots directory holds 0 frames |
| the runner's own status | `mut-backup-refused/run.log:25` — `RUNNER EXIT STATUS (as seen by the invoking shell): 70` |
| the trap did **not** invent a restoration it could not do | `mut-backup-refused/restore.log:6-10` — `the Apply-capable phase was entered: 0` … `[RESTORED] nothing to put back: nothing in this run could have pressed Apply` |
| `/etc/ufw` itself never moved | `mut-backup-refused/mutation.log:6-7` vs `:32-33` — the same two sha256s before and after |
| the world was alive throughout | `mut-backup-refused/state-before.txt:7-10` and its `state-final.txt` — `docker inspect … /ac-worldserver Running=true Pid=91802` |

The decisive line:

```
STOPPED: the firewall backup of /etc/ufw/user.rules could not be made (cp exited non-zero),
so the driver that presses Apply is NOT started (status 70)
```

## 2. Mutation, second half — the backup lost *after* the Apply phase was entered

This is the other half of Codex's finding 2, and it is the fail-open the trap itself carried:
`[RESTORED]` written on the strength of a missing file. `T23_DROP_BACKUP_AFTER_APPLY_PHASE=1`
deletes both copies at `run-t3.sh:584-587`, immediately after the driver slot, so the trap meets
exactly the case the finding describes.

| what had to happen | where it is |
|---|---|
| the copies were really made and really verified first | `mut-backup-lost/run.log:22-24` — `cmp byte for byte OK, 307 bytes` / `107 bytes`, then `apply-capable : entered` |
| they were then deleted, at a named place | `mut-backup-lost/run.log:26` |
| **the trap called it a failure, not a restoration** | `mut-backup-lost/restore.log:9` |
| and it did not just miss them — it knew what they should have been | `mut-backup-lost/restore.log:7-8` — `(absent) bytes, verified at 307`, `(absent) bytes, verified at 107` |
| it recorded the live rules it could no longer check | `mut-backup-lost/restore.log:10-13` |
| a distinct result, the driver's status kept beside it | `mut-backup-lost/run.log:29-32` — `RESTORATION FAILED -- 1 step(s) did not verify`, `the drivers' own status was 1; this script exits 90` |

The decisive line:

```
[RESTORATION FAILED] the Apply-capable phase WAS entered and a firewall backup is missing, empty
or short (user.rules usable=0, user6.rules usable=0): the live rules can be compared with nothing,
and that is NOT evidence that nothing changed
```

*(This is a sixth run beyond the five the ticket names. It is here because the ticket's own wording
for the trap — "a missing or short backup is a RESTORATION FAILURE, recorded as such, never
`[RESTORED]`" — is a rule, and this folder's whole ethic is that a rule which has never been watched
failing is a claim.)*

## 3. Mutation — a matching line planted in the gap, and the restarted server silent

`mut-stale-window.sh` stages two things and puts both back:

* **the planted line**, by the runner itself under `T23_PLANT_IN_GAP=1` at `run-t3.sh:369-379` —
  written into `ac-authserver`'s own log stream through `/proc/1/fd/1`, after the precheck and before
  `docker restart`. It is marked `[T23-PLANTED]` so that a later reader of the container's log is not
  deceived by it, and it still matches what the check greps for (`at 100.99.204.5:8085`), which is
  the whole point.
* **the silence**, by lowering `authserver.conf`'s `Logger.root` from 4 (Info) to 3 (Warning) for the
  length of the run. `Added realm` is an Info line, so the restarted process writes none — Codex's
  exact case, *"if the restarted process announces nothing"* — while the server itself is untouched
  and goes on serving the realm at the address the row holds. The conf is copied aside first and put
  back afterwards, compared with `cmp`, which is the same rule this ticket writes for ufw:
  `mut-stale-window/mutation.log:5-6` and `:19-22` — the same sha256 `88604496…` on both sides, and
  `authserver.conf is byte for byte what it was (cmp)`.

| what had to happen | where it is |
|---|---|
| the precheck passed, as it always did — it is not the guard | `mut-stale-window/restore.log:19-20` — `0 (expected 0)` |
| the line was planted **in the gap** | `mut-stale-window/restore.log:22-26` — `planted (docker exec exit 0)` |
| the container really restarted, and the rule says how it knows | `mut-stale-window/restore.log:21`, `:28` — StartedAt `17:14:00.021866649Z` → `17:14:44.003189041Z` |
| the stamp handed to Docker is printed, verbatim | `mut-stale-window/restore.log:29` — `the stamp handed to Docker, verbatim: --since '2026-09-10T17:14:44.003189041Z'` |
| **the run went RED** | `mut-stale-window/restore.log:32-34` |
| a distinct result and a non-zero exit | `mut-stale-window/run.log:28-31` — `RESTORATION FAILED -- 1 step(s)`, `RUNNER EXIT STATUS … 90` |

The decisive line:

```
  the LAST 'Added realm' line written since 2026-09-10T17:14:44.003189041Z:
    (none)
  [RESTORATION FAILED] ac-authserver's newest announcement since its own StartedAt
  2026-09-10T17:14:44.003189041Z is '(none)', not 'at 100.99.204.5:8085'
```

**And the contrast is measured, not asserted.** `mut-stale-window/contrast.txt` reads the *same*
container log through the *two* windows, using the two stamps the run itself wrote into
`restore.log`, and it is taken before the authserver is restarted with its voice back so that no
later announcement can stand in for the planted one:

```
round 4's window  (a stamp off the clock, taken before the restart): --since '2026-09-10T17:14:43Z'
round 5's window  (the restarted container's own StartedAt)        : --since '2026-09-10T17:14:44.003189041Z'

--- what round 4's rule would have read: the LAST 'Added realm' line since 2026-09-10T17:14:43Z ---
[T23-PLANTED] Added realm "Yulon ubuntu2" at 100.99.204.5:8085.
--- what round 5's rule actually read: the LAST 'Added realm' line since 2026-09-10T17:14:44.003189041Z ---
```

One second apart, and one of them passes a restoration that never happened. That is the finding, on
this box, in this log.

## 4. Failed restoration — `failrestore/`

The same runner, the failing stub driver, and `T3_REALM_ROW_ID=9999` so the realm restore's `UPDATE`
targets a row that does not exist while the verification reads `id 1`.

| what had to happen | where it is |
|---|---|
| the drivers' status is kept, not overwritten | `failrestore/restore.log:2` — `the status the drivers left: 1` |
| the injection is on the record | `failrestore/restore.log:4` — `realm UPDATE targets row id: 9999   (verification always reads id 1)` |
| **the realm restore is caught** | `failrestore/restore.log:17` |
| and so is what it caused | `failrestore/restore.log:29` — the newest announcement names `192.168.77.77:8085` |
| the firewall backup was there and matched, so that step passed on its merits | `failrestore/restore.log:6-11` |
| the other restorations still ran | `failrestore/restore.log:34`, `:37`, `:38` |
| a distinct result and a non-zero exit | `failrestore/restore.log:39`, `failrestore/run.log:28-31` — `RESTORATION FAILED -- 2 step(s)`, `RUNNER EXIT STATUS … 90` |
| the row put back by hand straight after | `failrestore/hand-restore.txt` |

The decisive line:

```
[RESTORATION FAILED] realmlist id 1 reads '192.168.77.77|192.168.77.77|255.255.255.0',
wanted '100.99.204.5|100.99.204.5|255.255.255.0' -- the row this box advertises is WRONG
```

Note what the boundary rule bought here: `failrestore/restore.log:29` fails on the *restarted
server's own* wrong announcement, read through a window that begins at that container's
`StartedAt` (`:24`). Round 3's `--tail 80` passed this same case on a stale line; round 4's clock
window could have been fed one.

`failrestore/hand-restore.txt` puts the row back at once and reads it back on all three columns,
and its authserver reading is bounded the same way — `StartedAt before` `17:15:56.879945711Z`,
`after` `17:16:40.300095261Z`, `StartedAt moved, so the window below is this restart s own`, then
`Added realm "Yulon ubuntu2" at 100.99.204.5:8085.` The row held `192.168.77.77` for about 45 s;
the world was up throughout and only a new login would have been redirected.

## 5. Failed driver — `failclosed/`

The same runner and the same stub, with the restoration able to succeed.

| what had to happen | where it is |
|---|---|
| the driver exits 1 and the runner **stops there** | `failclosed/run.log:25-26` |
| the run is recorded as FAILED, not finished | `failclosed/run.log:28` — `T3 run FAILED, status 1, box restored and verified` |
| the runner's own exit status is the driver's | `failclosed/run.log:29` — `RUNNER EXIT STATUS (as seen by the invoking shell): 1` |
| the trap sees the damage | `failclosed/restore.log:14` — `as found now (id 1): 192.168.77.77\|192.168.77.77\|255.255.255.0` |
| and undoes it, and checks that it did | `failclosed/restore.log:15-17` |
| the authserver re-reads it, in its own restart's window | `failclosed/restore.log:21-29` |
| every step said so | `failclosed/restore.log:39` — `every restoration verified; this script exits 1` |

The decisive line:

```
=== T3 run FAILED, status 1, box restored and verified 2026-09-10T19:17:20+02:00 ===
```

`failclosed/clause35-falsify.log` carries the stub's output under the driver's slot name, because
the runner names each log for the driver it ran; its first lines say plainly which file it is. The
same is true in `failrestore/`, `mut-stale-window/` and `mut-backup-lost/`.

## 6. Green — `green/`

The real drivers: `clause35_falsify.py` first (it is the one that presses **Apply**), then the whole
33-clause widget half.

| what had to happen | where it is |
|---|---|
| the firewall was backed up and the copies verified before anything could press Apply | `green/run.log:22-24` |
| the falsification held, both halves watched failing and then passing | `green/clause35-falsify.log:23`, `:24`, `:30`, `:39`, `:40`, `:55`, `:58`, `:61` |
| **33 of 33** | `green/widget-run.log:127` |
| the corrected clause passes on the same disagreement that made the old one fail | `green/widget-run.log:60-62` |
| every restoration verified, in the restart's own window | `green/restore.log:5-39` |
| the runner's own exit status | `green/run.log:28` — `RUNNER EXIT STATUS (as seen by the invoking shell): 0` |

The decisive line:

```
RESULT: 33 OK, 0 FAIL
```

and beside it, the one this whole gate exists for:

```
[OK]   the plan the widget shows names the LAN address the widget itself computed --
       plan.lan_ip='172.26.8.248', row='100.99.204.5', they DISAGREE -- which this clause no
       longer depends on
```

`green/clause35-falsify.log:61` reads `RESULT: 5 OK, 2 FAIL -- of which 2 FAILs were the point
(A2 and B3)`; A2 falsifies the proposal half by mutating the plan object, B3 the apply half by
reading the row before the button is pressed, and B5 is the same expression passing after it.

**One reading has moved since 09-09 and is not smoothed:** the box's LAN address is `172.26.8.248`
tonight, where the 09-09 run computed `172.30.48.189`. The clause compares the plan to the widget's
own `plan.lan_ip`, so it does not care which address that is — which is the correction this folder
records. The row it prints beside it is unchanged at `100.99.204.5`, and they still disagree.

---

## The process was alive at every capture

`probe()` now writes a `docker inspect` section (`run-t3.sh:163-170`) beside `docker ps`, because
`docker ps` says a container is up and does not say *which run* of it is up — and `StartedAt` is the
same field the boundary rule is built on. Every `state-before.txt` and `state-final.txt` in this
folder carries, for all three containers:

```
/ac-worldserver Running=true Pid=91802 StartedAt=2026-09-10T16:48:48.793295165Z Restarting=false
/ac-authserver Running=true Pid=… StartedAt=… Restarting=false
/ac-database Running=true Pid=1634 StartedAt=2026-09-10T10:47:55.6428276Z Restarting=false
```

The world's pid is `91802` in all thirteen of those files (six `state-before.txt`, six
`state-final.txt`, and `green/state-after.txt`) and in `session-before.txt` and `session-after.txt` — the process T13's live half left up at 18:48, never stopped, started or
restarted by this lane. Lines 19-21 of each run's `run.log` also carry the authserver's `StartedAt` and the
realm row as the run found them, and each `restore.log` records the trap's own before/after pair.

## The box, before and after

`session-before.txt` (19:13:19) and `session-after.txt` (19:21:07) are the pair.

| | before | after |
|---|---|---|
| world | `Running=true Pid=91802 StartedAt=2026-09-10T16:48:48.793295165Z` | the same, `Up 32 minutes` |
| realm row id 1 | `100.99.204.5 \| 100.99.204.5 \| 255.255.255.0`, port 8085 | the same |
| `ac-authserver` | up, announcing `Added realm "Yulon ubuntu2" at 100.99.204.5:8085.` | up, the same line, read through its own `StartedAt` |
| `ufw` | inactive; `320f53e1…` / `f6696cd7…`, `root:root 640` | identical, byte for byte |
| non-bot accounts | `101 YULON_243C46E3`, `102 PERZI`, `103 YULONADMIN` | the same three, same gm levels, 0 orphaned `account_access` rows |
| `WIDGET0909T3` | absent | absent (0 rows) — created by clause 11 of the green run, deleted by the trap |
| `PERZI` | `last_login 2026-09-08 23:24:16`, `Pakka` level 6 | unchanged; **`PERZI` was never used** |
| `LootPet2.lua` | 37294 bytes, `Sep 9 02:06` | unchanged, with the five bridge scripts beside it |
| `Logger.ALE=4,Console Server` | line 706 of `worldserver.conf` | line 706; `worldserver.conf` md5 `e796513022d3cfa422ea13dec817ae6e` both readings |
| `Logger.root=4,Console Auth` | line 432 of `authserver.conf` | line 432; md5 `a86f2f64abb0913ab2a13c84ba35921e` both readings |
| characters | 1001 | 1001 |
| code under test | `528f219e` | `528f219e` |

`ac-authserver` was restarted eight times by this session — once by each of the six runs' traps,
once by `mut-stale-window.sh` putting the conf back, and once by the hand restore of the realm row
(`failrestore/hand-restore.txt:9-11`; the reviewer counted the `StartedAt` chain) — plus twice more
during the two mechanism checks described under "What was tried before it was written" below. `ac-worldserver` and
`ac-database` were never restarted.

**One reading needs saying plainly, because it looked like damage and was not.** The first
`session-after.txt`, taken 37 s after the last `docker restart ac-authserver`, read
`realmlist.flag = 2` (`REALM_FLAG_OFFLINE`) where `session-before.txt` read `0`. Nothing in this
lane writes that column — the only realmlist `UPDATE` it runs names `address`, `localAddress` and
`localSubnetMask` (`run-t3.sh:290`), and the app's own is `pylauncher/yulon/networking.py:2888-2894`, the same three.
The row was watched once every ten seconds for 45 s and read `0` every time; the reading that is
committed here is the settled one. It is recorded because a reader comparing the two files later
would otherwise have to guess, and because a reading taken seconds after an authserver restart is a
reading of a realm that has not been marked online again yet.

---

## What was tried before it was written

Two mechanisms this folder depends on were measured on the box first, rather than assumed:

* **Can a line be planted into `ac-authserver`'s own log stream?** Yes — `docker exec … printf …
  > /proc/1/fd/1` appends to the container's stdout, which is what `docker logs` reads, and the
  json-file driver stamps it at the moment it is written. That is why a line planted before
  `docker restart` is inside a clock window and outside `StartedAt`.
* **Does `docker logs --since` accept `StartedAt` verbatim?** Yes. `StartedAt` is RFC3339Nano in UTC
  with a `Z` (`2026-09-10T17:14:44.003189041Z`), and the query returns the restarted container's own
  announcement and nothing earlier. It is handed to Docker unmodified — not truncated to the second,
  which would open the window up to a second early and let part of the gap back in.

Both checks restarted `ac-authserver`; the second also edited `authserver.conf` and restored it by
`cmp`, which is where the `Logger.root` silencer came from.

---

## Files

| file | what it is |
|---|---|
| `run-t3.sh` | the fixed runner — the two rules are written in it at `:339-349` and `:498-508`/`:233-243`, and it is byte for byte what ran (`md5 8ea7853f9e93fe207f860d11696897db` here and at `/home/pk/lane710b/run-t3.sh`) |
| `invoke-t3.sh` | the invocation, committed: it runs the runner and writes the `RUNNER EXIT STATUS` line into `run.log`, so that number has a file behind it. Its header carries the six exact commands the six runs were started with |
| `mut-stale-window.sh` | mutation 1's own script: the conf copied aside, the silencer, the run, the conf restored by `cmp`, the contrast measured, the authserver given its voice back |
| `mut-backup-refused.sh` | mutation 2's own script: `chattr +i` on both copy targets, the run, the flags off again, `/etc/ufw` read before and after |
| `failclosed_stub.py` | the stand-in driver four of the six runs use: it moves the realm row and exits 1. Copied unchanged from the 09-09 folder |
| `green/` | the real drivers: `widget-run.log` (33 OK, 0 FAIL), `clause35-falsify.log`, `restore.log`, `run.log`, the three state probes, the two firewall copies, and `shots/` |
| `failclosed/` | a **driver** deliberately fails; the trap runs and every step verifies; exit 1 |
| `failrestore/` | a **restoration** deliberately fails; two `[RESTORATION FAILED]` lines, exit 90, the other steps still done, and `hand-restore.txt` |
| `mut-stale-window/` | mutation 1, with `contrast.txt` and `mutation.log`; exit 90 |
| `mut-backup-refused/` | mutation 2, with `mutation.log`; exit 70; **no driver log, because no driver ran** |
| `mut-backup-lost/` | the trap half of mutation 2; exit 90; no `ufw-*.before`, because they are what was deleted |
| `session-before.txt`, `session-after.txt` | the box either side of all six runs, taken by hand at an ssh prompt with every value read from the box |
| `gates.txt` | the `test_no_secrets_in_evidence.py` result for the commit this folder ships with |

`mut-backup-refused/`, `mut-backup-lost/`, `mut-stale-window/`, `failclosed/` and `failrestore/`
have **no `state-after.txt`**, and that is not an omission: `probe after` runs only after both
drivers are green (`run-t3.sh:596`), and in those five invocations the first driver never finished
green. Their `state-before.txt` and `state-final.txt` are the pair — before anything, and after the
trap. `mut-backup-refused/` also has no `clause35-falsify.log`, which is the capture.

The two `ufw-user*.rules.before` files in `mut-backup-refused/` are **0 bytes**: they are the
immutable targets the mutation created, and the runner's `cp` never wrote them. That is the
mutation, not a truncated artefact.

**No generated password is in this folder.** The only credential-shaped value anywhere in it is
`password`, the AzerothCore compose fixture's default root password, which is not generated, is the
same on every install this project makes, and reaches nothing off this box (`ac-database` publishes
`127.0.0.1:3306->3306/tcp`, loopback only — line 6 of every `state-before.txt`). It is passed through
`MYSQL_PWD` in the environment and is on no argv (`run-t3.sh:141-151`). The widget account's
throwaway password at `run-t3.sh:107` belongs to an account that no longer exists.
`test_no_secrets_in_evidence.py` was run before the commit and is in `gates.txt`; the folder was
also grepped by hand for `passw|pwd|secret|token|SOAP|verifier|salt|MYSQL_PWD`, and the six hits are
the ones just described.

## What this folder does not claim

* **The `SIGINT`/`SIGTERM` path is still unexercised.** The trap is installed on `EXIT`, which those
  signals reach, but no run here was interrupted to watch it.
* **`chattr +i` is one way to make `cp` fail, not all of them.** What was measured is that the
  runner refuses and does not start the driver when its `cp` returns non-zero. A `cp` that returns 0
  having written a different file is caught by the separate `cmp` at `run-t3.sh:519`, which was
  exercised only in its passing direction.
* **The silencer is a config change, not a broken server.** During `mut-stale-window/` the
  authserver was serving the realm correctly at `100.99.204.5:8085`; only its Info log was off. What
  the run shows is that the *check* fails when it has no announcement to read — which is the case
  Codex named — and not that a silent authserver is a broken one.
* **The code under test is unchanged at `528f219e`**, the same sha the 09-09 folder measured, so the
  33 clauses are comparable with it clause for clause. T23 is about the runner; nothing under
  `pylauncher/` is touched by this ticket.
