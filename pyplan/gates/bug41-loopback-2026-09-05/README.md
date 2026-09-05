# bug-checklist §41 — a realm can be set to loopback on purpose

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
