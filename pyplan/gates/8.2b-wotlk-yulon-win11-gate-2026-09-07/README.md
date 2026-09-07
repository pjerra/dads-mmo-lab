# 8.2b — Command channel, WoW WotLK on native Windows — live gate

**Where and when.** `yulon-win11-gate`, 2026-09-07 07:15Z – 07:44Z, against the WotLK install
`D:\gate\wotlk-server77` that the 7.7 gate built on this box on 2026-09-05. Code at `c5576aa4`
shipped to `C:\gate\src82b`, then at `3af8edb4` for everything from "the fix" onward. The box shows
PST; every stamp below is the script's own UTC.

**Nothing was built for this.** The install was still on the box's D: drive, its `install_id`
`f7357748` still matches the four cached images (`yulon.local/ac-wotlk-*:native-f7357748`), and the
containers were sitting `Exited (0)` from 43 hours earlier. The whole gate is a stop, a press, a
start and six readings.

## What 8.2b is for

The Console tab reaches a container's stdin through a pty. **Windows has no pty to reach through**,
so on this host the console could show output and never send. The command channel is the route that
does not need one, and this box is where "the app can send a command" becomes true for the first
time.

## The clauses, and what the box said

| Definition of done (as 8.2a) | Result |
|---|---|
| With the world stopped, one press writes the configuration | `AC_SOAP_ENABLED "1"`, `AC_SOAP_IP "0.0.0.0"`, `AC_RA_ENABLE "0"`, `AC_AI_PLAYERBOT_COMMAND_SERVER_PORT "0"` — screenshot `4-` |
| The user's ordinary Start brings the world up through the staged start | started 07:34:28Z, ready 07:35:55Z (82 s) |
| Restart count unchanged, and THIS run's ready marker in its own log | `restarts=0`; `AzerothCore rev. 413bea61a85e+ … (worldserver-daemon) ready...`, read with `--since` this run's `StartedAt` |
| The tab reads verified with the time | `Command channel: verified as YULON_F7357748 at 2026-09-07 07:35 UTC.` — **first settle attempt**, screenshot `5-` |
| The account row and its access row exist at level 3 | `101  YULON_F7357748` and `account_access` `101  3  -1` |
| The reply text is in the capture, **taken from the host** | `answered` — `AzerothCore rev. 413bea61a85e+ 2026-09-04 09:03:49 -0700 (Playerbot branch) (Unix, RelWithDebInfo, Static)` / `Connected players: 0. Characters in world: 500.` |
| Nothing listening on 8888 or 3443 inside the container | `[7878, 8085, 39917]` — both silent, read from `/proc/net/tcp` |
| A wrong credential reads refused, and the repair returns it to verified without a second account | refused → `repair()` → `Verified at 07:38 UTC`; **1 account before, 1 after**; round trip `answered` — screenshots `6-`, `7-` |
| An occupied port leaves the configuration rolled back and the server startable | the tab's own sentence, screenshot `8-`; `AC_SOAP_ENABLED` gone; the server then started **with 7878 still held** |

## The defect this box found, and it is not a small one

**The only control that turns the channel on was live exactly when pressing it could not work.**

The press refuses while the world is running — that is 8.2a's whole shape, because a failed bind is
not atomic and on the CMaNGOS trees it costs character saves. The button was enabled on
`Verdict.stable`, and `stable` is only ever true while the world **is** running. So:

```
world up      ENABLE live      → press refuses
world stopped ENABLE greyed    → the press that would work cannot be reached
```

and the app's own refusal closes the circle (`1-refusal-while-running.png`):

> the server has to be stopped before the command channel can be turned on. **Stop it, press this
> again**, then start it as usual — the setting is read when the world starts, and writing it under
> a running world risks the world.

Stopping it is what greys out the button you were told to press again.

**8.2a did not catch it because its gate called `InstallChannel.enable()` directly.** The mechanism
was proved; the route a person has was not. That is the shape of the lesson already filed as
`reviews-check-functions-not-call-sites`, arriving through a GUI this time.

Fixed at `3af8edb4`: `_press_is_allowed()` keeps `stable` — an `up` world with an unreachable
database is still not one to aim a command at, which the TBC gate bought — and adds `stopped`, the
state the press is FOR. `restart_loop` and `unknown` stay shut, because both may be running. Four
mutations of the predicate, all four killed, each by a different test.

Screenshots `2-` and `3-` are the same install in the same state, minutes apart, with only that one
line different: `gate82b_pair.py` puts the old predicate back, photographs the tab, restores the fix
and photographs it again.

## Two things about running a Qt gate on this box

**Qt had no fonts.** `QT_QPA_PLATFORM=offscreen` under this venv looked for
`PySide6/lib/fonts` and found nothing, so the first screenshots were rows of tofu boxes and were
deleted rather than kept — an illegible artifact is not evidence. Copying four faces out of
`C:\Windows\Fonts` into that directory fixed it.

**A gate script must decode the way the app does.** `subprocess.run(..., text=True)` picks cp1252
on Windows and died on the colour bytes in this core's log:

```
UnicodeDecodeError: 'charmap' codec can't decode byte 0x90 in position 578
```

The app was never exposed to it — `runner.py:331` passes `encoding="utf-8", errors="replace"`
everywhere — so this was the gate measuring its own bug. Fixed in `gate82b.py`.

## The files

* `gate82b.py` — 8.2a's own two gate scripts merged, with two constants changed and nothing else,
  so that "no new code" is a claim the script itself supports.
* `gate82b_gui.py` — the press through the BUTTON rather than the seam. This is the script that
  found the defect.
* `gate82b_pair.py` — the before/after pair of the interlock, one line apart.
* `raw-logs.txt` — every stage's unedited output, in the order they were run.
* Screenshots `1-` … `9-`, all of the Server tab on Windows 11.

## How to re-run it

```
python gate82b.py      ports | before | press | prove | rows | marker | break | repair | port | after
python gate82b_gui.py  stopped | running | press | start | after
python gate82b_pair.py stop | before | after | start
```

Docker Desktop must be running in the **interactive** session first; from ssh it is not:

```
schtasks /create /tn dml-dockerup /tr "\"C:\Program Files\Docker\Docker\Docker Desktop.exe\"" ^
         /sc once /st 00:00 /ru PK /it /f
schtasks /run /tn dml-dockerup
```
