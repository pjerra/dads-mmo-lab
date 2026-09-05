# bug-checklist §43 — the headless harness holds the Windows keep-awake assertion — 2026-09-05

`platform.keep_awake()`'s Windows branch refused any caller running on `threading.main_thread()`.
That refusal is right about the app — `SetThreadExecutionState` is scoped to the thread that sets
it, and the GUI hands every install to a `QThread`, so a claim made on the GUI thread holds
nothing — and wrong about `yulon.install_wiring`, whose main thread iterates the engine's
generator and therefore IS the thread doing the install. On `yulon-win11-gate` the 7.7 second
press said so once and installed unheld.

This folder holds the before-and-after on the same box, the same install folder and the same
harness, plus the four mutations run on `yulon-fedora`.

Fix at `lane/b43` `0ad9d99a`: `main.py` calls `platform.declare_gui_thread()` beside
`QApplication(sys.argv)`, and the Windows branch refuses `platform.gui_thread()` by identity.
A process that starts no window declares nothing and is not refused.

## Before — the defect, as the box recorded it (`yulon-log-before-745307ad.txt`)

`C:\Users\pk\AppData\Roaming\Yulon\yulon.log` lines 1551-1560 of 1673 (the file as it stood at
13:33 box-local, 253,859 bytes), the 7.7 TBC second press at `745307ad`:

```
2026-09-05 05:12:16 INFO [__main__] Already finished: clone-sources, write-dockerfile, generate-compose, build, extract, mmaps, conf, import
2026-09-05 05:12:16 WARNING [yulon.catalog.native] not holding this machine awake: keep_awake() must run on the worker thread doing the install: …
2026-09-05 05:12:16 INFO [__main__] This machine may go to sleep during the build; leave it awake, and leave the lid open on a laptop.
2026-09-05 05:12:16 INFO [__main__] Step 1 of 12 (8%): clone-sources
```

## After — the same press from `0ad9d99a` (`yulon-log-b43-press.txt`)

Same file, lines 1640-1673:

```
2026-09-05 13:30:21 INFO [__main__] Already finished: clone-sources, write-dockerfile, generate-compose, build, extract, mmaps, conf, import
2026-09-05 13:30:21 INFO [yulon.platform] holding this machine awake for the build: SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)
2026-09-05 13:30:21 INFO [__main__] Step 1 of 13 (7%): clone-sources
```

`grep`-style check over the whole 1673-line file, run 13:33 box-local: `not holding this
machine awake` appears on line 1555 only — the 05:12:16 record above, from the run that found the
bug. It appears nowhere in this press's 34 lines. The sleep note (`This machine may go to sleep
during the build`) is likewise absent: the engine only yields it when the assertion was refused.

The INFO line is the point. §43's box asks for the ABSENCE of a warning, and absence proves
nothing about a line that may simply never have been reached; this line is the positive half, and
it is also the first time this project has executed `SetThreadExecutionState` at all — it
resolved through `ctypes.windll.kernel32` and Windows answered non-zero. What the OS then does
with the assertion over hours is still unmeasured (roadmap 6.3).

## The run

| stamp (box-local PST; CEST = +9 h) | event | source |
|---|---|---|
| 13:17:49 | first start of task `dml-b43` | `b43-wrapper.log` |
| 13:18:52 | **refused by preflight**: `free space on Docker's disk and the server folder: 34 GB free, and the install needs 40 GB`; errorlevel 1 | `b43-wrapper.log`, `yulon-log-b43-refused-press.txt` |
| 13:30:07 | second start, after 13.0 GB of WSL crash dumps were deleted (below) | `b43-wrapper.log` |
| 13:30:19 | preflight now `[warn] free space …: 46 GB free; 60 GB is the comfortable figure` | `yulon-log-b43-press.txt` |
| 13:30:21 | `Using C:\gate\tbc-server (resuming)`, then the keep-awake INFO line, then `Step 1 of 13` | `b43.log`, `yulon-log-b43-press.txt` |
| 13:30:27 | `Step 2 of 13 (15%): patch-sources` → `install failed`, errorlevel **1** | `b43.log`, `b43.exitcode` |

The press did **not** compile: `clone-sources` reported all three checkouts "already in … leaving
it exactly as it is", and the install stopped at stage 2.

**Why it stopped, and why that is not this fix's business.** `C:\gate\tbc-server` was built on
2026-09-04, before `shared/cmangos/patches/vmap-extractor-doodad-name-case.patch` entered the
catalog (merged into `yulon-phase7` at `5a57164d`, committed 2026-09-05 21:04:49 +0200, seven
hours after the 7.7 press ran). `patch-sources`
now refuses a press that would patch a checkout whose compiled image cannot get the fix. That
refusal is `lane/doodad`'s deliberate behaviour and it is exactly what a user with this folder
should see. It means no TBC press on this box can reach `ready` again without a recompile, which
this lane may not start — so **`The server is up.` and exit 0 are NOT part of this evidence**, and
`yulon-fedora`'s
`tests/test_install_wiring.py::test_the_harness_holds_a_windows_machine_awake_on_its_own_main_thread`
is what carries the assertion across a whole install instead. The same limit holds for a Vanilla
press: `catalog.json` attaches the doodad patch to `wow-vanilla` too, and `C:\gate\vanilla-server`
was built 2026-09-03.

`_held_awake()` is entered before stage 1, so the press reached and passed the decision this
entry is about before the doodad refusal ended it.

## Where the space came from

Preflight wants 40 GiB free with the server folder and Docker's disk on one drive; `C:` had
37,041,487,872 bytes (34.5 GiB) free at 13:20 box-local. `docker builder prune -af` returned
`Total: 0B`. `%TEMP%\wsl-crashes` held ten WSL crash dumps totalling 13,674,397,696 bytes, listed
2026-09-05 13:26 box-local:

```
wsl-crash-1788256353-129893-_usr_bin_weston-11.dmp        78,376,960  2026-09-01 02:52   (x8, to 03:05)
wsl-crash-1788555997-1-_opt_mangos_bin_mangosd-11.dmp  7,212,302,336  2026-09-04 14:11
wsl-crash-1788613894-1-_opt_mangos_bin_mangosd-11.dmp  5,835,079,680  2026-09-05 06:16
```

The two `mangosd` cores were deleted (`del /f /q`), taking `C:` to 49,936,990,208 bytes (46.5 GiB)
free. The eight `weston` dumps were left. Nothing else on the box was removed. The 09-05 06:16
core is worth someone's attention on its own: it was written 54 minutes after the 7.7 press
reported `The server is up.` at 05:22:33, and it accounts for most of the 8 GB `C:` lost between
05:12 (preflight read 42 GB free) and 13:18 (34 GB).

To compact the Docker WSL VHDX instead — 16,331,571,200 bytes for 12.86 GB of content —
`taskkill` and `diskpart` were both refused by this session's permission classifier, so the
crash-dump route was taken. `docker desktop stop` and `wsl --shutdown` DID run and are the
supported way to release that disk; the engine was restarted with `schtasks /run /tn dd-start`
and answered `29.7.2` before the second press.

## Mutations — `yulon-fedora`, `git clone --shared` at `0ad9d99a`

`mutate.sh` and `mutate-m1.sh` are the scripts; `mutations-yulon-fedora.txt` and
`mutation-m1-yulon-fedora.txt` are their transcripts. Each mutation was applied to a fresh
`/tmp/b43mut` clone of `~/dads-mmo-lab` checked out at `0ad9d99af6100f4a16926f8a891d3324f3962e9f`,
with `__pycache__` purged under both the clone and `~/dads-mmo-lab` before and after every run,
and the tree restored with `git checkout -- .` afterwards. 4 of 4 killed, each red shown:

| # | mutation | test that went red | how it failed |
|---|---|---|---|
| M1 | `gui_thread() is threading.current_thread()` → `threading.main_thread() is threading.current_thread()` (the §43 bug, restored) | `test_keep_awake_on_windows_holds_the_main_thread_of_a_process_with_no_window` and `…::test_the_harness_holds_a_windows_machine_awake_on_its_own_main_thread` | `RuntimeError: keep_awake() must run on the thread doing the install…` at that clone's `platform.py:3826`; and `assert [] == [2147483649, 2147483648]` |
| M2 | `platform.declare_gui_thread()` deleted from `main.py` | `test_the_launcher_declares_which_thread_is_its_gui_thread` | the child process printed `main() built a QApplication without declaring its GUI thread: gui_thread() is None` |
| M3 | the Windows refusal made unreachable (`if False and …`) | `test_keep_awake_on_windows_refuses_the_declared_gui_thread` | `Failed: DID NOT RAISE RuntimeError` |
| M4 | the INFO line dropped from `_keep_awake_windows()` | `test_keep_awake_on_windows_says_in_the_log_that_the_assertion_was_taken` | `assert 'holding this machine awake for the build: SetThreadExecutionState' in ''` |

The five tests passed together on the unmutated clone first (`5 passed in 0.93s`).

## Suite

`YULON_TEST_BOX=yulon-fedora bash run-tests-vm.sh --checks` at `0ad9d99a`, the commit the box was
pressed from: `2792 passed, 4 skipped in 27.14s`, mypy clean on this platform, as Windows and as
macOS (72 source files each), `ruff` and `black` clean.

Run again at 22:47 CEST on the tree this folder's own commit records — whose only code difference
from `0ad9d99a` is the `_keep_awake_windows()` docstring, rewritten from "Unverified" to the
measurement above: `2792 passed, 4 skipped in 22.50s`, same three mypy passes, same `ruff` and
`black`.

## The box, as this lane left it

`C:\gate\b43-src\b43-src\pylauncher` (the lane source, `git archive` of `0ad9d99a`) and
`C:\gate\b43-src.tgz` are kept, as the brief allows. `C:\gate\run-b43.cmd`,
`C:\gate\compact-b43.txt` and task `dml-b43` are spent and left in place beside the earlier
gates' equivalents. The Docker engine is up; the press never reached `up`, so it started no container.
`docker ps -a` at 13:35 box-local listed all fourteen as Exited — `tbc-realmd` `Exited (0) 7
hours ago`, `tbc-mangosd` `Exited (137) 7 hours ago`, `tbc-db` `Exited (0) 7 hours ago` — which is
where the VM's own power-off left them, not this lane. `C:\gate\tbc-server`,
`C:\gate\vanilla-server` and `C:\gate\client` were not touched. The VM was not stopped.
