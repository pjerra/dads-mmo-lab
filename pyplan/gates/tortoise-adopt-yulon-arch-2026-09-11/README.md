# T29 Half 1 — a server behind the Steam entries: `~/tortoise-vm` adopted, started, and played against (`yulon-arch`, 2026-09-10/11)

**The Steam entries now have something to play against.** Yu'lon (from `~/y8` at `803a686`,
venv `~/y8v313`, Python 3.13.15 / PySide6 6.11.2) took `/home/pk/tortoise-vm` through the WoW
Tortoise tile's **"Use existing…"**, started it from the Server tab, and the **"Turtle WoW"**
entry launched from Big Picture reached the login screen and **authenticated against it**.

**The adopt press T19 built could not run here, and that is the correct answer, not a
failure.** These databases already carry Yu'lon's own import marker, because this tree was
made by Yu'lon's installer on this box — not by the shell scripts the m910q install came
from. The Modules tab greys the button, and the press, driven directly, refuses in its own
words. Section 3 has both.

Every stamp in every file here was read from the box's own clock (UTC). Every action on the
box was announced first through `~/bin/claude-say`. Every sentence below is backed by a file
in this folder. **The Steam account is named only by its userdata id `18347166`**; no process
environment and no `loginusers.vdf` was captured.

## The box, as found — `01-as-found.txt`, `02-containers-as-found.txt`

At 22:03:24 the three Tortoise containers read `Exited (0) 46 hours ago` under compose project
`yulon-wow-tortoise-54fa64f0`; `~/.local/share/yulon/` held only `yulon.log` — **no
`state.json`**, so every catalog tile read "Install / Use existing…"; `~/tortoise-vm` was 4.1 GB
carrying `.yulon-install.json` with nine completed stages including `import` and a `last_error`
from 2026-09-08 ("the server started but never reported ready: `tortoise-mangosd` restarted 5
times…"); the client's `realmlist.wtf` read `set realmlist logon.turtle-server-eu.kz` (CRLF);
Steam ran in Big Picture with the two entries; `gamescope` was not installed.

## 1. "Use existing…" — frames 01-05, `01-as-found.txt`

`frame-01-catalog-tab.png` is the Catalog tab with the four tiles, each offering **Install** and
**Use existing…** — the state a missing `state.json` produces. The press on WoW Tortoise's
"Use existing…" opened the folder picker (`frame-02`), `/home/pk/tortoise-vm` was typed into it
(`frame-03`), and because this entry's plan sets `requires_client_dir`, a second picker asked
for the client (`frame-04`) and was given `/home/pk/clients/TurtleWoW`.

`yulon.log` at 22:09:25: `attaching existing wow-tortoise install at /home/pk/tortoise-vm` —
`catalog_view.attach_existing()`'s own line. `state.json` appeared the same second and names the
install:

```json
{"schema_version": 1,
 "installs": [{"game": "wow-tortoise", "server_dir": "/home/pk/tortoise-vm",
               "client_dir": "/home/pk/clients/TurtleWoW", "wsl_distro": null}]}
```

`frame-05-server-tab-attached.png` is the controller tab it opened — "WoW Tortoise —
tortoise-vm", `status: db down, auth down, world down`, and the eight sub-tabs.

## 2. The database, started alone — `03-startdb.txt`

`startdb.py` calls `docker.start_database(spec, SERVER, …)`, the primitive `stage_start_db`
calls, exactly as the m910q gate's own `startdb.py` did and for the same reason: the Modules
tab takes its adopt reading once per time the database comes up, and with the database down
that reading is `unreadable` and the button is greyed regardless of what the databases hold
(T19 finding 2). Its own line: `start_database(): starting tortoise-db alone`,
`world_running before: False` … `after: False`. The world server was not touched.

## 3. The adopt press has nothing to adopt here — `04-probe-before.txt`, `05-adopt-press-refusal.txt`, `frame-06`

`04-probe-before.txt` is the gate's own answer through the same `MarkerGate` the press uses,
read-only:

| reading | value |
| --- | --- |
| marker table anywhere on this server | `tw_world  yulon_install` |
| `probe answer` | **`imported`**, complete `True` |
| detail | `tw_world.yulon_install records a finished import by an older plan (a1dfc9fefe235d37, this app's is 8b60e764371f2293); it is kept as it is` |
| `import_reads_as_finished` | **`True`** |
| `adoption_gaps` | `()` |
| `adopt_state` (the button's own reading) | `ImportState(state='imported', …)` |

The m910q install read `populated` and `import_reads_as_finished: False` — the refusal the
adopt press was built to lift. **This one reads `imported` before anything is pressed**, by an
*older plan hash*: it was installed by Yu'lon's own engine, which writes that row at the end of
a successful import.

`frame-06-modules-tab-adopt-greyed.png` is the Modules tab as the reading left it: **"Adopt as
imported…" greyed**, "Apply pending database updates…" and "Rebuild the server…" enabled. The
code says so at the button's own construction site (`ui/controller_view.py:4755-4761`):

> *this one is greyed until the databases themselves say `populated`, which is the state of an
> install this app did not make. On a server Yu'lon installed it never lights up at all,
> because that server already carries the row (T19).*

So the GUI cannot reach this press on this tree. `05-adopt-press-refusal.txt` drives the same
engine method the button is wired to (`adopt_as_imported()`) so the refusal is in the press's
own words rather than only in a greyed pixel. It yields the opening note, `start-db`, and then:

```
  > The databases read as imported: tw_world.yulon_install records a finished import by an older plan …

REFUSED (InstallerError), the sentence a user reads:
  ! WoW Tortoise's databases already carry Yu'lon's marker (…), so there is nothing to adopt —
    they already read as a finished import. Nothing was written. If you meant to apply the files
    this install plan has gained since, press "Apply pending database updates…" instead.
```

The same file carries the confirmation dialog this button would have shown, composed against
the real folder. **Nothing was written**: the refusal is raised inside `stage_adopt` before
`write_import_marker()`, and the database was already up so the press's conditional stop did
not fire.

## 4. The realmlist, Yu'lon's way — frames 07-08, `06-realmlist-after-apply.txt`, `07-client-realmlist.txt`

The Networking tab's **"Only this computer (127.0.0.1)"** → **Show plan** (`frame-07`) composed
this install's plan: `Mode: loopback`, `Ports: 3724, 8090`, `firewall: none`, `Players set
realmlist to: 127.0.0.1`, and the one statement `UPDATE tw_logon.realmlist SET
address='127.0.0.1' WHERE id=1;`, with the tab's own warning that the realm will advertise a
local-only address. **Apply** (`frame-08`) reported `✓ realmlist → 127.0.0.1` and `⚠ restart the
server so the new realmlist address is used`.

`06-realmlist-after-apply.txt` reads the row back through the app's own
`networking.realmlist_address_query()` — `SELECT address FROM tw_logon.realmlist WHERE id=1;`
→ `127.0.0.1`, and the whole row: `1 | Tortoise WoW | 127.0.0.1 | 8090`.

The client's own file is the second half and **Yu'lon has no button for it**: the Networking tab
writes the realm row and *names* the address for the player, and `networking.write_client_realmlist()`
has no UI caller in this build (`tests/test_spine.py` lists it as a writer with a glob). Rather
than hand-edit, `clientrealm.py` calls that function — Yu'lon's own code doing it
(`07-client-realmlist.txt`): `b'set realmlist logon.turtle-server-eu.kz\r\n'` →
`b'set realmlist 127.0.0.1\n'`. **This is the only thing under `~/clients/TurtleWoW` this run
touched, and it is left pointing at the local server.**

## 5. Start, and the server running — `frame-09`, `08-world-ready-and-ports.txt`

Start was pressed on the Server tab at 22:15:28. `docker logs` is cumulative on a restarted
container, so the ready line is bounded by an epoch window the way the m910q gate bounded its
own: the window `--since 1789078528 --until 1789078530` holds **0 lines and 0 ready matches**,
and the whole run window `--since 1789078528` holds 1067 lines and **exactly one** match of the
catalog entry's own `ready.world` pattern:

```
World server is up and running! Loading time: 1 minutes 53 seconds
```

`tortoise-realmd` in the same window: `Welcome to Turtle WoW!` / `Login server is up and
running.` Ports **3724** and **8090** — the entry's `auth` and `world` — listening on
`0.0.0.0`. `frame-09-server-tab-running.png` is the running tab: **`up — 0 players, 473 bots, up
2m`**, `status: db up, auth up, world up`, Start greyed and Stop live.

## 6. A throwaway account — frames 10-11

`frame-10` is the Accounts tab's create form with username **`T29TEST`** and a masked password;
`frame-11` is the result — **`T29TEST: created (id 109).`** and the account list showing
`T29TEST — id 109 — GM level 0` beside the four `YULON*` probe accounts earlier phases left.
**The account name is `T29TEST`.** Its password was generated on the box into a 0600 file and
typed with `xdotool type --file`, so it never entered an argv, this folder, or the transcript;
the file was shredded at the end (`28-half2-box-as-left.txt` in the other folder lists what is
left in `~/t29`). **No account of the owner's was used and no character was touched.**

## 7. The client entry reaches the server — frames 12-16, `10-realmd-login.txt`

`frame-12` is Big Picture with the **"Turtle WoW"** tile focused; `frame-13` is its page with the
green **Play**. The entry launched (Steam's `reaper SteamLaunch AppId=3811632958`), and
`frame-14-client-login-screen.png` is the Turtle WoW login screen, **Version 1.18.1 (7272)
(Release)**.

`frame-15` shows `T29TEST` and a masked password typed into it, and `frame-16` is what pressing
Login produced: the client is **past the login screen**, on WoW's realm-choosing page. The
realmd container's own log for that window (`10-realmd-login.txt`, `--since` five seconds before
the press) is the other side of it:

```
Accepting connection from '172.20.0.1'
… SELECT sha_pass_hash,id,locked,… FROM account WHERE username = 'T29TEST'
[AuthChallenge] Account 'T29TEST' using IP '172.20.0.1' is using 'enGB' locale (0)
[AuthChallenge] Account 'T29TEST' using IP '172.20.0.1' successfully authenticated
… SELECT id, name, address, port, … FROM realmlist WHERE (realmflags & 1) = 0 ORDER BY name
… SELECT `numchars` FROM `realmcharacters` WHERE `realmid` = '1' AND `acctid`='109'
```

The SRP verifier, salt and session key that row gained are redacted in that file; no claim
rests on their value. **The Steam entry, the client, the realmlist and the server are one
working chain.** No character was created and none was logged in.

## 8. Stop, and the box as left — `frame-17`, `frame-18`, `11-half1-box-as-left.txt`

`frame-17-tab-reopened-after-vm-reset.png` is Yu'lon restarted after the VM reset (deviation 1):
it reopened the controller tab **straight from `state.json`** with no second "Use existing…",
reading `up — 0 players, 499 bots, up 9m`.

Stop was pressed on the Server tab at 22:52:30. `frame-18` reads `stopped`, `status: db down,
auth down, world down`, and the app's own line *"The server's log was saved to
/home/pk/.local/share/yulon/logs/wow-tortoise-54fa64f0-20260910T225230Z.log"*.
`11-half1-box-as-left.txt` at 22:54:07: all three containers `exited exit=0` by `docker
inspect`, **nothing listening on 3724 or 8090**, no Yu'lon process, no Steam process, no client
process, `state.json` intact, the client's realmlist at `127.0.0.1`.

Both Start and Stop went through the app's own Server tab, never a raw `docker` verb.

## 9. `shortcuts.vdf` — `09-shortcuts-vdf.txt`, and `27-shortcuts-as-left.txt` in the other folder

The file's sha256 **moved**, and the entries did not. Steam writes `LastPlayTime` into
`shortcuts.vdf` whenever a shortcut is launched, and this ticket asked for both entries to be
launched. Read with the app's own codec (`yulon.steam.vdf_parse`), the file is 681 bytes before
and after, holding exactly `Turtle WoW` (`…/clients/TurtleWoW/WoW.exe`) and `Turtle WoW Server`
(`~/y8v313/bin/python` + `…/y8/pylauncher/main.py`); only `LastPlayTime` differs. The three
readings are tabulated in `27-shortcuts-as-left.txt`.

## Deviations, stated

1. **The VM ran out of memory and was hard-reset mid-run.** With the Proton client, Steam, the
   three containers and Yu'lon all up, the host reported `MemoryDemand 21130903552` against
   `12884901888` assigned; the guest swapped until `ssh` no longer completed its banner
   exchange, while the console still showed the client's login screen. The lead killed the
   stuck worker from the host at about 22:41 on the box's clock and restarted the VM with
   **20 GB instead of 12**. Nothing of Half 1 was lost: `state.json`, the client's realmlist and
   the three containers (docker's restart policy) all came back, and `frame-17` is Yu'lon
   reattaching from `state.json` on its own. The login attempt (section 7) was made after the
   reset, with `free -m` watched at each step.
2. **The adopt press was not pressed from the button** — it cannot be, on this tree. The button
   is greyed by its own enabling rule and the press's refusal was taken by driving
   `adopt_as_imported()` directly, which is the method the button is wired to.
3. **The database was started alone by `startdb.py`**, Yu'lon's own `docker.start_database()`,
   because the Modules tab's enabling reading is only taken while the database is up. Same
   concession, same reason, as the m910q gate.
4. **The client's `realmlist.wtf` was written by `networking.write_client_realmlist()` from a
   script**, because no button in this build writes it. The DB half of the same address was
   written by the Networking tab's own Apply.
5. **The GUI was driven with `xdotool` by pixel coordinates** read off the VM console frames.
   The console renders the guest's 1920x1080 letterboxed into 1024x768, so every click was
   computed as `x/0.5333`, `(y-96)/0.5333` and checked against the next frame.
6. The first launch of the client entry (22:23) was closed by four `Escape` presses meant to
   skip the intro cinematic; the client exited instead. The second launch (22:25) was left alone
   and reached the login screen. Nothing was written by either.
7. `~/tortoise-vm/.yulon-install.json` was neither read as authority nor written. Its
   `last_error` from 2026-09-08 (the world "never reported ready") is still there and is now
   contradicted by `08-world-ready-and-ports.txt`: on this run the world reported ready in
   1 minute 53 seconds.

## Files

`01-as-found.txt` · `02-containers-as-found.txt` · `03-startdb.txt` · `04-probe-before.txt` ·
`05-adopt-press-refusal.txt` · `06-realmlist-after-apply.txt` · `07-client-realmlist.txt` ·
`08-world-ready-and-ports.txt` · `09-shortcuts-vdf.txt` · `10-realmd-login.txt` ·
`11-half1-box-as-left.txt` · the scripts that produced them (`containers.py`, `startdb.py`,
`probe.py`, `adoptpress.py`, `realmread.py`, `clientrealm.py`, `shortcuts.py`, `ready.sh`,
`realmd.sh`, `asleft1.sh`) · and eighteen frames:

| frame | what it shows |
| --- | --- |
| `frame-01-catalog-tab.png` | the Catalog tab, every tile offering Install / Use existing… |
| `frame-02-pick-server-folder.png` | "Select the folder where WoW Tortoise is installed" |
| `frame-03-server-folder-typed.png` | `/home/pk/tortoise-vm` in the picker |
| `frame-04-pick-client-folder.png` | the second picker, for the 1.18.1 client |
| `frame-05-server-tab-attached.png` | the controller tab, db/auth/world down |
| `frame-06-modules-tab-adopt-greyed.png` | **"Adopt as imported…" greyed**, updates and rebuild live |
| `frame-07-networking-plan-loopback.png` | the loopback plan, ports and the realmlist UPDATE |
| `frame-08-networking-applied.png` | `✓ realmlist → 127.0.0.1` |
| `frame-09-server-tab-running.png` | `up — 0 players, 473 bots, up 2m`, all three up |
| `frame-10-accounts-create-form.png` | `T29TEST` and a masked password |
| `frame-11-account-created.png` | `T29TEST: created (id 109).` |
| `frame-12-bigpicture-turtle-wow-tile.png` | the client entry focused in Big Picture |
| `frame-13-bigpicture-turtle-wow-page-play.png` | its page, green Play |
| `frame-14-client-login-screen.png` | the login screen, 1.18.1 (7272) |
| `frame-15-client-credentials-entered.png` | `T29TEST` typed in |
| `frame-16-client-authenticated-realm-chooser.png` | **past the login screen** — the realm chooser |
| `frame-17-tab-reopened-after-vm-reset.png` | Yu'lon reattaching from `state.json` after the reset |
| `frame-18-server-tab-stopped.png` | `stopped`, and the saved server log |

## Checks

`tests/test_no_secrets_in_evidence.py` was run against this worktree before the commit.
