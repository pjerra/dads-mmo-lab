# 8.2e — Command channel, WoW Tortoise — live gate

**Where and when.** `m910q`, 2026-09-07 12:07Z – 12:13Z, against the Tortoise install at
`~/tortoise-server`, code at `e046a28c` in `~/gate82e`. Nothing was pressed and nothing was
written: this box has no press. The world was restarted once, deliberately, for the clause that
needs it.

**This tree has no listener at all.** Its mangosd links neither gsoap nor `RASocket` — the complete
source and dependency lists name neither — so there is nothing to enable, no port to publish and no
account to authenticate. What it has is the console the Console tab already types at, and `8.2e`
wraps that in the one method every feature module is handed.

## The clauses

| Definition of done | Result |
|---|---|
| The tab carries the console sentence | *"Commands reach this server through the worldserver console on the Console tab. This core has no remote command listener to turn on — it is built with neither SOAP nor the telnet console — so there is nothing to set up here."* |
| No set-up button exists on this entry | `ENABLE button exists at all: False`, `REPAIR button exists at all: False` — screenshot `1-` |
| A command from the Server tab's own probe answers through the attach console in about five seconds | **3.6 s** — once, not twice: run.txt records a single probe (`the probe took 3.6s`) (audit, 2026-09-08) — `Core revision: unknown / … Linux_x64 (little-endian)`, `Players online: 0. Max online: 0.`, `Server uptime: 41 Minutes 42 Seconds.` — screenshot `2-` |
| A reply window with no prompt reads as could-not-ask, never as failure | *"Could not ask: the console printed no prompt inside the reply window, so nothing in it is this command's answer. The command may still have run, so nothing here is a failure."* — screenshot `3-` |
| A mutation reports itself as confirmed only after its verify read answers | nothing on this tree is offered a mutation yet, and the reason is below |

## Why this transport is offered no mutations, shown rather than argued

The probe's own reply is the argument, and it took two readings to state it honestly.

**The first reading over-claimed.** What came back from `server info` was four lines of the server's
own SQL log sitting between the prompts:

```
[1 ms] SQL: SELECT id FROM character_pet WHERE owner = '791'
[0 ms] SQL: REPLACE INTO `character_pvp_currency` (`guid`, `honor`, …) VALUES (721, 0, 0, 0, 20698)
[0 ms] SQL: UPDATE `characters` SET `honorRankPoints` = 0.0, … WHERE `guid` = 721
Core revision: unknown / 1970-01-01 00:00:00 +0000 / Linux_x64 (little-endian)
Players online: 0. Max online: 0.
```

Those lines are **this install's own configuration**, not this core's nature: `LogFilter_SQLText`
is `0` in `mangosd.conf`, and `DatabaseMysql.cpp:229` prints every statement when it is. One key
turns them off. Leading with them was leading with something we had switched on ourselves.

**With that key set to 1, the reply is this:**

```
Could not open bot log file ../logs/bot_events.csv (No such file or directory). Logging to it is
off for this run.
Core revision: unknown / 1970-01-01 00:00:00 +0000 / Linux_x64 (little-endian)
Players online: 0. Max online: 0.
Server uptime: 29 Minutes 28 Seconds.
```

Still not the command's answer alone — and now the intruding line comes from the playerbots module
rather than the database layer, which is the point stated properly. **The console is a shared
stream.** Quietening one subsystem does not make it a request/response channel; it makes the next
subsystem's output the one you read. An action confirmed by "I saw something in the window" would
be confirmed by whatever the server happened to be saying.

The install was put back to `LogFilter_SQLText = 0` afterwards, so it is as 8.1d and this box
recorded it. The measurement is the finding; changing somebody's logging is not this app's to do
quietly.

## The clause the box turns on, and how it was produced

`prompted=False` means nothing in the window was delimited, and **the transport cannot tell why**:
`docker attach` failing before it reached a console looks the same as a worldserver that is up and
still loading. Either way the command may have been typed and may have run. So it is `unknown`
carrying `indeterminate` — *could not ask* — never `no`, which would invite a caller to send it
again. A `ConsoleError` is the opposite half and deliberately not indeterminate: nothing was typed.

**Producing one honestly took two attempts.** A very short reply window does not do it on this fork:
it prints so much of its own SQL that a prompt from an earlier command is usually already in the
buffer, and a 0.05 s window came back fully delimited. The case the transport's docstring names is
the real one — a world that is UP and has not printed its first prompt yet — so the gate stops the
world, starts it, and asks while it loads. That is exactly what a person hits pressing the button
straight after Start.

## Two wording defects, found by reading a live tab

Neither was caught by a test, because both tests asserted a phrase was *present*.

**The tab said "the command may still have run" twice** — the channel's reason said it and the
label said it again. The channel states what happened; `indeterminate` carries what it implies, and
the wording of the implication belongs to whatever puts it in front of somebody. Fixed at
`e046a28c`, and the test now asserts the **count**.

**Then the two sentences ran together with no full stop.** The reason is written as a clause to
follow "Could not ask:", so the join owns the punctuation. Pinned as well.

## One thing this gate did to itself

The first attempt to build a single log piped every stage through `grep "^\\["`, which drops
tracebacks — so a stage that was failing its assertions read exactly like one that passed. `run.txt`
is unfiltered and carries each stage's exit code.

## The files

* `gate82e.py` — stages `tab`, `probe`, `noprompt`, `surface`.
* `run.txt` — the final pass, unfiltered, with exit codes.
* `1-the-tab.png`, `2-the-probe-answered.png`, `3-no-prompt.png`.

## How to re-run it

```
python gate82e.py  tab | probe | noprompt | surface
```

`noprompt` restarts the world and leaves it loading; the others read and send one `server info`.
