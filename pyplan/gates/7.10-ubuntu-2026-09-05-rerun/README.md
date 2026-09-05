# 7.10 re-run on the MERGED engine, 2026-09-05, `yulon-ubuntu`

Answering `pyplan/checklist.md:2075` — *"re-run WotLK's 6.5 coverage gate after 7.1–7.9 land to
confirm shared layers (`docker.py`, base `Controller`, `runner.py`, `platform.py`,
`networking.py`) weren't regressed"* — a second time, on the tree that has 7.1–7.9 and the
merge lanes in it.

**Why a second time.** The 2026-09-04 sweep (`pyplan/gates/7.10-ubuntu-2026-09-04/`) and its
widget follow-up (`pyplan/gates/7.10-gaps/`) ran at `81d7311e`. Between that commit and this
one, `git diff --stat 81d7311e cfb4c04f -- pylauncher` is **64 files changed, 23,102
insertions, 773 deletions**, and of the five shared files 7.10 names, four moved:
`networking.py` +3,028, `runner.py` +153, `docker.py` +28, `controller.py` +16 (measured on the
laptop at `cfb4c04f`). Two of the things the earlier runs reported as open are closed by that
delta, and this run is where that stops being a claim.

**Code under test.** `/home/pk/p7/checkout` at
`cfb4c04f367536375abf6382694a1f800c468b8a` — a clone of `pjerra/dads-mmo-lab` made on the box,
checked out at `yulon-phase7`'s tip. Interpreter `/home/pk/p7/venv/bin/python`, **Python
3.12.3**, **PySide6 6.11.2**, `pydantic 2.13.5`, installed from
`pylauncher/requirements-dev.txt` (`run.log` carries the first three lines as the runner read
them). `QT_QPA_PLATFORM=offscreen` throughout.

**Target.** The live 7.2 WotLK install at `/home/pk/wowserver`, `install_id 243c46e3`, all six
stages completed, its three `ac-*` containers up. Nothing was installed, reinstalled or removed.

## Result: 88 checks, 88 OK, 0 FAIL

| Driver | what it drives | checks | exit | elapsed | log |
|---|---|---|---|---|---|
| `sweep_driver.py` | base `Controller` status/import/ports | 3 OK | 0 | 1 s | `sweep1.log` |
| `sweep_driver2.py` | modules, plans, repair refusal, update, console, log stream | 7 OK (one of them vacuous — read on) | **0** | 6 s | `sweep2.log` |
| `sweep_driver3.py` | backup → verify → restore → start | 7 OK | 0 | 60 s | `sweep3.log` |
| `sweep_driver4.py` | `network_apply('lan')`, realmlist file, interrupted-restore | 3 OK | 0 | 2 s | `sweep4.log` |
| `widget_driver.py` | six real buttons on `ControllerView` + `CatalogView`'s Install | 32 OK | 0 | 11 s | `widget-run.log` |
| `widget_cancel_driver_tbc.py` | a REAL cancelled install, clicked and stopped | 20 OK | 0 | 36 s | `widget-cancel-tbc.log` |
| `copy_shapes_driver.py` | the cancel copy over the `wow-wotlk` cancelled-clone folder shape | 16 OK | 0 | 101 s | `copy-shapes.log` |

`run.log` and `cancel-run.log` carry the same exit codes and elapsed times as the runners wrote
them.

**One of the 88 asserts nothing, and it is named rather than hidden.** `sweep2.log:32-33` prints
`[OK] send_console('server info') -> ConsoleReply(command='server info', lines=(), prompted=False)`
— an empty reply, passed because the 2026-08-28 drivers count any return that does not raise.
`worldserver-boot-2210.txt` says why the reply was empty (the sweep asked 44 s after a restart,
and the world finished initializing 10.8 s after the call), and the console claim of this run
rests on `widget-run.log:20-28` instead, four minutes later, where the same call comes back with
the core revision, uptime and the update-time diff. Read the console line of the headline as 87
+ 1; the trap section at the end of this file is about exactly this shape.

## The two things the 2026-09-04 run left open, both closed here

### 1. `sweep_driver2.py` no longer aborts at interpreter shutdown

The 09-04 sweep's one finding was that `sweep_driver2.py` **exited 134 (SIGABRT)** after all
seven of its checks passed: it takes five lines off `logs_source()` and `break`s without closing
the generator, so `stream()`'s `finally` ran at interpreter shutdown when the daemon
`_drain_stderr` thread could no longer be joined. That run's README established the two
functions in the traceback were byte-identical to the 2026-08-28 code, so it was a latent hazard
rather than a Phase 7 regression.

**On `cfb4c04f` the same driver, unchanged apart from its `sys.path` line, exits 0**
(`run.log`, `sweep2.log:` the driver's own `[OK] logs_source() yielded 5 live line(s)` and the
`--- exit code: 0` line the runner appended). What changed is nameable: `runner.py::stream()` is
**not** byte-identical between `81d7311e` and `cfb4c04f` (99 → 62 lines by the extraction in
`git show <sha>:pylauncher/yulon/runner.py`), and the commit that changed it is `d2b963d5`
*"Six bug-checklist defects closed, each with the RED that proved it"*. It registers every
generator in `_LIVE_STREAMS` and closes what is left from `_close_abandoned_streams()` at
`atexit` (`pylauncher/yulon/runner.py:56`, `:79`, `:152` at `cfb4c04f`). `docker.py::follow_logs()`
is still byte-identical across the two commits — so the abort went away because the shared layer
was fixed, not because the driver was.

**That "because" is a mutation, not a correlation** (`mutation-atexit-close.txt`). On
`yulon-fedora` on 2026-09-05 at 21:48 UTC, in a throwaway `git clone --shared` checked out at
`cfb4c04f367536375abf6382694a1f800c468b8a` with `__pycache__` purged on both sides of every
transition, a probe that takes five lines off `runner.stream()` and `break`s without closing the
generator was run three times on Python 3.13.15:

* as shipped — `took 5 lines; abandoning the generator without close()`, **exit 0**;
* with `runner.py:152` mutated from `atexit.register(_close_abandoned_streams)` to
  `atexit.register(lambda: None)` (one line, `runner.py` sha256
  `eadc94d6…` → `4b21af1d…`) — `Fatal Python error: _enter_buffered_busy: could not acquire lock
  for <_io.BufferedReader name=5> at interpreter shutdown, possibly due to daemon threads` …
  `File ".../yulon/runner.py", line 375 in _stream_lines`, `Aborted (core dumped)`, **exit 134**;
* restored (sha256 back to `eadc94d6…`, worktree clean) — **exit 0**.

That is the 2026-09-04 crash, reproduced on demand and removed again by that one registration.
The throwaway clone and the probe were deleted; `~/dads-mmo-lab` on the box was left clean at
`03262b10`.

### 2. The LAN plan no longer carries `ufw --force enable`

The 09-04 sweep corroborated the 7.1 lockout: `network_plan('lan')` contained
`('ufw', '--force', 'enable')` while `warnings=()` and `manual_steps=()`, and the report came
back `skipped=()`. That box only survived it because port 22 had been allowed by hand after the
7.1 lockout. **This box's `ufw` is inactive**, so the plan was READ before any driver was
allowed near it — `ufw_plan_probe.py`, run twice, once in the ssh session and once under
`systemd-run --user` where `SSH_CONNECTION` is absent
(`ufw-plan-probe-interactive.txt`, `ufw-plan-probe-systemd-run.txt`). Both print:

```
firewall_commands:
  [0] ('ufw', 'allow', '3724/tcp')
  [1] ('ufw', 'allow', '8085/tcp')
enable at index  : None
VERDICT: no enable in the plan -- nothing to lock this box out
```

and a `warnings` entry that says so in the user's words, citing bug-checklist §39. The two
probes disagree on `detect_ssh_route()` — `connected=True, ports=(22,)` in the session,
`connected=False, ports=()` under `systemd-run` — and produce the same plan, which is the point:
the enable is gone unconditionally rather than by a route-dependent judgement.

`sweep_driver4.py` then applied it for real: `done=('ufw allow 3724/tcp', 'ufw allow 8085/tcp',
'realmlist → 172.30.55.119')`, `skipped=(the §39 warning,)`, and **`ufw status` afterwards is
still `inactive`** (`sweep4.log`). `ufw allow` writes into `/etc/ufw/user.rules` even on an
inactive firewall, so the two rules files were copied aside before the driver and put back after
it; `sweep4.log` carries the diff of what the driver added and the sha256 of both files before
and after, equal. The same warning reaches the user through the widget:
`widget-run.log`'s Networking-tab plan renders it in full.

## What each shared layer answered

* **base `Controller`** — `status()` `db=True auth=True world=True`; `import_state()`
  `populated`, *"102 rows in acore_auth.account, 1000 rows in acore_characters.characters"*;
  `port_conflicts()` `[]`; `repair_import()` **refused** on a populated DB naming the two running
  containers; `stop()` driven three times and `start()` four
  times around the install-half attempts (`containers-stop.txt`, `containers-stop-2.txt`,
  `containers-stop-3.txt`, and `containers-start.txt`, which holds the last of the four starts) —
  `start()` brought `db/auth/world` back to `True` in 2 s from stopped containers each time.
* **`docker.py`** — `follow_logs()` yielded 5 live worldserver lines; the console round-trip
  `send_console('server info')` came back through the widget with the core revision, server
  uptime and `Update time diff: 2ms. Mean 33ms, Median 4ms, p95/p99/max 122/137/390ms`
  (`widget-run.log`); `port_conflicts()` `[]` with the containers stopped.
* **`runner.py`** — every subprocess above, plus the log stream, plus the abort that no longer
  happens.
* **`platform.py`** — `check_for_update()` `current=0.6.59 latest=v0.6.59Public available=False
  error=None`; `keep_awake()` **taken** around the cancelled install — its release is cited, not
  shown by this run (see below).
* **`networking.py`** — `network_plan('lan')` and `('internet')` both `ready=True`;
  `network_apply('lan')` 3 done; `write_client_realmlist()` round-tripped
  `192.168.1.50` into `Data/enUS/realmlist.wtf`.
* **maintenance / backup-restore** — `backup()` of the live server produced 4 dumps
  (`acore_auth` 80,355 B, `acore_characters` 10,659,800 B, `acore_playerbots` 62,378,278 B,
  `acore_world` 306,398,766 B) while it was running; `verify_dump()` passed on all four;
  `plan_restore()` refused while auth+world were up and allowed it once they were down;
  `restore()` completed with a pre-restore safety backup; `interrupted_restore()` `None`.

## The install half

### A real cancelled install, driven by clicks, on the merged engine

`widget_cancel_driver_tbc.py` built the real `CatalogView` over a real `LogPanel` with the real
`installer_for_app` factory `main.py` passes, clicked the TBC tile's `Install`
(`QTest.mouseClick`), waited for the engine's own `Cloning cmangos/mangos-tbc into
src/mangos-tbc` line (2.4 s after the click), clicked the panel's `Stop` 20 s later, and
**12.2 s afterwards** `install_finished(ok=False)` arrived with a modal titled *Install
cancelled* whose text **is** `cancelled_install_message(entry, folder)` for the folder as it then
stood — compared as strings, both for the signal's message and for the modal's own text. 20 OK /
0 FAIL, the tile still reads `Install`. `widget-cancel-tbc.log`.

The folder it left (`cancel-folder-after-tbc.txt`): `.yulon-install.json` present with
`"install_id": "6bc04447"` and `"completed": []`, `src/mangos-tbc/`, 337 MB, no `.git` and no
compose file at the root. Nothing was left running: `docker ps` empty, no `buildx_buildkit`
container, `docker images` unchanged from the six listed before the click.

Yu'lon's `keep_awake()` was **taken** at `22:45:40` (`widget-cancel-tbc.log:22`, the
`systemd-inhibit --what=idle:sleep --who=Yu'lon` line). **This run does not show it released,
and the file that looks as if it does cannot.** `run-710-cancel3.sh:83` captured
`systemd-inhibit --list 2>&1 | tail -5`, so `cancel-folder-after-tbc.txt:53-57` holds three rows
of a list whose own last line reads `8 inhibitors listed.` — the three are GNOME's
(`gsd-media-keys` twice, `gsd-power`); the other five were never captured, and a `--who=Yu'lon`
inhibitor still held would not have to appear among the last five rows. That probe answers the
same whether the inhibitor was released or not, so nothing is claimed from it here. The release
is **cited**, not re-earned: `7.10-gaps/README.md:22` → `7.1-ubuntu-2026-09-04/kill-record.txt:41`
(*"NONE: no install, no builder, no inhibitor left behind"*).

A note on `widget-cancel-tbc.log:14`, the run's first check: its label reads *"the WotLK tile's
Install"*. That wording is inherited from the driver this one was forked from
(`drivers/widget_cancel_driver.py:210`, the `wow-wotlk` driver) and was not updated in
`drivers/widget_cancel_driver_tbc.py:243`. The tile actually clicked is named four lines later,
at `widget-cancel-tbc.log:18`: `started=['wow-tbc']`. The drivers are committed exactly as run
and `drivers.diff` is their complete diff, so the label is corrected here rather than in the file.

### Why it is `wow-tbc` and not `wow-wotlk`, and what that costs

The lane's plan was to stop the live install's containers through the app to free the ports and
cancel a `wow-wotlk` install. **Stopping is not enough, and that is a measured refusal rather
than a guess.** With all three `ac-*` containers stopped through `Controller.stop()`
(`containers-stop.txt`), preflight **refused nothing** — its panel dump is six `[pass]` and two
`[warn]` (compiler jobs vs memory; free space, 51 GB against the comfortable 75 GB — preflight's
"GB" is GiB, `preflight.py:53` `GIB = 1024**3` and `:659` `gigabytes = free / GIB`) with no
`[refuse]` line at all, and the check the stop was for is one of the passes: *"[pass] the
server's ports: nothing else is using them"* (`widget-cancel-wotlk-refused.log:35`; the two warns
are `:30-31`, and because the panel is dumped twice `grep -c 'warn]'` on the file is 4). The
engine refused after preflight, on a different guard:

> A container called ac-database already exists and belongs to another install
> (yulon-wow-wotlk-243c46e3). Two servers cannot share that name. Remove the other install's
> containers from its own tab first, then try again.

(`widget-cancel-wotlk-refused.log`, a modal titled *Install failed*, clicked away by the driver.)
The app's own remedy is to remove that install's containers; this lane may not, so `wow-wotlk`
has no second install on this box while the 7.2 one is here.

A second refusal is on record from the first `wow-tbc` attempt
(`widget-cancel-tbc-refused-client.log`): `wow-tbc` has `requires_client_dir: true`, so
`CatalogView._start_install()` calls the folder picker twice, and a driver that answers the same
folder both times is refused with *"has no Data directory, so it is not a game client"*. The run
above tells the two prompts apart by the words the user reads and answers the second with a
throwaway stand-in holding `Data/expansion.MPQ` — the `required_file` `catalog.json` names for
this game. Two warning-level rules fired on it (1 MPQ where 6 is the floor; no locale folder) and
neither refuses. Nothing read it: the cancel lands in `clone-sources`, stages before
`client-data`, and `cancel-folder-after-tbc.txt` lists the stand-in unchanged, three entries.

**What that costs:** `wow-tbc` clones into `src/`, so the two findings the 2026-09-05 `wow-wotlk`
cancel filed are shapes this run cannot reach through a widget. They are answered next, and the
route is named rather than blurred.

### The two 2026-09-05 findings, on the folder shape that produced them

`copy_shapes_driver.py` rebuilds that folder — `git clone --depth 1` of
`mod-playerbots/azerothcore-wotlk`, which is what `clone-core` leaves — and asserts it field by
field against `pyplan/gates/7.2-ubuntu-2026-09-05/widget-cancel-folder-after.txt`: no
`.yulon-install.json`, a `.git` at the root, `docker-compose.yml` that `git ls-files` names and
`git status --porcelain` reports clean, `generated_compose_files()` empty, `compose_file()`
answering. The 2026-09-05 modal is **read out of the committed
`7.2-ubuntu-2026-09-05/widget-cancel.log`** rather than retyped, and checked against the
transcription in the driver. Then `cancelled_install_message()` is rendered for that folder on
`cfb4c04f`. 16 OK / 0 FAIL (`copy-shapes.log`):

| | 2026-09-05 (`widget-cancel.log:44`) | `cfb4c04f` (`copy-shapes.log`) |
|---|---|---|
| finding 1 — upstream's own compose file | *"The source is there. If the build had already finished … press "Use existing…" … **nothing is lost**"* | *"There is a compose file in … that this app did not write (docker-compose.yml). **If it was already there before this attempt** … **If this attempt was downloading into an empty folder**, that file came down with the server's source and there is no server behind it."* — both readings, no "nothing is lost" |
| finding 2 — the resume over a folder with no record | *"press Install again and choose … the installer carries on from the last stage recorded in …/.yulon-install.json"* | *"**Do not press Install again on this folder — the app will refuse it.** There is no .yulon-install.json …, **it holds a git checkout**: … so it stops rather than run `git fetch` and `git reset --hard` over your work."* |

The refusal the new copy names is the one the engine actually raised on that folder on
2026-09-05 (`7.2-ubuntu-2026-09-05/cycle2-pressA2-refused-existing-checkout.log:31`), not the
not-empty one.

**Why rendering the function is enough here, said explicitly.** The widget half is proved
separately and on the same engine: the `wow-tbc` run compares the modal's own text with
`cancelled_install_message(entry, folder)` as strings, so what that function answers is what a
user reads. And the call site is one line, unchanged in substance since the 2026-09-05 run:
`git diff 4c959d70 cfb4c04f -- pylauncher/yulon/ui/catalog_view.py` is a **single hunk**,
`cancelled_install_message(self._catalog.get(game_id).name, server_dir)` →
`cancelled_install_message(self._catalog.get(game_id), server_dir)`, inside the same
`QMessageBox.information(self, "Install cancelled", note)` and the same
`install_finished.emit(game_id, False, note)`. `4c959d70` is the SHA
`7.2-ubuntu-2026-09-05/README.md:13` records for that run.

**Still not exercised, and reported as such:** that copy arriving as a modal from a cancelled
`wow-wotlk` install on the merged engine. It needs a box with no AzerothCore containers on it.

### The other half of the `claimed` gate, measured through the widgets

`cancelled_install_message()`'s fix rests on an asymmetry: a CMaNGOS cancel keeps this attempt's
record while a `wow-wotlk` one does not, because `_clone_core()` empties the destination it
clones into and only `wow-wotlk` spells that destination `"."`. That was measured on m910q on
2026-09-05 through `CmangosInstaller.run()`. Here it is measured through the widgets on a
different box: `.yulon-install.json` **is** on disk after the cancel, and the copy therefore
does promise the resume — and does **not** also warn of a refusal, which is the pair that used to
appear together. The record's `"completed": []` is worth noting: "carries on from the last stage
recorded" would here mean starting `clone-sources` again — which is exactly what the 2026-09-05
`wow-wotlk` folder was promised and then refused.

**That the engine would accept this folder on a second press is a reading of the code, not a
measurement of this run.** At `cfb4c04f`, `_claim_folder()` in
`pylauncher/yulon/catalog/native.py:1812-1910` refuses a non-empty folder only when it holds no
record of its own (`:1895` `if existing is None and server_dir.is_dir() and not (server_dir /
".git").is_dir():` → `:1904` *"is not empty and was not created by this app"*), and its three
record-mismatch refusals (`:1875` a different `install_id`, `:1881` a different game,
`:1886` a different family) all compare against a record this same install of this same game
wrote into this same folder. `cancelled_install_message()`'s two "the app will refuse it"
branches (`pylauncher/yulon/catalog/installer.py:607` and `:611`) are likewise the
no-`.yulon-install.json` case. Install was pressed on the path
`/home/pk/p7-cancel-install-tbc` **twice** in this run, but never twice on the same folder
contents: at `22:32:00` (attempt 2, the first `wow-tbc` press,
`widget-cancel-tbc-refused-client.log:17`), where preflight
refused the press because the driver had passed that same path as the *client* folder and it has
no `Data` directory (`native.py:1787-1790`), and at `22:45:38` (attempt 3, the cancel,
`widget-cancel-tbc.log:18`, with `/home/pk/p7-fake-tbc-client` as the client folder). The path
was emptied between them — `run-710-cancel2.sh:103` at the end of attempt 2 and again at
`run-710-cancel3.sh:37` before attempt 3 — so the folder that *survived the cancel* was never
pressed again: its modal was clicked away and it was removed at cleanup (`final-state.txt`). No
second press on a cancelled folder was driven here and none is claimed.

## What is CITED from earlier runs, not re-run

| 6.5 install-half item | Cited from | Why not re-run |
|---|---|---|
| preflight floors **refusing**, not warning, on free space | `7.1-ubuntu-2026-09-04/press1.log:15-26`, and `7.10-gaps/README.md`'s widget-driven refusal | this box had **54.5 GB** free at the Install click (`widget-run.log:69`, `22:15:15`; the driver prints decimal GB, `drivers/widget_driver.py:309` `f_bavail * f_frsize / 1e9`) = **50.8 GiB**, and preflight's own reading nine minutes later was `51 GB` (`widget-cancel-wotlk-refused.log:31`, `22:24:38`) — the same scale, because preflight's "GB" is GiB (`preflight.py:53`, `:659`) and so is its 48 GB refusal floor (`7.1-ubuntu-2026-09-04/press1.log:21` *"the install needs 48 GB"*). The margin over the floor is therefore **2.8 GiB**, not the 6.5 the decimal figure reads as — still above it, so the space refusal is not reachable here. The refusal this box IS under — the port conflict — was driven through the widget instead and is asserted in its place; the driver's diff records the swap and why |
| staged / resumable install | `7.1-ubuntu-2026-09-04/press2.log`, `press3.log`, `kill-record.txt`, `ccache-stats.txt` | a resume needs a build, and a build compiles for hours |
| `keep_awake()` **released** | `7.10-gaps/README.md:22` → `7.1-ubuntu-2026-09-04/kill-record.txt:41` | taken again here (`widget-cancel-tbc.log:22`), but NOT re-earned: the after-probe is `systemd-inhibit --list \| tail -5` (`run-710-cancel3.sh:83`) of an 8-row list, so `cancel-folder-after-tbc.txt:53-57` would print the same three GNOME rows whether Yu'lon's inhibitor was still held or not |

## The server is intact afterwards

`state-before.txt` / `state-after-restore.txt` / `state-after.txt` / `final-state.txt`. All four
report:

* schema table counts **22 / 111 / 315 / 30** (auth / characters / world / playerbots) — the
  figure every platform's install gate has produced;
* realmlist `1 AzerothCore 172.30.55.119 172.30.55.119 8085`, unchanged, and the widget's plan
  names that same address because the driver now reads it out of the DB rather than comparing
  against a literal;
* `/home/pk/wowserver/.yulon-install.json` byte-for-byte the same six completed stages and
  `install_id 243c46e3`;
* 3724 / 8085 listening, `ac-database` healthy, all three containers up.

`final-state.txt` is the cleanup:

* `acore_auth.account` **103 → 102**: `WIDGET0905R` (id 103), which the widget driver created
  through the Accounts tab, deleted with its `account_access` row; `101 YULON` and
  `102 GATELOGIN` untouched, `account_access` back to 2 rows;
* the five dumps `sweep_driver3.py` wrote removed (this run created the whole
  `sql_scripts/backups` directory; the now-empty directory is left, since the app makes it);
* all seven throwaway folders listed and then gone;
* `ufw` **inactive** with `/etc/ufw/user.rules` and `user6.rules` at the same sha256 as
  `state-before.txt` recorded, back to `root:root 640`;
* `docker images` the same six, `docker system df` the same 20 images / 12.74 GB build cache —
  nothing pruned;
* no `dml-710*` user unit and no `lane710*` system unit left;
* `/home/pk/p7/` **kept on purpose** (checkout, venv, drivers, logs) — lane b39 reuses it.

## The trap this run fired, kept because it cost a whole sweep

**`systemd-run --user` does not carry the `docker` group.** The first launch of
`run-710-rerun.sh` ran the drivers directly under `systemd-run --user --collect`, and every
`docker` call inside them came back *"permission denied while trying to connect to the docker API
at unix:///var/run/docker.sock"* — while the same commands work in the ssh session, where
`id -Gn` lists `docker`. The user manager was started before `pk` joined that group and has never
been re-execed. `aborted-nodockergroup/` is that run.

Two things make it worth keeping rather than deleting. First, `sweep_driver3.py` got as far as
`sudo docker stop ac-authserver ac-worldserver` — which worked, because it is the one call in the
sweep that uses `sudo` — and then could not start them again, so the box was left with auth and
world down until `Controller.start()` put them back. Second, and worse: **the drivers reported
`[OK]` for calls that had failed.** `aborted-nodockergroup/sweep1.log:44` is
`[OK] import_state() -> ImportState(state='unreadable', detail='could not list the databases:
permission denied …')` and `sweep2.log:41` is `[OK] send_console('server info') ->
ConsoleReply(command='server info', lines=('permission denied …',))`. The 2026-08-28 drivers
count a call as passing when it returns without raising, whatever it returns.

**The re-run is not immune to that shape, and one of its 88 `[OK]`s is an instance of it.**
`sweep2.log:32-33` is `[OK] send_console('server info') -> ConsoleReply(command='server info',
lines=(), prompted=False)` — no lines at all, passed anyway. Not a broken console:
`worldserver-boot-2210.txt` holds the worldserver's own stamps for that window (captured
read-only from `docker logs -t ac-worldserver` on 2026-09-05 at 23:48 CEST). The aborted run's
`sudo docker stop` halted the world at `22:08:12` (`aborted-nodockergroup/sweep3.log:79`); it
was started again at `22:10:25` — by hand, with `stopstart.py start` (`Controller.start()`),
the action the paragraph above records, though no log of that call survives in this folder
(`containers-start.txt` was overwritten by the install half's own `22:47:56` start), so the
docker stamp is the only trace of it; `sweep_driver2.py` started at `22:11:06`, which
`sweep2.log:2` does log, and sent `server info` at `22:11:09`,
44 s after that start; `WORLD: World Initialized In 0 Minutes 53 Seconds` landed at
`22:11:19.83` — 10.8 s after the call, and 7.8 s after the driver had already exited at
`22:11:12`. `prompted=False` says the console had no prompt yet, and the world then consumed the
queued command at `22:11:19.87`. So the check was made against a server that was still booting,
and the driver's "no exception is a pass" shape reported it green. The console round-trip on this
engine is carried by `widget-run.log:20-28` instead. The re-run was relaunched under
`sg docker -c`, which is what the 2026-09-05 widget-cancel run used for the same reason.

## Files

| File | What it is |
|---|---|
| `run-710-rerun.sh` | the sweep runner, as it executed under `systemd-run --user --collect --unit=dml-710-rerun` via `sg docker -c` |
| `run-710-cancel.sh`, `run-710-cancel2.sh`, `run-710-cancel3.sh` | the three install-half attempts, in order |
| `cleanup-710.sh` | the cleanup, which wrote `final-state.txt` |
| `stopstart.py` | stops/starts the live install through `Controller.stop()` / `.start()` |
| `ufw_plan_probe.py` | the read-only firewall-plan probe run before any driver applied anything |
| `drivers/` | every driver exactly as run |
| `drivers.diff` | their complete diff against the committed originals |
| `run.log`, `cancel-run.log` | provenance header + per-driver exit code and elapsed time |
| `sweep1.log` … `sweep4.log`, `widget-run.log` | the sweep half's transcripts |
| `mutation-atexit-close.txt` | the mutation that turns §1's "because" from a correlation into a cause: `atexit.register(_close_abandoned_streams)` neutered at `cfb4c04f` on `yulon-fedora`, exit 0 → 134 → 0 |
| `worldserver-boot-2210.txt` | `ac-worldserver`'s own boot stamps for the window `sweep_driver2.py` ran in — why `sweep2.log:32`'s console reply was empty |
| `widget-cancel-wotlk-refused.log` | the container-name refusal, through the widget |
| `widget-cancel-tbc-refused-client.log` | the client-folder refusal, through the widget |
| `widget-cancel-tbc.log`, `cancel-folder-after-tbc.txt` | the real cancelled install and what it left |
| `copy-shapes.log` | the two findings on the `wow-wotlk` folder shape (`copy-shapes-before-tbc-cancel.log` is the same driver run before the TBC cancel existed to contrast with) |
| `containers-stop*.txt`, `containers-start.txt` | the live install stopped (3x) and started (4x) through the app; the start file was rewritten by each attempt and holds the last |
| `state-before.txt`, `state-after-restore.txt`, `state-after.txt`, `final-state.txt` | the same probe, four times |
| `ufw-plan-probe-*.txt` | the firewall plan read in two environments before it was applied |
| `aborted-nodockergroup/` | the first, wrong launch — kept for the trap above |
