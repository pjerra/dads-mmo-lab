# The rollback's RESTORE arm, pressed live — WoW WotLK on `yulon-ubuntu2`, 2026-09-09

**It works.** A build that compiled and would not come up was put back automatically: the old
images' tags were restored, the containers recreated on them, and the world reported ready again
**74 seconds after it went down**, with the app's own sentence naming the failure, quoting the dead
server's last words and saying what the rollback does not cover. Every image under the running stack
at the end is the id the ground recorded.

**It also found a defect, which was fixed test-first and re-pressed.** The `-failed` name that
`_restore_rollback()` gives the new build was let go while the containers made FROM that build were
still there, so docker refused it for both long-running services and nothing ever asked again: on
press 1 the box was left holding 840 MB of a build known to be broken, under a name
`FAILED_TAG_SUFFIX` documents as transient. Press 2, on the fixed tree, ended with no `-failed`
name at all.

**Box:** `yulon-ubuntu2` (12 vCPU, 16 GB), the WotLK install at `/home/pk/wowserver`
(`install_id 243c46e3`, built with `mod-ale`, 500 playerbots, the command channel verified).
**Tree:** this lane's worktree at `bbf730df` for press 1, and `bbf730df` + this lane's fix for
press 2, exported to `~/lane-restore`. **Checkpoint:** `before-rollback-restore-test-2026-09-09`
(Standard, 2026-09-09 02:56:45 host local = 00:56Z), which already existed when this lane started
and predates everything below. A second one was NOT taken: this VM has 16 GB assigned and the last
checkpoint of it cost 7.85 GB of `.VMRS`, against **34.81 GB free on the host's `U:`** — half the
free space on the drive the build writes into, to duplicate a checkpoint an hour old that nothing
had been written over. Named here rather than skipped quietly. Every action was announced on the
box's activity terminal first (`~/claude-activity.log`).

**Left running:** the whole stack, up, on the images the ground recorded, `mod-ale` compiled in, the
bridge answering. See *Left on the box*.

Captures are the VM's own desktop photographed from the Hyper-V host with `vmshot.ps1` (`show.sh`
puts one reading on that desktop; GNOME refuses an in-guest screenshot driven from ssh), except
`7-the-apps-own-rebuild-panel-press2.png` — see its row. `ac-worldserver`'s liveness is printed
under every capture and quoted below, because **a server that has died photographs exactly like a
refusal**.

---

## Verdict, clause by clause

| Clause | Verdict | Evidence |
| --- | --- | --- |
| the ground was read before anything was broken | **PASS** | `1-ground.log`, `ground-press1.json`, `1-…png` |
| the `-rollback` tags existed **during the compile**, read from a SECOND process | **PASS** | `tag-watch-press1.log`, `events-press1.log`, `2-…png` |
| the new build was tagged `-failed` rather than deleted | **PASS**, and its whole life is 0.49 s | `events-press1.log`, `3-…png` |
| the images under the running stack are the ones from the ground, the same ids | **PASS** | `5-after-press1.log`, `11-after-press2.log`, `5-…png`, `6-…png` |
| the world is up and the bridge answers again | **PASS** | `5-after-press1.log` (`DML-BRIDGE-READY`), `14-box-back.log` |
| the app's own sentence names what was put back and what was not | **PASS**, with one thing it does not name — see *§5* | `press1-message.txt`, `4-…png`, `7-…png` |
| **the `-failed` name is let go once the restore has settled** | **FAILED on press 1**, fixed test-first, **PASS on press 2** | `press1.log`, `press2.log`, `6-…png`, `15-what-was-left.log` |

---

## The ground, read before anything was broken

`1-ground.log`, 01:59:15Z (`1-ground-images-tags-world-bridge.png`, `ac-worldserver running
pid=170230 restarts=0`):

| Fact | At the start |
| --- | --- |
| the four images this install builds | `authserver 1024b0a2fb43`, `client-data ffa9094ed5ba`, `db-import a55f34201a7a`, `worldserver cad2566406d2` |
| `ac-worldserver` | `running id=2754d6dcd5d7 pid=170230 restarts=0 image=cad2566406d2` |
| `ac-authserver` / `ac-database` | `running … image=1024b0a2fb43` / `running … image=3466ba4a4828` |
| `-rollback` tags | **`[]`** |
| `-failed` tags | **`[]`** |
| the bridge | `dml_bridge_ping` → `outcome='yes'`, `'DML-BRIDGE-READY dml_bridge_ping\r\n'` |
| the scripts on disk | the five `dml_*.lua`, plus the owner's `LootPet2.lua` |
| `worldserver.conf` | `SOAP.Enabled = 1`, `SOAP.IP = "127.0.0.1"`, `SOAP.Port = 7878`, `Logger.ALE=4,Console Server` |
| the realm row | `1 │ Yulon ubuntu2 │ 100.99.204.5 │ 8085` |
| the accounts | `101 YULON_243C46E3 gm=3`, `103 YULONADMIN gm=3` |
| characters / mail rows | `1001` / `163` (and the per-receiver breakdown, 100 rows) |
| `updates` applied | world `2967`, characters `29`, auth `22` |
| the sabotage marker | **absent from `Main.cpp` and absent from the running world's log** |
| host `U:` free | **34.81 GB** (`2-host-disk-before.log`) |

Everything after this is compared against `ground-press1.json`, the file that reading wrote — not
against a second derivation of the same numbers.

**The state block was verified, not inherited, and two of its lines are wrong on this box.** The
brief said the owner's script is `LootPet.lua`; what is there is `LootPet2.lua` (37 294 bytes, last
written 00:06:55Z — two hours before this lane touched anything) beside
`LootPet.lua.bak-20260909-002427`. Both were left exactly alone, and both are still there
(`14-box-back.log`). The brief also named `YULONADMIN` as the command-channel account; the
credential the app actually holds and answers with is `YULON_243C46E3` (`7-…png`, the tab's own
line: *verified as YULON_243C46E3 at 2026-09-08 20:25 UTC*). `YULONADMIN` exists at GM 3 as
described, but it is not the one the channel uses.

---

## 1 — how the build was made to fail, and why that shape

`3-sabotage.log`. One statement at the top of AzerothCore's own
`src/server/apps/worldserver/Main.cpp`, in the checkout the build reads:

```cpp
 int main(int argc, char** argv)
 {
+    if (argc >= 0)
+    {
+        std::cerr << "YULON-GATE-SABOTAGE: this worldserver build exits immediately …" << std::endl;
+        return 1;
+    }
     Acore::Impl::CurrentServerProcessHolder::_type = SERVER_PROCESS_WORLDSERVER;
```

Shape (a) of the brief's two: **a source change that compiles and yields a worldserver that exits
at once.** It breaks the build's OUTPUT and nothing else — `native.py` was not patched and no
container was killed by hand, either of which would have proved the harness rather than the feature.
Three details are deliberate:

* `if (argc >= 0)` rather than a bare `return 1`, so no compiler sees unreachable code and a
  `-Werror` build cannot fail for the wrong reason. The compile had to SUCCEED — `built` is what
  puts `rebuild()` on the restore path at all.
* `std::cerr` rather than `printf`: `<iostream>` is already included there, so the sabotage adds no
  header and cannot fail to compile for want of one.
* it prints before it exits, which is what makes the "show what the log said" half of owner answer 2
  checkable: the marker had to appear in the sentence the user reads, and it does.

**The ceiling was not lowered and did not need to be.** `READY_CEILING_SECONDS` is six hours, but a
worldserver that exits under `restart: unless-stopped` is a crash loop, and `docker.wait_ready()`
returns False the moment `RestartCount` has grown by `restart_loop` (4). Measured: the ready wait
started 02:04:10Z and raised at **02:04:15Z, five seconds later**.

---

## 2 — press 1, and the arc read from outside

`press1.log` (298 KB, every line the engine yielded), `tag-watch-press1.log` (a 3 s poll from a
second process), `events-press1.log` (`docker events --filter type=image`, the daemon's own clock).

```
02:02:37Z  Kept the build you have now as a rollback (4 images tagged -rollback).
02:02:37Z  build            ─┐ 73 s (one object recompiled, everything else cached)
02:03:50Z  recreate          │  the NEW build's containers replace the running ones
02:04:10Z  ready wait        │
02:04:15Z  ac-worldserver restarted 4 times while being waited on; that is a crash loop
02:04:15Z  Putting the build from before this rebuild back.
02:04:16Z  4 × -failed named, 4 × rollback tags moved back, 2 × -failed let go
02:04:16Z  recreate          │  the OLD build's containers
02:05:04Z  The server is up. ─┘  148.0 s in total; the world was down for 74 s
```

**The `-rollback` tags, from outside the press** (`2-rollback-tags-during-the-compile-second-process.png`):

```
--- 02:02:36Z ---  suffixed count: 0        world: running id=2754d6dcd5d7 image=cad2566406d2
--- 02:02:39Z ---  4 × …-rollback           world: running id=2754d6dcd5d7 image=cad2566406d2
--- 02:03:39Z ---  4 × …-rollback           world: running id=2754d6dcd5d7 image=cad2566406d2
--- 02:04:19Z ---  6 (4 rollback, 2 failed) world: restarting id=31769acad1c4 restarts=7 image=7c1ecf94b44e
--- 02:05:07Z ---  0 rollback               world: running id=a91156260035 image=cad2566406d2
```

Absent → present for the whole compile with the world untouched on the ground's image → the new
build up and crash-looping on `7c1ecf94b44e` → the world back on `cad2566406d2`. The image id under
the container is in every sample, because "the world is up" and "the world is up on the build we
started with" are two different facts and only the second one is this feature's claim.

**The `-failed` name, from the daemon's own event stream** (`3-failed-tag-named-before-the-tags-move.png`):

```
02:04:16Z.021  tag    …worldserver:native-243c46e3-failed     ← the new build named FIRST,
02:04:16Z.077  tag    …authserver:native-243c46e3-failed        before a single tag moves
02:04:16Z.136  tag    …db-import:native-243c46e3-failed
02:04:16Z.191  tag    …client-data:native-243c46e3-failed
02:04:16Z.252  tag    …worldserver:native-243c46e3            ← the rollback tags going back
02:04:16Z.312  tag    …authserver:native-243c46e3
02:04:16Z.373  tag    …db-import:native-243c46e3
02:04:16Z.431  tag    …client-data:native-243c46e3
02:04:16Z.509  untag  …db-import:native-243c46e3-failed       ← and let go again
02:04:16Z.544  untag  …client-data:native-243c46e3-failed
```

**0.49 seconds, end to end.** No poll can be relied on to land inside that, which is why the events
stream is here at all — and why it was probed before it was trusted (`0-instrument-probe.log`). The
probe earned its place immediately: its first version passed `--until int(time.time())`, which
truncates DOWN, so the two events it had just caused fell outside its own window and it reported the
instrument dead. A gate that had skipped the probe would have read that same emptiness after the
press as "the app never named the new build".

---

## 3 — the defect press 1 found

**A `-failed` name docker refuses is never asked about again, and on the two long-running services
docker always refuses it.**

`_restore_rollback()` calls `_let_go(named)` the moment the rollback tags are back — which is
*before* its own `stage_recreate()`. At that instant the containers made FROM the new build are
still there, and docker will not remove a name whose image a container references
(`press1.log`, and reproduced independently in `press2.log` with different container ids):

```
04:04:16 WARNING could not remove image …ac-wotlk-worldserver:native-243c46e3-failed:
    conflict: unable to delete … (must be forced) - container 31769acad1c4 is using its
    referenced image 7c1ecf94b44e
04:04:16 WARNING could not remove image …ac-wotlk-authserver:native-243c46e3-failed: … 9b1a5a67a394 …
04:04:16 INFO    removed image …ac-wotlk-db-import:native-243c46e3-failed
04:04:16 INFO    removed image …ac-wotlk-client-data:native-243c46e3-failed
```

The two that succeeded are the one-shots, whose containers the recreate never touches; the two that
failed are exactly the services the restore is about to replace. **So it is not bad luck: on this
shape it happens every time**, and press 1 ended with `-failed tags = ['…authserver…-failed 336MB',
'…worldserver…-failed 504MB']` — 840 MB of a build known to be broken, kept for ever under a name
whose own docstring says "removed once the restore has settled either way".

It is the mirror image of what 2026-09-08's press found from the other end (a `-rollback` tag on the
one-shots surviving `_let_go` because their exited containers still referenced the image). Same
docker rule, opposite half of the stack, and this half is deterministic.

**Driven by test first.** RED, with the message recorded:

```
tests/test_rebuild.py::test_the_failed_name_is_let_go_once_the_containers_holding_it_are_replaced
E  AssertionError: yulon.local/ac-wotlk-worldserver:native-16d468d5-failed was only ever let go
   while the container holding it was still there, so the refusal stands and the name is left on
   the daemon for ever: [… 'rmi:…-failed', 'rmi:…-failed', 'rmi:…-failed', 'rmi:…-failed',
   'recreate', 'rmi:…-rollback' …]
E  assert 14 > 18
```

**The seam that hid it**, again the double: `Recorder.remove_image` is documented as "always allowed
here", so no unit test could ever see a refusal. The new test hands the engine a `remove_image` that
refuses the way this daemon does — while `rec.calls.count("recreate") < 2` — and asserts on the
LAST attempt rather than on any attempt, because an attempt made while the blocker is still in place
proves nothing about the name being gone.

**The fix** is three lines: ask again, immediately after the restore's own recreate, which is the
thing that frees them. The first call is kept, because the failure paths above it never reach a
recreate, and a name docker has already let go is a no-op (`remove_image` reads "no such image" as
done). `FAILED_TAG_SUFFIX`'s docstring now records the measurement instead of the assumption.

Two repairs considered and dropped:

* *`docker rmi --force`* — it deletes an image a running container is using, and the container the
  refusal names may be one the recreate has not replaced yet. The refusal is docker protecting a
  container, not an obstacle.
* *let the name go before the recreate and accept the leak, but say so in the sentence* — the user
  would be told about a leftover they cannot act on, in the sentence that is already reporting a
  failed rebuild. A name the app can remove itself is not news.

---

## 4 — press 2: the same broken build, on the fixed tree

`press2.log`, `6-press2-the-failed-name-let-go-after-the-fix.png`. Same sabotage, still on disk;
the compile was 4 seconds because every layer was cached from press 1.

```
02:17:06Z  Kept the build you have now as a rollback (4 images tagged -rollback).
02:17:10Z  recreate  →  02:17:29Z
02:17:34Z  crash loop; Putting the build from before this rebuild back.
04:17:34   could not remove …worldserver…-failed (container 5a8bbfe60c57) ← refused again
04:17:34   could not remove …authserver…-failed  (container 885b95005fe4)
02:17:34Z  recreate  →  02:17:45Z
04:17:45   removed image …worldserver:native-243c46e3-failed   ← THE FIX, after the recreate
04:17:45   removed image …authserver:native-243c46e3-failed
02:18:15Z  The server is up.                                     69.5 s in total
after: -rollback tags = []      after: -failed tags = []
```

and the daemon agrees, `untag` and `delete` in the same millisecond
(`events-press2.log`): `02:17:45.801 untag …worldserver…-failed` /
`02:17:45.801 delete sha256:04e184df08ca…`.

The four images under the install's tags are `1024b0a2fb43 / ffa9094ed5ba / a55f34201a7a /
cad2566406d2` — **the ground's ids, all four** — the world is up on `cad2566406d2`, its current run
printed `ready...`, it does **not** print the sabotage marker, and `dml_bridge_ping` answers
`DML-BRIDGE-READY` (`11-after-press2.log`, `14-box-back.log`).

---

## 5 — the sentence the user reads, and what it does not say

`press1-message.txt` / `4-the-sentence-the-user-reads.png`, and the app's own panel carrying it as
its `FAILED:` header (`7-the-apps-own-rebuild-panel-press2.png`):

> The server started but never reported ready: ac-worldserver restarted 5 times while this waited,
> which is a crash loop and not a slow start. `docker compose logs ac-worldserver` in
> /home/pk/wowserver has what it printed before each one. **The build from before this rebuild was
> put back and is running again.** Before it was replaced, ac-worldserver had printed:
> ```
> cp: warning: behavior of -n is non-portable …
> Starting worldserver...
> YULON-GATE-SABOTAGE: this worldserver build exits immediately (rollback-restore press, 2026-09-09)
> ```
> What the new build wrote into the database on its first start, if anything, is NOT put back by
> this — the lines above say whether its updater ran — so the old build is running on the database
> as the new one left it.

The same string is in the install's own state file as `last_error`, so it survives the panel.
Owner answer 2 asked for three things — retag, bring the old one back, **show what the log said** —
and the third is the block in the middle: the dead server's own last words, read BEFORE the recreate
took its log with it.

**Is the database sentence true here?** It is true, and on this press it is also empty, which is
worth separating. Measured against the ground: world `updates` 2967, characters 29, auth 22, mail
163 rows, 1001 characters — **every one identical afterwards** (`11-after-press2.log`, all `SAME`).
Two reasons, and only one of them is the sabotage:

1. this worldserver exits before its own updater runs, so it wrote nothing;
2. **the rebuild's recreate never re-runs the importer at all.** `staged_up_argv()` issues
   `compose up -d --force-recreate --no-deps ac-database ac-authserver ac-worldserver` — the
   one-shot `ac-db-import` is not in that list, and `docker ps -a` shows it still `Exited (0) 6
   hours ago` after both presses.

So the Tortoise incident the sentence exists for — the new binary's own updater migrating the world
database at startup — was **not reproduced by this press**, and this folder is not evidence that the
sentence is needed. It is evidence that it is printed and that it is not lying.

**What the sentence does NOT say, and it matters.** An image rollback puts the BINARY back. It does
not touch the source tree the build read, so the thing that made the build fail is still on disk and
the next press of Rebuild builds the same broken server again. Nothing in the message, the
confirmation or `REBUILD_OPENING_NOTE` says so. This lane put the source back by hand
(`13-revert.log`, `git checkout -- src/server/apps/worldserver/Main.cpp`), which is a step the app
cannot do for the user — the change is usually a module they installed on purpose. Named here rather
than decided: whether the closing sentence should point at the modules folder is the owner's call,
and `phase8-decisions.md`, `phase8-parity-decisions.md` and the 2026-09-08 owner answers are all
silent on it. The Rust launcher answered the same question the other way for a different action —
`maint.rs:718` + `:988` roll the SOURCE repos back on a failed update and says
*"Everything was rolled back — the core and the module are exactly as they were, and nothing needs
rebuilding"* — but that is an update, where the app chose the source change; here it did not.

---

## What the restore cost, on this box

| | compile | recreate #1 | crash detected | restore + recreate #2 | ready | total |
| --- | --- | --- | --- | --- | --- | --- |
| press 1 (one object recompiled) | 73 s | 20 s | **5 s** | 11 s | 38 s | **148.0 s** |
| press 2 (every layer cached) | 4 s | 19 s | **5 s** | 11 s | 30 s | **69.5 s** |

The world was unavailable for **74 s** on press 1 and **65 s** on press 2 — from the first recreate
to the restored world's ready line. Disk: the host's `U:` went 34.81 GB free → 30.34 GB over press 1
and → 30.31 GB over press 2 (a cached rebuild costs almost nothing); the guest's build cache grew
19.52 → 23.39 GB. Per-box facts, measured on this box.

---

## Things left unexplained, and one thing cleaned by hand

1. **840 MB of dangling images, and where they came from** (`15-what-was-left.log`). Press 2 reused
   the `-failed` NAME for its own build, which moved that name off press 1's images — and a tag move
   does not delete what it moved off, so press 1's two never-let-go images became untagged rather
   than removed. Press 2's own broken build was deleted properly (`untag` + `delete`, same
   millisecond). The two orphans were removed by hand, by id
   (`docker image rm 7c1ecf94b44e 7d9b7c745a82`), and the daemon now holds exactly the eight images
   the ground recorded. **Note what this means for the fix's reach**: it makes the NAME go away, and
   on this daemon that also deleted the image — but only because nothing else referenced it. Nothing
   in this app prunes dangling images, and this folder does not claim it does.
2. **The two dangling images that were already there** (`dcaadcf6692d`, `92252d77483e`) predate this
   lane: the exited `ac-db-import` and `ac-client-data-init` containers still reference them, which
   is the same fact 2026-09-08's README recorded from the `-rollback` side. Untouched.
3. **`test_rebuild.py`'s `_answers(False, True)` is now load-bearing in two tests**, and the new one
   would pass for the wrong reason if the second wait ever answered False — the run would end on the
   "did not report ready either" branch, which also reaches no second `_let_go`. The assertion on
   `"put back and is running again"` is what pins it to the branch it means to test.

---

## Left on the box

* The stack **UP**, on the images the ground recorded: `ac-worldserver` `running id=cd68db8cd6b7
  pid=278886 restarts=0 image=cad2566406d2`, `ac-authserver` on `1024b0a2fb43`, `ac-database`
  healthy. `mod-ale` compiled in, the five bridge scripts loaded, `dml_bridge_ping` answering.
* `~/wowserver` **as it was**: `git status --porcelain` on `src/` is empty again, the owner's
  `LootPet2.lua` and `LootPet.lua.bak-20260909-002427` untouched, `worldserver.conf` still carrying
  `SOAP.Enabled = 1` and `Logger.ALE=4,Console Server`, the realm row still
  `100.99.204.5:8085`, the mail table still 163 rows across the same 100 receivers.
* **No `-rollback` and no `-failed` tags**, and no image on the daemon that was not there at 01:59Z.
* The checkpoint `before-rollback-restore-test-2026-09-09`, untouched. It predates the whole night,
  so restoring it would undo this and everything the 2026-09-08 lane did. Host `U:`: **30.31 GB**
  free.
* `~/lane-restore/` — this lane's tree, with the fix; `~/restore-gate-out/` — every log in this
  folder; `~/restore_gate.py`, `~/watch_tags.sh`, `~/watch_events.sh`, `~/show.sh`. The
  `~/lane-rebuild/` tree and `~/rebuild-gate-out/` from 2026-09-08 were left alone.

## What this press still did not show

* **A build whose failure the readiness check cannot see quickly.** This one crash-loops in five
  seconds. A build that comes up, prints for ever and never reports ready is the case that spends
  `READY_CEILING_SECONDS` before the restore starts, and six hours of that has never been run.
* **A restore that fails.** Both retag loops succeeded on both presses, so the three sentences for a
  refused or part-way restore, and the `-failed` name's actual undo path, are still unit-test-only
  (`test_a_restore_that_fails_halfway_puts_every_tag_back_on_the_new_build`).
* **The database half.** No press here made a new build write to the database, and the recreate's
  `--no-deps` means the importer cannot be what does it. The Tortoise shape — a worldserver whose
  own updater migrates the world DB and then dies — is still the open one, and it is the shape owner
  answer 2 was written for.
* **The `-rollback` tags surviving a purge or an uninstall.** `remove_image()` enumerates
  `built_image_refs()`, which are the plain refs; nothing here asked what an uninstall does with a
  suffixed leftover.
* **Any of this on another tree.** WotLK only. A per-tree fact is measured per tree: the crash-loop
  latch, the `restart: unless-stopped` policy that makes it one, and the `--no-deps` recreate are
  all facts about this family's compose plan.
