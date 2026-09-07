# 8.2a — Command channel, WoW WotLK — live gate

**Where and when.** `yulon-ubuntu`, 2026-09-06 23:20Z – 2026-09-07 00:25Z, against the finished 7.2
WotLK install at `~/wowserver`, **no checkpoint restored**. Code in `~/gate81a`, moved forward
through the run as the gate found things: `eac5dbe7` for the first half, `2f3b3cf9` for the port
clause, `54f01161` for the final reading. Every action was announced on that box's activity
terminal first.

The whole run is in `transcript.txt`. The screenshots are the Server tab, rendered offscreen from
the real `ControllerView` over the real install.

## What was proved

| Definition of done | Result |
|---|---|
| One press writes the configuration and the tab says it will be checked at the next start | `changed=True`, four keys in the override; a second press `changed=False` |
| The press refuses while the world is running | refused, naming what to do; the override byte-identical afterwards |
| The user's ordinary Start brings the world up through the staged start | `start_staged()`, unchanged from Phase 7 |
| After it the world container is running, **its restart count is unchanged**, and **this run's ready marker is in the log** | `status=running restarts=0 started=2026-09-06T23:26:29Z`; `…(Playerbot branch) (Unix, RelWithDebInfo, Static) ready...` found with `--since` that same timestamp |
| **The tab reads verified with the time** | `Command channel: verified as YULON_243C46E3 at 2026-09-07 00:15 UTC.` — screenshot `4-repaired.png` |
| **The account row and its access row exist at level 3** | `account` → `106 YULON_243C46E3`; `account_access` → `106  3  -1` |
| The round-trip reply text is in the capture, **taken from the host** | `answered`, through the published loopback port, carrying `AzerothCore rev. 413bea61a85e+ … Connected players: 0. Characters in world: 500.` |
| Nothing is listening on 8888 or 3443 inside the container, read from `/proc/net/tcp` | **8888 was LISTENING before the press** and is silent after; 7878 went silent → LISTENING; 3443 silent throughout. The final reading, at 00:22:42Z: `[7878, 8085, 34163]` |
| **A deliberately wrong credential file reads as refused and the repair path returns it to verified without creating a second account** | `check()` → `Refused`, screenshot `3-refused.png`; `repair()` → `Verified`; accounts matching the app's prefix: **1 before, 1 after** |
| **A deliberately occupied port leaves the configuration rolled back and the server startable** | the tab's own sentence, screenshot `5-rolled-back.png`; `AC_SOAP_ENABLED` gone from the override, `DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:0` in `.env`; the server then started **with the port still held** |

## The screenshots

| File | What it shows |
|---|---|
| `1-before.png` | the tab before anything was pressed |
| `2-after.png` | after the press, the start, and the first verification |
| `3-refused.png` | a credential the server refuses: the sentence, and the Repair button that appears only here |
| `4-repaired.png` | verified again, **with the time**, after a repair that created no second account |
| `5-rolled-back.png` | what the tab says when the port is taken: the channel turned off again, the port given back |
| `6-after.png` | the tab as the gate leaves it |

## What the machine refuted

**1. The daemon's words for a taken port were not the words the predicate knew.**
`blames_the_host_port()` was written against Docker's older sentence, `Bind for 127.0.0.1:7878
failed: port is already allocated`. At 00:16:25Z this daemon said:

```
failed to bind host port 127.0.0.1:7878/tcp: address already in use
```

Neither `bind for` nor `already allocated` is in it. The predicate answered no, the rollback never
ran, and the gate failed on the one clause it was written to prove. Fixed at `2f3b3cf9`; both
sentences are now in the test, the measured one with its date.

**2. The tab said "verified" about a credential the server was refusing.**
The state is read off the credential file — which is right, because that file IS the record that a
round trip once answered — but nothing asked the server, so a credential that had gone stale kept
reading as verified until the next Start, and the repair the user needs was never offered. Found by
breaking a credential on purpose and watching the tab claim it was fine. Fixed at `ab631d36`: the
tab asks once when it opens, off the GUI thread, with `check()` and never `settle()` — opening a tab
is not permission to write a row into somebody's auth database.

**3. The server escapes its own output, and nothing was undoing it.**
The first successful round trip came back with every line ending `&#xD;`. That text goes into this
capture and, later, into the console this channel will carry. Fixed at `54f01161`: XML's five named
entities and numeric character references, and nothing else.

**4. A failed bind leaves a container a later `compose up` will start rather than recreate.**
Not a defect in this step, and worth writing down. When the daemon refuses to publish the port, the
world container is created with its networking half-configured and compose reports the error. If the
configuration is then unchanged, the next `compose up` **starts that same container** rather than
recreating it: it comes up with no network at all, cannot resolve `ac-database`, and exits 1 into a
restart loop — which is what 8.1a's verdict is for, and what it reported. The rollback avoids this by
construction, because it changes both the override and `.env`, so compose recreates.

## The account, and a wildcard that would have made the count lie

The app's prefix is `YULON_`, and `_` is a single-character wildcard in SQL `LIKE`. On this box
`LIKE 'YULON%'` matches three rows — `YULON` and `YULONGATE` are left over from the 7.2 account
gate — and `LIKE 'YULON_%'` matches `YULONGATE` too. Counted as `LEFT(username, 6) = 'YULON_'`
instead, so "no second account was created" is measured rather than true by accident. `rust-main`
records the same trap against `DMLSOAP\_%`.

Two other per-tree facts, read on the box rather than inherited: this install's `account_access` is
keyed `id`, not `AccountID`; and its auth schema is reached through the seam's logical name `auth`,
which resolves to `acore_auth` here.

## How to re-run it

`gate82a.py` is the first half (ports, the press, the first verification); `gate82a_rest.py` is the
five clauses above. Both take a stage name:

```
python gate82a.py       ports | before | press | prove | after
python gate82a_rest.py  rows  | marker | break | repair | port | after
```

`port` stops the server, puts the override back to channel-off, presses, holds `127.0.0.1:7878` from
inside the script, starts, and puts the install back the way the user would want it afterwards.
