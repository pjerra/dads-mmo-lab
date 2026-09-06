# bug-checklist §41 — a realm can be set to loopback on purpose

**Two runs, and the second one closes the gate.** Round 1 (2026-09-05, m910q) is everything
from here down to "The box, put back"; it left one clause of §41's gate unmet — the second
Install press, refused there by preflight for disk. Round 2 (2026-09-06, yulon-ubuntu, commit
`96251d57`) is the last section of this file, and it is where the press actually ran, twice.
Read round 2 for the gate's verdict, round 1 for the widget half, the RED-before-the-code
evidence and the defect this work found.

---

## Round 1 — m910q, 2026-09-05 (the press clause not met)

Driven on **m910q** on **2026-09-05** against the finished CMaNGOS Vanilla install at
`/home/pk/vanilla-75b` (`.yulon-install.json`: `game_id wow-vanilla`, `family cmangos`,
`install_id 06ced116`, eight stages completed; containers `vanilla-db`, `vanilla-realmd`,
`vanilla-mangosd` up), at commit **`d1e41fbc`** of `lane/b41`.

Everything below is a file in this directory. Nothing here is a summary of something that
was not kept.

## What the entry asked for, and what each half is

§41's gate, in the entry's own words:

> choose loopback through the app, press Install again on the finished install, and the row
> is still `127.0.0.1` with a log line saying why it was left alone — while a server whose
> loopback was never chosen still ends up advertising a reachable address.

| clause | met | file |
| --- | --- | --- |
| choose loopback **through the app** | yes, through the real widgets | `widget-driver-output.txt` |
| the row is `127.0.0.1` after that choice | yes, read back through `docker exec` | `widget-driver-output.txt` |
| **press Install again** on the finished install | **no — refused by preflight, for disk** | `press-refused-by-preflight.txt` |
| the row is still `127.0.0.1`, with a line saying why | yes, from the engine's closing step | `closing-step-output.txt` |
| a never-chosen server still advertises a reachable address | yes, same run, same install | `closing-step-output.txt` |

## Half one: the choice, made through the real Networking tab

`widget_driver_b41.py` builds the real `ControllerServices.for_entry()` wiring and the real
`ControllerView` on an offscreen `QApplication`, and presses the tab's real widgets with
`QTest.mouseClick` — the new radio, `Show plan`, `Apply`. It calls neither `networking.plan()`
nor `networking.apply()` itself. **16 passed, 0 failed** (`widget-driver-output.txt`).

What the widget itself showed, verbatim from that file:

    loopback radio label = 'Only this computer (127.0.0.1)'
    | Mode: loopback   LAN IP: 192.168.10.134   public IP: -
    | Players set realmlist to: 127.0.0.1
    | Realmlist: UPDATE realmd.realmlist SET address='127.0.0.1' WHERE id=1;
    |   ⚠ this realm will advertise 127.0.0.1, so no other machine can reach this server: …
    | Applied:
    |   ✓ realmlist → 127.0.0.1

and the row, read back by a route the widget never touches
(`docker exec -e MYSQL_PWD=… vanilla-db mariadb -uroot -N -B -e "SELECT address FROM
realmd.realmlist WHERE id=1;"`): `'192.168.10.134'` before the press, `'127.0.0.1'` after it.
The file it wrote:

    /home/pk/vanilla-75b/.yulon-network.json
    { "mode": "loopback", "recorded_unix": 1788639538 }

## Half two: what the closing realm step then did — and what it is not

**The second Install press did not run, and the reason has nothing to do with §41.** It was
attempted first, as `python -m yulon.install_wiring wow-vanilla --server-dir /home/pk/vanilla-75b
--client-dir /home/pk/clients/WoW-Client-1.12.1`, and the engine's own preflight refused it at
2026-09-05 22:20:32 box-local (`press-refused-by-preflight.txt`):

    [refuse] free space on Docker's disk and the server folder: 18 GB free, and the install
    needs 40 GB (the server folder and Docker's disk share one drive, so both needs add up)

`df -h /` on m910q that minute: `117G` total, `93G` used, `19G` available. Reaching 40 GB
would have meant deleting other lanes' and the owner's material on a shared box — 29 GB of
game clients, a 13.6 GB Docker build cache another lane may be measuring — so it was not done.
`yulon-ubuntu`, the other box with a finished install and 51 GB free, was in use by lane 7.10
at the same minute (its `claude-activity.log`, 22:15). **So this clause of §41's gate is not
met, and the box that can meet it has to be one with room.**

What ran instead is `closing_step_driver_b41.py`: the real `CmangosInstaller` built by the same
`install_wiring.installer_for_app()` the CLI builds, handed the real `InstallState` read off the
real state file and the real `Secrets` resolved from the real `.db_password`, with its closing
realm step called against the live `vanilla-db`. That is the same method, the same seams and the
same database a press would reach — one call further in, past the preflight that refused. It is
weaker evidence than a press in exactly one way: it does not show the eight stages printing
"already finished" ahead of it. **12 passed, 0 failed** (`closing-step-output.txt`).

The step is on the shared spine, so both families run it — printed by the driver rather than
asserted from a reading of the source:

    engine class = CmangosInstaller   family = cmangos
    _advertise_realm is defined on StagedInstaller, which is the shared spine both families
    inherit

**Part A — the loopback the owner chose is left alone.** Row `'127.0.0.1'` going in, row
`'127.0.0.1'` coming out, and one line said:

> The address this realm advertises was left exactly as it is, because this server was set to
> only this computer (127.0.0.1) on 2026-09-05 from its Networking tab, and that choice is
> recorded in .yulon-network.json in the server folder. Nothing here overwrote it, which means
> no other machine can reach this server. To undo it, open this server's Networking tab, pick
> LAN (same Wi-Fi) or Internet play, press Show plan and then Apply.

**Part B — a server whose loopback was never chosen.** `.yulon-network.json` removed, the same
step run again on the same install, same row: `'127.0.0.1'` going in, `'192.168.10.134'` coming
out — `platform.detect_lan_ip()` on that box answered `'192.168.10.134'` in the same run — and
the line named it.

## What the tests said before the code existed

`red-before-the-code-networking.txt` and `red-before-the-code-spine-and-view.txt` are the runs
taken on m910q before any of §41's implementation was written: 9 failed / 3 passed and 4 failed,
each on the attribute or widget that did not exist yet (`LOOPBACK_ADDRESS`, `INTENT_FILE`,
`read_network_intent`, `record_network_intent`, `ControllerView.loopback_radio`).

`checks-green.txt` is `run-tests-vm.sh --checks` on m910q at `d1e41fbc`: **2802 passed, 4
skipped**, mypy clean on this platform, as Windows and as macOS, ruff clean, black clean.

## One defect this work found, and fixed

Running the new spine tests turned up a folder this app had written every byte of being refused
by the app's own guard:

    InstallerError: …/wow is not empty and was not created by this app (.yulon-network.json).
    Nothing was written.

`_claim_folder()` asked "is this folder somebody else's?" with one file's NAME
(`ignoring=STATE_FILE`) rather than with the set of files this app writes. The set now has a
name, `native.OUR_OWN_FILES`, used at all five call sites, and `_listing(ignoring=)` raises
`TypeError` on a bare `str` — which mypy accepts as a `Collection[str]` and which would have
filtered the listing by character.

## The box, put back

`box-readback-after.txt` and `ufw-put-back.txt`, both taken after everything above:

* realm row `1  MaNGOS  192.168.10.134  8085` — the value it held before this lane touched it;
* no `.yulon-network.json` in `/home/pk/vanilla-75b`;
* `.yulon-install.json` unchanged (`updated_unix 1788516894`, the same eight stages);
* `vanilla-db`, `vanilla-realmd`, `vanilla-mangosd` all up, 34 hours, db healthy;
* the two `ufw allow` rules the Apply press added deleted; `ufw show added` is `(None)` and
  `ufw status` is `inactive`, which is how the box was found.

---

## Round 2 — yulon-ubuntu, 2026-09-06: the press, both halves

Driven on **yulon-ubuntu** on **2026-09-06** against the finished AzerothCore WotLK install at
`/home/pk/wowserver` (`.yulon-install.json` read at 04:43:21 box-local: `game_id wow-wotlk`,
`family azerothcore`, `install_id 243c46e3`, six recorded stages completed; containers
`ac-database`, `ac-authserver`, `ac-worldserver` up; `/` had 50 GB free), from a checkout of
`lane/b41` at **`b206ad0c`** for the run itself and **`96251d57`** for the script fix that got
its second half started. Every file named below is in `yulon-ubuntu-press/` beside this README.

### Why not m910q

Two reasons, and only the first was measured by this lane:

1. **Preflight refused the press there for disk** — round 1 above, `press-refused-by-preflight.txt`.
2. **The CMaNGOS engine now refuses a press on a folder built before the doodad patch.** The
   round-2 lane brief records lane b43 measuring this on 2026-09-05, after `lane/doodad` merged
   at `5a57164d`; m910q's Vanilla install was built 2026-09-04. In this tree the refusal is
   `CmangosInstaller._refuse_to_patch_what_will_not_be_rebuilt()`
   (`pylauncher/yulon/catalog/families/cmangos.py:360`, called from `_patch_sources()` at line
   318, before `db-password` and `write-dockerfile`). **Read, not driven here** — this lane ran
   no CMaNGOS press in round 2.

yulon-ubuntu's install is AzerothCore, whose stage list has no `patch-sources` at all
(`clone-core, clone-modules, generate-compose, build, client-data, start-db, import, up, ready`),
and it had the room.

### The gate, clause by clause

| clause | met | file |
| --- | --- | --- |
| choose loopback **through the app** | yes, `QTest.mouseClick` on the real radio/buttons | `widget-loopback.log` |
| the row is `127.0.0.1` after that choice | yes, `127.0.0.1 / 127.0.0.1`, read through `docker exec` | `widget-loopback.log` |
| **press Install again** on the finished install | **yes — 880 s, exit 0, nothing compiled** | `press-chosen.log` |
| the row is still `127.0.0.1` after the press | yes | `after-press-chosen.txt` |
| a log line saying why it was left alone | yes, in stdout **and** in `yulon.log` | `press-chosen.log`, `yulon-log-press-chosen.txt` |
| a never-chosen server still advertises a reachable address | yes, second press, 10 s, exit 0 | `press-never-chosen.log` |

### Half one — the choice, through the real Networking tab

`widget_driver_b41_wotlk.py`, offscreen, on the real `ControllerServices.for_entry()` wiring:
**17 passed, 0 failed**. The row read through `docker exec -e MYSQL_PWD=password ac-database
mysql -uroot -N -B -e "SELECT address, localAddress FROM acore_auth.realmlist WHERE id=1;"` —
a route the widget never touches — was `172.30.55.119 / 172.30.55.119` before the Apply and
`127.0.0.1 / 127.0.0.1` after it, and the file it wrote was

    /home/pk/wowserver/.yulon-network.json
    { "mode": "loopback", "recorded_unix": 1788662602 }

### Half two — press 1: the chosen loopback is left alone

`python -m yulon.install_wiring wow-wotlk --server-dir /home/pk/wowserver`, started 04:43:23,
finished 04:58:03 box-local, **exit 0, 880 s**. It printed the engine's own already-finished
lines for every recorded stage and compiled nothing (`press-chosen.log`):

    Already finished: clone-core, clone-modules, generate-compose, build, client-data, import
    mod-playerbots/azerothcore-wotlk is already cloned in /home/pk/wowserver; leaving it exactly as it is.
    The compose files are already exactly what this install needs.
    The server is already built; skipping the compile.
    ac-client-data-init  | yulon: client data v20.0 already installed
    The databases read as populated: 102 rows in acore_auth.account, 1000 rows in acore_characters.characters
    They are already imported; leaving them alone.

The run script watched that log for a compiler line (`Building CXX`, `make[N]`, a `[ NN%]`
progress line, a BuildKit step header) and would have killed the press; it never fired.

Then, after `ready`:

> The address this realm advertises was left exactly as it is, because this server was set to
> only this computer (127.0.0.1) on 2026-09-06 from its Networking tab, and that choice is
> recorded in .yulon-network.json in the server folder. Nothing here overwrote it, which means
> no other machine can reach this server. To undo it, open this server's Networking tab, pick
> LAN (same Wi-Fi) or Internet play, press Show plan and then Apply.

The row afterwards: `127.0.0.1 / 127.0.0.1` (`after-press-chosen.txt`).

**Where the headless log is.** `install_wiring.main()` calls
`configure(config_dir=platform.config_dir(), …)`, and `platform.config_dir()` on Linux is
`$XDG_DATA_HOME/yulon` or, when that variable is empty, `~/.local/share/yulon`. It was empty in
this run's environment, so the file is **`/home/pk/.local/share/yulon/yulon.log`** — not
`~/.config/Yulon/yulon.log`, which the round-2 brief guessed at. It did not exist before this
press (`before.txt`: `wc: /home/pk/.local/share/yulon/yulon.log: No such file or directory`);
the press created it, and line 61 of it is the sentence above, timestamped `2026-09-06 04:57:59`
(`yulon-log-press-chosen.txt`).

### Half two — press 2: a server whose loopback was never chosen

`.yulon-network.json` removed (`record-removed.txt`), the row left at `127.0.0.1 / 127.0.0.1`,
and the same command pressed again: started 05:00:15, finished 05:00:25, **exit 0, 10 s**. Same
already-finished lines, and then:

> The realm now advertises 172.30.55.119, so players on other machines can reach this server:
> 172.30.55.119 is the address they set in their client's realmlist. The server was already
> running when this was set, so if a client is still sent to the old address, stop and start it
> again on the Server tab.

Row afterwards: `172.30.55.119 / 172.30.55.119` (`after-press-never-chosen.txt`, which also
carries that press's whole `yulon.log` excerpt).

### The way back a user actually has

The entry asks how the choice is undone without deleting a file by hand.
`networking.record_network_intent()` (`pylauncher/yulon/networking.py:3008` at `cdd5eba2`)
stores the LAST APPLIED mode rather than only `loopback`, so the answer is the two buttons the
install log's own sentence names. Driven, not read: `widget_driver_b41_wotlk_undo.py` chose the
loopback again through the tab (record `{"mode": "loopback"}`, row `127.0.0.1`), then clicked
`LAN (same Wi-Fi)` → `Show plan` → `Apply` — **5 passed, 0 failed** (`widget-undo.log`):

    the record no longer says loopback -- NetworkIntent(mode='lan', recorded_unix=1788663628)
    realm row now = '172.30.55.119   172.30.55.119'

So there is a route through the tab and it needs no file handling. Deleting
`.yulon-network.json` also works and is what press 2 above used, but nothing in the UI does that.

### A defect this gate ran into, which is NOT §41's

**Press 1 sat in `ready` for 14 minutes 28 seconds and was only released when `ac-authserver`
was restarted.** The mechanism, in `why-ready-waited.txt`:

* the `ready` stage waits for an auth-container log line matching `install.native.ready.auth`,
  which is `{{REALM_HOST}}:{{WORLD_PORT}}`, filled at `native.py:2741` with
  `INSTALL_REALM_HOST` — the fixed string `"127.0.0.1"` (`native.py:202`; both at `cdd5eba2`) —
  and escaped, so the pattern is `127\.0\.0\.1:8085`;
* `docker.wait_ready()` reads the CURRENT run's log only (`--since` the container's `StartedAt`);
* `ac-authserver` had been running since `2026-09-05T20:48:03Z`, and its line for that run was
  `Added realm "AzerothCore" at 172.30.55.119:8085.` — the row's value at the time it started.
  The Apply had changed the row, but the container had not restarted, so its log could not match;
* `up` runs `compose up -d --no-deps …`, which does not recreate a container that is already
  running, so the press could not fix this itself.

Measured: the press reached `ready` at 04:43:31 and said nothing more until `ac-authserver` was
restarted at 04:57:57 box-local, whereupon its current-run log said `Added realm "AzerothCore" at
127.0.0.1:8085.` and the press printed `The server is up.` at 04:57:59 — two seconds later.

This is not caused by §41 and not fixed by it; `INSTALL_REALM_HOST` and the comment explaining it
(`native.py:1539-1542` at `cdd5eba2`) both predate this lane. **Not measured here:** a press
against a plain LAN-advertising install whose auth container was started under that row — the shape
most finished installs are in. This lane restarted one container and got past it; it did not drive
that case to a verdict, and it did not file a checklist entry for it, because allocating a number
while other lanes are open would collide. It is reported in `r2-fix-b41.json` as a finding for the
workflow to place.

### The box, put back

`put-back.txt`, taken 05:00:28-05:00:29 box-local, against `before.txt` from 04:43:21:

| | before | after |
| --- | --- | --- |
| realm row `id 1` | `AzerothCore 172.30.55.119 172.30.55.119 8085` | the same |
| `.yulon-network.json` | absent | absent (removed) |
| `ufw status` | `inactive` | `inactive` |
| `ufw show added` | `(None)` | `(None)` |
| `ac-database` / `ac-worldserver` | up | up |
| `/` free | 50 GB | 50 GB |

Two things did change and are said rather than glossed:

* **`ac-authserver` was restarted** (`Up 6 hours` → `Up 2 minutes`), deliberately, for the reason
  in the section above. Nothing was removed or rebuilt, and its `RestartCount` read 0 afterwards.
* **`.yulon-install.json` was rewritten by the presses**: `updated_unix` 1788563121 → 1788663620,
  280 bytes both times, and its group went from `docker` to `pk` because the press wrote it as
  user `pk`. The `completed` list is the same six stages. A press rewrites that file; that is the
  engine's normal behaviour and not something this lane could avoid.

### The one thing the run script got wrong

The first launch died after press 1, before it could remove the record and press again: `wc -l`
on a `yulon.log` that did not exist yet wrote its error into the same file the line count was
read back from, and `$(( wc: … + 1 ))` under `set -u` exits the shell. `run.log` shows the run
stopping there. The counter now tests for the file first, and the script takes a `part2`
argument, which is how the rest of the gate was run (`96251d57`). Press 1 was not repeated: it
had already finished green, and its evidence is the files above.

---

## Round 3 — what the record claimed and the machine did not

Two things in the round-2 record were held by sentences rather than by tests, and one of them was
a real side effect on a real firewall. Both were answered with commands; nothing was pressed, no
container was touched, and neither box changed state.

### The detection order was not held by anything

Round 2's entry, the `_advertise_realm()` docstring and the spine test's own docstring all said
the two empty SQL lists in
`test_spine.py::test_a_loopback_the_owner_chose_is_left_alone_and_the_line_says_why` were what
kept the recorded intent read BEFORE the address was detected. They were not. Three mutations were
run on m910q on 2026-09-06 at `30671d6e`, from a fresh `git clone --shared` of the box's reference
clone (`/home/pk/p7-b41-r3`, detached at that commit, venv symlinked as the test runner does),
`__pycache__` purged on both sides of each mutation, the clone removed at the end. Script:
`mutations-round3.sh`; transcript: `mutations-round3.txt`.

| | what it changes | result |
| --- | --- | --- |
| baseline | — | `419 passed in 4.04s` |
| **M5** | `address = self._detected_lan_ip()` moved above the intent read | `1 failed, 418 passed` — `AssertionError: an address was detected before the recorded choice was read` |
| **M6** | the intent read moved below the `address is None` early return | `2 failed, 417 passed` — the M5 failure plus `REALM_ADDRESS_UNKNOWN not in said` |
| **M7** | `wants_firewall = mode != "loopback"` → `wants_firewall = True` | `2 failed, 417 passed` — `test_a_loopback_plan_asks_the_firewall_for_nothing` and `test_the_loopback_plan_in_the_tab_offers_to_open_no_ports` |
| restored | — | `419 passed in 4.10s`, `git status --porcelain` clean but for `?? pylauncher/.venv` |

Under M5 and M6 every assertion in those three files that predates round 3 passed: both empty SQL
lists stayed empty, which is exactly the claim the record made and the reason it was worth nothing.
What holds the order now is a `lan_ip` seam that records each call and must record none, and
`test_spine.py::test_the_machine_this_mode_is_for_has_no_lan_address_at_all` — a server with the
loopback recorded on a machine with no LAN address at all, which is the machine `NetworkPlan.ready`
says the mode exists for. Under M6 that server was told "This machine's address on the local
network could not be worked out …" and never heard about its own recorded choice.

### The loopback plan opened firewall ports

`plan()` built `fw_cmds` from the backend and the ports before it looked at `mode`, so the loopback
plan carried `ufw allow 3724/tcp` and `ufw allow 8085/tcp` under its own warning that no other
machine can reach the server. This is not a wording problem: it happened on yulon-ubuntu on
2026-09-06. `ufw-after-apply.txt` (04:43:23, immediately after the loopback Apply) lists both rules
under `show added`. The widget's own text has the whole shape of it (`widget-loopback.log`):
line 51 `Firewall commands:` with the two `ufw allow` lines under it, line 56 the §39 warning
that begins "Yu'lon opened the game ports in ufw's rule list", line 57 the `ONLY_THIS_COMPUTER`
warning saying no other machine can reach this server, and line 60 `✓ ufw allow 3724/tcp` under
`Applied:`. (Both rules were deleted when the box was put back; `put-back.txt` shows
`ufw show added` → `(None)`.)

`plan()` now computes no firewall commands, no SSH lock-out guard and no "allow inbound TCP … by
hand" manual step when `mode == "loopback"` (`networking.py`, `wants_firewall`). M7 above is the
mutation that restores the old behaviour, and it is red. Each of the two new tests carries a `lan`
control in the same body, so a build that returned an empty command list for every mode fails them
too.

**Not decided, and not changed:** the loopback-binding warning ("ports [...] are published on
127.0.0.1 … other machines cannot reach them") and the WSL/`netsh` portproxy commands in the same
function still behave the same for every mode, loopback included. Nobody has ruled on what they
should do under this mode; this round left them alone rather than guess.

### The third tick, said plainly

Press 1 did not produce the §41 sentence on its own. It sat in `ready` from 04:43:31 until **this
lane ran `docker restart ac-authserver` by hand over ssh at 04:57:57**, mid-press and not through
the app, and the sentence was printed two seconds later. `_advertise_realm()` runs after the last
stage and outside the `try` (`catalog/native.py:1543` at `cdd5eba2`, argued at `1535-1542`), so a
`ready` that ran out its window would have raised, the install would have been recorded as failed,
and the sentence would never have appeared. The entry now says so.

### `--checks`

`checks-green-round3.txt` holds two runs of `run-tests-vm.sh --checks` on m910q on 2026-09-06:
one with the round-3 edits uncommitted and one with the working tree clean at `30ccbfce`. Both:
`2805 passed, 4 skipped` (20.34 s / 20.01 s), `Success: no issues found in 72 source files` three
times (this platform, as Windows, as macOS), `All checks passed!`, `140 files would be left
unchanged.`, `=== --checks: ALL GREEN ===`, exit 0. The lane tip differs from `30ccbfce` only by
that file and this paragraph, which no check reads.

## Round 4 (2026-09-06) — the two thirds of the firewall branch nothing was watching

Round 3's `test_a_loopback_plan_asks_the_firewall_for_nothing` asserted an empty
`firewall_commands`, an empty `ssh_ports` and "no manual step containing TCP". All three stay true
with the `wants_firewall` guard cut back out of the firewalld and `alf` branches, so those two
thirds of the branch answered the same with the fix and without it. Measured here rather than
taken on trust: `mutations-round4.sh` checks the round-3 tip `ccfe7f97` out in the same throwaway
clone and runs the mutations there too.

`mutations-round4.txt` (m910q, 2026-09-06, throwaway `git clone --shared /home/pk/dads-mmo-lab
/home/pk/p7-b41-r4` detached at `92cacc44`, `__pycache__` purged before and after every mutation,
clone removed at the end — the last line is `ls: cannot access '/home/pk/p7-b41-r4': No such file
or directory`):

| block | line | what it printed |
| --- | --- | --- |
| baseline at `92cacc44` | 27 | `419 passed in 4.12s` |
| UNMUTATED probe | 30 | `UNMUTATED firewalld loopback: seams=[] fw=[] manual=[] warnings=1` |
| UNMUTATED probe | 32 | `UNMUTATED firewalld lan: seams=['detect_firewalld', 'detect_zones'] fw=['firewall-offline-cmd --add-port=3724/tcp', 'firewall-offline-cmd --add-port=8085/tcp'] manual=[] warnings=2` |
| UNMUTATED probe | 35 | `UNMUTATED alf loopback: detect_alf called=False firewall_state=None manual=[]` |
| UNMUTATED probe | 37-38 | `netsh loopback: manual=[]` / `netsh lan: manual=['Windows: set the network profile to Private …']` |
| MR1 `if backend == "firewalld"` | 54, 57 | red, `AssertionError: ('firewalld', ['detect_firewalld', 'detect_zones'])` at `test_networking.py:4929`, `1 failed, 418 passed` |
| MR1 probe | 58-59 | `seams=['detect_firewalld', 'detect_zones'] fw=[] manual=[] warnings=2`, the extra warning being "the game ports were written to the DEFAULT zone" with no port written |
| MR3 `if backend == "alf"` | 83, 86 | red, `AssertionError: ['detect_alf']` at `test_networking.py:4969`, `1 failed, 418 passed` |
| MR3 probe | 92 | `alf loopback: detect_alf called=True firewall_state=AlfState(enabled=True, …)` |
| MR4 `if backend == "netsh"` | 111, 114 | red, `AssertionError: ('netsh', ('Windows: set the network profile to Private …',))` at `test_networking.py:4934`, `1 failed, 418 passed` |
| MR2 control `elif backend == "none"` | 129, 132 | red, the "allow inbound TCP 3724, 8085 by hand" step, `1 failed, 418 passed` |
| restore | 141 | `419 passed in 4.32s` |
| round-3 tip `ccfe7f97` baseline | 151 | `419 passed in 4.06s` |
| MR1 at `ccfe7f97` | 160 | `419 passed in 4.09s` — **silent** |
| MR3 at `ccfe7f97` | 169 | `419 passed in 4.12s` — **silent** |
| back at `92cacc44` | 177 | `419 passed in 4.06s` |

What changed in the code: the test's firewalld seams now record their calls (`calls == []`), an
`alf` case with a recording `detect_alf` asserts `firewall_state is None`, and the loopback
assertions are on the WHOLE `warnings` tuple (`== (ONLY_THIS_COMPUTER,)`) and the WHOLE
`manual_steps` tuple (`== ()`). The netsh "set the network profile to Private" step moved inside
`wants_firewall`, because the network profile picks which Windows Firewall rule set is in force.

### The reason the branch gave for itself was false

The comment above `wants_firewall` said the loopback warning means no other machine can reach the
server, "so a hole for the game ports is a hole nothing is going to come through". Read on
yulon-ubuntu 2026-09-06 06:27:43 +02:00 (read-only, announced), on the install the 04:43 loopback
Apply had run against: `docker ps --format '{{.Names}}\t{{.Ports}}'` → `ac-authserver
0.0.0.0:3724->3724/tcp` and `ac-worldserver … 0.0.0.0:8085->8085/tcp`; `ss -ltn` → LISTEN on
`0.0.0.0:3724` and `0.0.0.0:8085`. The mode changes the address the realm row hands out and nothing
else — of everything a `NetworkPlan` hands back, `apply()` RUNS the contents of
`firewall_commands`, `portproxy_commands` and `realmlist_sql`, SHOWS `client_realmlist`, and leaves
`manual_steps`, `warnings` and `refusals` as text for the owner (the firewall instructions among
that text — the firewalld zone warnings, the backend-`none` step and the `netsh` profile step —
were each read under a `wants_firewall` gate at `6795cbdd`, `networking.py:3298`, `:3397` and
`:3481`, so none is emitted under `loopback`); beyond
the fields it writes `.yulon-network.json`. No container port binding is among them: a `portproxy`
rule forwards a host address to 127.0.0.1, and the UPDATE changes only the address the realm row
hands out. Another machine still connects and logs in, and is then told the world server is at
127.0.0.1, i.e. on itself. The comment now says that.

Rounds 4 and 5 both wrote this as a list of "the output fields a `NetworkPlan` carries" and both
lists were short — the dataclass has 18 fields
(`n=$(grep -n '^class NetworkPlan' pylauncher/yulon/networking.py | cut -d: -f1); awk -v s=$n
'NR>=s && NR<=s+75' pylauncher/yulon/networking.py | grep -cE '^ +[a-z_]+:'` → `18` at
`b481be54`), and round 5's four left out `manual_steps`, a field the comment was about. Round 6
stopped enumerating the fields but wrote a new exhaustive claim (`manual_steps` as the one field
carrying a firewall instruction) that the tree refuted — the firewalld zone warnings go to
`warnings` — so at `6795cbdd` the merger removed the claim; rounds 4, 5 and 6 each wrote one
such sentence and each was false. Round 6 also
and stated what `apply()` does with them; the conclusion (no networking mode writes a container
port binding) is the one the 06:27:43 reading above supports and is unchanged.

The two owner-visible strings were reviewed and KEPT: `networking.ONLY_THIS_COMPUTER` and the
install line from `loopback_chosen_on_purpose()`. Both are quoted verbatim in committed records of
the presses that closed this entry (`widget-driver-output.txt` lines 31 and 52, `README.md:51`,
`closing-step-output.txt:13`, `yulon-ubuntu-press/yulon-log-press-chosen.txt:12`) and the second is
asserted literally by `closing_step_driver_b41.py:121`; rewording them would make the closed entry
quote sentences the tree no longer prints, and no code reads either string to decide anything. The
reading above is recorded in each string's docstring instead.

### `mutations-round2-meta.txt`

The §41 entry's "each left both lists empty and the whole 416-test file green" describes a run made
at the round-2 tip `9f0c2fa2`, before round 3's tests existed. That transcript lived only in a
scratch directory, so the clause had nothing a reader could re-derive; it is now committed here
verbatim as `mutations-round2-meta.txt`, with `416 passed` at lines 25, 43, 54 and 66 (baseline,
M5, M6, restore) and the clone's removal at line 68.

### `--checks`

`checks-green-round4.txt` holds two runs of `run-tests-vm.sh --checks` on m910q on 2026-09-06,
both `=== --checks: ALL GREEN ===` and exit 0: `2805 passed, 4 skipped` in 20.62 s (the round-4
code edits uncommitted over `ccfe7f97`) and in 20.75 s (with the corrected docstrings and these
gate files, uncommitted over `2586b913`); `Success: no issues found in 72 source files` three times
each (this platform, as Windows, as macOS), `All checks passed!`, `140 files would be left
unchanged.` A third run at `5bf29ec5` and a fourth after two sentences about
what a `NetworkPlan` carries were tightened are at the end of the same file: `2805 passed, 4
skipped` in 20.32 s and in 19.62 s, the same three mypy successes each, ruff and black clean,
`=== --checks: ALL GREEN ===`, exit 0. Their headers say `==> syncing lane/b41 (<sha>)`, which the
runner prints whether or not the tree has uncommitted edits, so "clean" is not readable from those
four transcripts; `checks-green-round5.txt` below carries `git status --porcelain` in the same
capture instead. None of these four runs covers the lane tip: they are at `ccfe7f97`, `2586b913`,
`5bf29ec5` and `5bf29ec5`.

## Round 5 (2026-09-06) — the citations re-derived at the commit they name

Eight source-line citations in this file and in §41 had drifted. `2586b913` added 17 lines to
`native.py` and edited `bug-checklist.md` and this README in the SAME commit, so every line number
written into the md that round was already off by the size of that commit's own docstring edit —
including the round-4 "correction" of `catalog/native.py:1527` to `1526`, which reproduces at no
commit from `2586b913` onward. Re-derived at `cdd5eba2` with the tree clean:

| written as | re-derived at `cdd5eba2` | the line it points at |
| --- | --- | --- |
| `networking.py:2989` | `networking.py:3008` | `def record_network_intent(server_dir: Path, mode: Mode) -> str:` |
| `native.py:2724` | `native.py:2741` | `tokens = {"REALM_HOST": INSTALL_REALM_HOST, "WORLD_PORT": str(self.entry.ports.world)}` |
| `native.py:1526` | `native.py:1543` | `yield from self._advertise_realm(replace(ctx, state=state))` |
| `native.py:1518-1525` | `native.py:1535-1542` | the comment block that opens "OUTSIDE the `try`, and after the last stage, on purpose." |
| `native.py:1522-1525` | `native.py:1539-1542` | the half of that comment that argues for `INSTALL_REALM_HOST` |

`yulon-ubuntu-press/why-ready-waited.txt` line 5 also says `native.py:2724`, and was left alone: it
is a capture taken 2026-09-06 05:01:58 during the press, and `git show 96251d57:…/native.py |
sed -n 2724p` prints the `tokens = {"REALM_HOST": …}` line, so it was true of the commit the press
ran at. At `cdd5eba2` that line is 2741.

Each of the eight now carries `at cdd5eba2`, so a later edit to `native.py` or `networking.py`
moves the line without silently invalidating the citation. `native.py:202`, `native.py:245`,
`native.py:92`, `networking.py:194`, `networking.py:220`, `controller_view.py:607` and
`controller_view.py:1835` were re-derived at `cdd5eba2` too and had not moved — but five of those
seven carried no SHA at all until round 6 pinned them; see the round-6 section below, which also
covers the citations this lane killed in OTHER entries and which round 5 never looked at.

The drift has a shape, so round 5 has a shape: commit A (`cdd5eba2`) changed only `pylauncher/`,
and commit B changed only md and txt. The numbers in B were read out of A's files with the tree
clean, and nothing in B can move them.

Two sentences were also wrong about the code rather than about a line number. The `plan()`
docstring pinned its firewalld/alf/netsh loopback reading to `2586b913`, while
`mutations-round4.txt`'s own header says `commit: 92cacc44…` and `mutations-round4.sh` says
`SHA=92cacc44…`; `networking.py` and `test_networking.py` both changed between the two commits.
And the `wants_firewall` comment (and its twin in §41) said the only ACTIONS a `NetworkPlan`
carries are `firewall_commands`, `portproxy_commands` and `realmlist_sql`. Measured at `cdd5eba2`:
the dataclass also carries `client_realmlist` (`networking.py:2786`), which `apply()` puts in its
report (`networking.py:3623`) and the Networking tab prints (`controller_view.py:1908-1909`), and
`apply()` writes the chosen mode to `.yulon-network.json` through `record_network_intent()`
(`networking.py:3626`) — all three at `cdd5eba2`. This branch's own feature. The conclusion the
sentence was making is untouched: none of it is a container port binding.

### `--checks`

`checks-green-round5.txt` is one run of `run-tests-vm.sh --checks` on m910q, 2026-09-06 07:02,
taken at commit A with the tree clean. Lines 1-5 are the capture's own header, written before the
runner was called: `### date: 2026-09-06T07:02:39+02:00`, `### git rev-parse HEAD:`,
`cdd5eba2a46abfda71e27e9ab8bf05366fbbff85`,
`### git status --porcelain (empty means the tree is the commit):` with nothing under it, and
`### ---`. Line 6 is `==> syncing lane/b41 (cdd5eba2) to m910q`. Then
`2805 passed, 4 skipped in 20.34s`,
`Success: no issues found in 72 source files` three times (this platform, as Windows, as macOS),
`All checks passed!`, `140 files would be left unchanged.`, `=== --checks: ALL GREEN ===`, exit 0.
Commit B adds only md and txt on top of it, which no check reads.

## Round 6 (2026-09-06) — the citations this lane killed in other entries, and the enumeration retired

Round 5 re-derived §41's own eight citations and then said the drift mechanism was closed. It was
not: this lane's edits to `pylauncher/yulon/networking.py` and `pylauncher/yulon/catalog/native.py`
(`git diff --numstat cfb4c04f b481be54 --` those two files → `306  24` and `116  8`) had already
moved lines that OTHER closed entries cite, and nobody had looked outside §41.

Three such citations were found, all true at `cfb4c04f` and dead at the lane tip:

| entry | said | printed at the lane tip | now says |
| --- | --- | --- | --- |
| §39, `bug-checklist.md` measured-closed paragraph | `networking.py:2956` | `@dataclass(frozen=True)` | `networking.py:3145` at `b481be54` |
| §39, same sentence | `networking.py:2967` | `cannot tell a decision from a leftover.` | `networking.py:3156` at `b481be54` |
| §23 | `native.py:335` | a bare `"""` | `native.py:406` at `b481be54` |

The §39 pair was never pinned to the three commits its own sentence names: `git show
<c>:pylauncher/yulon/networking.py | grep -n 'enable_firewall: bool = False'` printed `2310` at
each of `9b0eb089`, `e72bc758` and `ee361035`. 2956/2967 were the merged tree's numbers at
`cfb4c04f`, which is why they matched there and stopped matching here.

How the list was bounded: `citation-scan-round6.sh` reads every `networking.py:N`, `native.py:N`
and `controller_view.py:N` citation in `pyplan/**/*.md` and prints the pair when the cited line's
CONTENT differs between `cfb4c04f` and the lane tip. `citation-scan-round6.txt` is its output — 79
pairs, which is a superset of "this lane killed it": a pair is also printed when the citation was
already stale at `cfb4c04f`, and when it is deliberately pinned to a later commit (§41's own, at
`cdd5eba2`/`d1e41fbc`/`b481be54`, are in there for that reason). Each of the 79 was then read
against the sentence citing it. Outside §41 the base line matched the sentence the scan could see
in exactly two cases, `networking.py:2956` (§39) and `native.py:335` (§23); everywhere else the
base line was already something unrelated — e.g.
`bug-checklist.md:395` cites `controller_view.py:1307` for "`_module_action()` returns early" and
that line at `cfb4c04f` is a docstring about a second press; `bug-checklist.md:904` cites
`native.py:1061` for `already_cloned()` and that line is a parameter; the `native.py:272` pair in
the 2026-08-28 hunt runlogs, `controller_view.py:931` in `findings-m910q.md`, `native.py:1969` in
`phase7-decisions.md:1171`, `native.py:1955`/`:1990` in `checklist.md` and `native.py:1476` in
`upstream-cmangos-doodad-drop.md` are the same shape. Those predate this lane, so round 6 left them
as found rather than opening a tree-wide pass from inside a bug entry. The scan cannot see a bare
`:NNN` continuation, which is how §39's `:2967` is written; that one was found by reading the
sentence, and §23's `:709`, `:377`, `:667-669`, `:894`, `:899` were checked the same way and were
all already stale at `cfb4c04f`.

Five §41 citations that were correct but carried no SHA now carry `at b481be54`, which is what
made the three above silent: `controller_view.py:607`, `controller_view.py:1835`,
`networking.py:220`, `native.py:245` and `native.py:92`.

The `wants_firewall` comment's enumeration was retired rather than corrected a third time — see the
paragraph above under round 4's "The reason the branch gave for itself was false".

Round 6 kept round 5's shape: commit A changed only `pylauncher/yulon/networking.py`, commit B only
md and txt, and B's numbers were read out of A's files with `git status --porcelain` empty.

The m910q box was read back read-only on 2026-09-06 at 07:49:07 +02:00 (announced in
`~/bin/claude-say` before and after), through the same route the drivers in this folder use
(`docker exec -e MYSQL_PWD="$(cat /home/pk/vanilla-75b/.db_password)" vanilla-db mariadb -uroot
-N -B -e "SELECT id,name,address,port FROM realmd.realmlist;"`): `1  MaNGOS  192.168.10.134
8085`, which is what "The box, put back" above says was restored; `/home/pk/vanilla-75b` holds
`.yulon-install.json` and no `.yulon-network.json`; `vanilla-db` (healthy), `vanilla-realmd` and
`vanilla-mangosd` up 44 hours; 34 GB free on `/`. Round 6 also removed a leftover from the round-5
reviewer, `/home/pk/b41-r5-review-mut.sh`, and left nothing of its own on the box.

Left as found, and named so it is not read as unnoticed: five lines in this folder carry a
`/home/pk/yulon-runs/wt-b41/...` path — `red-before-the-code-networking.txt:1,24,25` and
`red-before-the-code-spine-and-view.txt:6,7`. All five are the test runner's own header
(`==> creating remote checkout …` and pytest's `rootdir:`), not a path this record cites as
evidence, and `git grep -l yulon-runs cfb4c04f -- pyplan/ | wc -l` printed `19`, so 19 files
already at the branch point carry the same header. Round 6 did not edit a captured transcript to
remove them; whether the rule reaches runner headers is the owner's call, not this lane's.

### `--checks`

`checks-green-round6.txt` is one run of `run-tests-vm.sh --checks` on m910q, 2026-09-06 07:39,
taken at commit A with the tree clean. Lines 1-5 are the capture's own header, written before the
runner was called: `### date: 2026-09-06T07:39:41+02:00`, `### git rev-parse HEAD:`,
`b481be54af6d7ef40b4427daf196778897c3c0f3`,
`### git status --porcelain (empty means the tree is the commit):` with nothing under it, and
`### ---`. Line 6 is `==> syncing lane/b41 (b481be54) to m910q`. Then
`2805 passed, 4 skipped in 20.41s`,
`Success: no issues found in 72 source files` three times, `All checks passed!`,
`140 files would be left unchanged.`, `=== --checks: ALL GREEN ===` (line 62), exit 0.
Commit B adds only md and txt on top of it, which no check reads.
