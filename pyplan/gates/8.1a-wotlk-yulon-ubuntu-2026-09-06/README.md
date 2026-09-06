# 8.1a — Observability, WoW WotLK — live gate

**Where and when.** `yulon-ubuntu` (Hyper-V, 20 GB RAM), 2026-09-06 19:03Z–19:10Z, against the
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

## What was NOT proved here, and is left open

**The visible in-game effect.** The box asks for the player count to change when a client on the
Hyper-V host logs in and out. That needs a real 3.3.5a client driven through a login, and it did not
happen in this run. Everything above is measured; this line is not, and the box stays unticked
until it is.

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

The stack is up (`ac-database` healthy, `ac-authserver` and `ac-worldserver` running), the install
untouched apart from the one snapshot in `~/.local/share/yulon/logs/`, and no checkpoint was taken
or restored.
