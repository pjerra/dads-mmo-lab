# 8.1a — Observability, WoW WotLK — live gate

**Where and when.** `yulon-ubuntu` (Hyper-V, 20 GB RAM), 2026-09-06 19:03Z–19:30Z, against the
**finished 7.2 install** at `/home/pk/wowserver`, **no checkpoint restored**. Docker 29.7.2,
AzerothCore + mod-playerbots, `Min = Max = 500` bots. Every action was announced on that box's
activity terminal before it ran. Code at `a46c93ed`, checked out into its own worktree
`~/gate81a`; the runner is `gate81a.py`, committed beside this file.

The transcript is `transcript.txt`. What follows says what each stage asked, what came back, and —
for the two cases where the machine contradicted a document — what changed as a result.

## What was proved

| Definition of done | Stage | Result |
|---|---|---|
| The tab's player and bot counts equal the same queries run by hand, in the same minute | A | **equal** — app `players=0 bots=500`, by hand `players=0 bots=500`, both at 19:03:45Z |
| A crash-looping server reads as a restart loop within two polls, and the verdict later command buttons key off reports unstable | D2 | **first poll**, and every poll after: `restart_loop`, `stable=False`, count climbing 6 → 10 |
| Every stop leaves a log file whose contents match what the Console tab was showing | E | the last five console lines are **byte-identical** inside the saved file; 2000 lines, 305 250 bytes |
| A snapshot that fails is reported and the stop still happens | F | `problem=…File exists: '/tmp/gate81a-unwritable'`, `stop() -> True`, container `running` → `exited` |
| With the bot marker blanked, the tab refuses rather than reporting zero | B | **refused**, naming the key and saying an empty prefix would report every player as a bot |
| With the marker readable but matching nothing while characters exist, it warns | B2 | `players=500 bots=0` **with** `warning="no account matched the bot marker 'NOSUCHBOTPREFIX', though this server has 1000 characters"` |
| The first poll is not five seconds late (`bug-checklist.md:552`) | C | at construction, with nothing else called: `status: db up, auth up, world up`, the verdict line already there, Start disabled and Stop enabled |
| **Visible effect:** the player count changes when a client logs in and out | login | **0 → 1 → 0 → 1 → 0** across 19:26–19:30Z, bot count steady at 500, and the server's own rows agree |

## The visible in-game effect — done 19:26–19:30Z, by the owner at the client

The box asks for the player count to change when a real client logs in and out. It did, twice, and
the dashboard was polling every three seconds the whole time with nothing else driving it
(`watch-during-the-login.txt`, every line a CHANGE):

```
[19:18:04Z] up — 0 players, 500 bots, up 6m      <- staged, waiting
[19:26:48Z] up — 1 players, 500 bots, up 15m     <- the character entered the world
[19:28:07Z] up — 0 players, 500 bots, up 17m     <- back to character select
[19:28:57Z] up — 1 players, 500 bots, up 17m     <- entered again
[19:29:47Z] up — 0 players, 500 bots, up 18m     <- logged out and quit
```

**The bot count never moved.** 500 before, during and after — so the person was counted as a
person, which is the half of the marker that the fabricated cases in B and B2 cannot demonstrate:
this is a real account with no registry row and no `RNDBOT` prefix, and the clause left it out of
the bot arm on its own.

The witnesses, and they agree:

| | |
|---|---|
| the dashboard | the five transitions above |
| the auth database, while online | `105  YULONGATE  last_login 2026-09-06 19:28:53  online=1  failed_logins=0` |
| the characters table, while online | `guid 1001  "Asfgg"  level 1  online=1  account=105` |
| the player clause, run by hand at that moment | `1` |
| both rows after the logout | `online=0` on the account and on the character |

`failed_logins=0` — first attempt. The account was made through
`ControllerServices.create_account`, the Server tab's own seam, not a console.

**The route**, which is the one 7.1's second login proved (`pyplan/gates/7.1-client-login/
LOGIN-2026-09-05.md`): an ssh tunnel from the laptop, `-L 3724` and `-L 8085`, with the realm row
set to `127.0.0.1` for the duration and the client's `Data/enUS/realmlist.wtf` pointed at the same.
Both were **put back afterwards** — the realm row to `172.30.55.119` and the client to
`100.71.125.58` — and the account row is left in place as evidence.

## Two things the machine contradicted, and both changed the code or the plan

**1. A missing module conf does not mean "unknown" — it means the compiled default is in force.**
`dbreads.resolve_marker()` refused outright on this install, because
`env/dist/etc/modules/playerbots.conf` is not there. The catalog's path was right and the reasoning
was wrong, and the worldserver says both in its own words:

```
> Config::LoadFile: Failed open file '/azerothcore/env/dist/etc/modules/playerbots.conf'
> Not found modules config files
```

An ordinary install ships `playerbots.conf.dist` and no `playerbots.conf`; the `.dist` is **not**
loaded; the module runs on its compiled defaults. Refusing there would have left the tab with no
counts at all on the very install this box gates. Fixed at `a46c93ed`: absent → the default, with
`source="default"`; present-but-unreadable → still refused, because a file whose contents are
unknown says nothing.

**2. `docker kill` cannot produce a restart loop, so the checklist's recipe does not work.**
The box asks for "a forced kill of the world twice in a row". Measured: `docker kill` sets Docker's
own *explicitly stopped* flag, so a container carrying `restart: unless-stopped`
(`docker-compose.yml:67`, `:212`, `:241`) stays `exited` with `RestartCount 0` — and the dashboard
correctly answered `stopped`, twice. The recipe asks for a state it cannot create.

What does loop is the world failing on its own, which this box demonstrated unprompted when the VM
booted: `RestartCount 6` while the database was still starting. Stage D2 reproduces that
deliberately — stop the database, start the world — and the verdict was `restart_loop` on the
**first** poll, because the daemon itself reports `restarting`.

## A note on the counts that are not zero by accident

Stage B set the account prefix to something no account matches and the bot count still came back
500: the **registry arm** found them. That is the two-arm clause behaving exactly as the 2026-08-01
incident requires, and it means the "matched nothing" state cannot be reached on this install by
editing the prefix. Stage B2 therefore fabricates the one thing the other three trees really have —
a marker with **no registry** — and points it at this live database. The clause, the query and the
1000 rows are real; only the absent registry is arranged, and it is arranged to equal what a CMaNGOS
install carries.

With that shape, the app reported `players=500, bots=0` — every bot counted as a person, which is
the incident itself — **and said so**, instead of printing two numbers with nothing wrong on the
face of them.

## The box was left as it was found

The stack is up (`ac-database` healthy, `ac-authserver` and `ac-worldserver` running) and no
checkpoint was taken or restored. Four things on it are not as they were, all of them deliberate and
all of them named: the one snapshot in `~/.local/share/yulon/logs/`, the `YULONGATE` account and its
level-1 character (kept as the login's evidence), and `RestartCount` on the worldserver, which stage
D2 drove from 6 to 10 on purpose. The realm address and the client's `realmlist.wtf` were both put
back.
