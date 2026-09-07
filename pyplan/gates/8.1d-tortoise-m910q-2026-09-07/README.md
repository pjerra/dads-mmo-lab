# 8.1d — Observability, WoW Tortoise — live gate, server side

**Where and when.** `m910q`, 2026-09-07 02:03Z – 02:24Z, against the Tortoise install at
`~/tortoise-server`, code at `2a3e6990` in its own worktree `~/gate81d`, under the Python 3.11 venv
built for 8.1b. Every action was announced on that box's activity terminal first.

**The install finished during this run.** It had been building since the previous session: the
`mmaps` stage ran 3 h 07 m of `MoveMapGen` across 40 maps, then `conf`, `import` and `up`. The world
then took **17 minutes 46 seconds** to load, most of it one-time work — `ai_playerbot_equip_cache`
is built class by class, spec by spec, level by level, and this fork logs every INSERT.

**This box is NOT ticked.** One clause is unproved: *"the player count on the tab moves when a
client logs in and out of this server."* A fresh install has no characters, and creating one needs
somebody at the character-creation screen — see "What is left" below.

## What was proved

| Definition of done (as 8.1b) | Result |
|---|---|
| The counts equal the same queries run by hand, on this tree's own tables | **equal** — `players=0 bots=510`, from the app and by hand, with a third reading taken after to show the population had not moved between them |
| The marker is resolved from this install's own configuration | `Marker(prefix='RNDBOT', source='conf')`, from `etc/aiplayerbot.conf:` `AiPlayerbot.RandomBotAccountPrefix = RNDBOT` |
| The clause is this tree's | `account IN (SELECT id FROM tw_logon.account WHERE UPPER(username) LIKE 'RNDBOT%')` — one arm, `tw_logon`, no registry |
| A blanked marker refuses rather than reporting zero | refused, naming the key and the file |
| A marker matching nothing warns while characters exist | `players=510 bots=0 characters=900` with the warning — the 510 are the bots, correctly counted as people by a marker that no longer knows them, and said so |
| The first poll is not late | at construction: `status: db up, auth up, world up` with `up — 0 players, 510 bots, up 19m` already in the verdict line |
| Every stop leaves a log whose contents match the Console tab | 227 061 bytes, 2000 lines, the console tail byte-identically inside it |
| A snapshot that fails is reported and the stop still happens | `problem=…File exists`, `stop() -> True`, `running` → `exited` |
| A crash-looping world reads as a loop | `restart_loop`, `stable=False`, **7 → 10 restarts** across seven polls |
| **Visible effect:** the player count moves when a client logs in and out | **not proved** — see below |

## Two per-tree facts, and one of them contradicts its sibling

**This fork dies when its database goes, and CMaNGOS proper does not.** 8.1b measured the opposite
on TBC — `mangosd` stays up retrying where AzerothCore's worldserver exits, which is what made
`stable` lie about a world with no database. Tortoise behaves like AzerothCore here: taking the
database away put the container into a real restart loop, 7 → 10 restarts, and the verdict said so.
Same family, opposite behaviour; the box's own section forbids inheriting anything, and this is why.

**The marker comes from a conf file this tree actually has.** `source='conf'` here, where WotLK's is
`source='default'` because that core loads no module conf at all. Same key, same catalog block,
different provenance — and the tab is right on both.

## The client half, and where it stopped

The owner's own Turtle client, on the owner's own laptop
(`Desktop\dadmmolab\Clients\1.18.1-7272-Hotfix-2026-04-12`), against
`100.78.24.50` — the realm address was pointed at this box **through Yu'lon's own Networking
plan/apply**, and the gate account was made **through Yu'lon's own create-account seam**.

**An account created by writing `sha_pass_hash` is refused on its FIRST login and accepted on the
second.** Screenshots `3-` and `4-`. The row this app writes is correct — its
`sha_pass_hash` equals `SHA1('TORTGATE:T0RT-G@TE12')` — but this fork's authserver wants SRP6
`v`/`s`, which the row does not carry, and it derives and stores them during the attempt it
refuses. After that the same credential works. The bot accounts the server made itself have
`v`/`s` NULL, so they are on the same path.

That is a real wart for anyone making an account through Yu'lon on this tree: it says "The
information you have entered is not valid" once, and then it works. Writing `v`/`s` at creation
would remove it; that belongs to 8.3d, not here.

## One more thing the watcher found, and it is a defect

**A `Dashboard` that has seen a restart loop keeps calling it one after the container is
recreated.** The watcher left running across the crash-loop check went on printing

```
restart loop — 0 restarts, this run up 3m
```

for minutes after the world was healthy — zero restarts, and still a loop. A dashboard built fresh
at the same moment read `up — 0 players, 510 bots`. So the verdict is being carried by history the
object holds rather than by what the container says now, and a recreate resets the count under it.

On the tab that means a server that HAD a restart loop and has been fixed keeps reading as broken
until the app is restarted — and 8.2a's enable button, which is interlocked on `stable`, stays
disabled with it. Not fixed here; it belongs to `dashboard.py` and wants a test that recreates the
container under a live watcher.

**Fixed the same morning, and the first fix was wrong.** The verdict now stops calling a restarted
container a loop — a count of zero cannot be one, and the sentence above was simply false. But the
first version went further and handed back `stable` as well, on the argument that a restart count
which has gone BACKWARDS means a different container. An adversarial review refuted it using
evidence this very run had already produced and I had read past: compose said **`Container
tortoise-mangosd Started`**, not `Recreated`, and `RestartCount` still went 8 → 0. **A manual start
resets docker's count exactly as a recreate does.** So pressing Start on a server whose crash cause
was still there would have been answered `stable=True` for as long as its world takes to load and
die again — minutes on these trees. That is the bug 8.1a exists to close, arriving from the other
side.

The label and the interlock are two questions, so they are two values. `Verdict.after_a_loop` says
this run followed a loop and has not yet outlasted `SETTLED_AFTER`; `stable` is False while it is
set, and the line says why:

```
up — 0 players, 0 bots, up 30s · restarted after a crash loop — not called steady until this run has lasted 10m
```

Ten minutes of the run that is actually going is the same evidence the settle rule always asked
for, measured against the right run. `.Id` was written first and taken out: identity would name a
manual restart and a recreate apart, and **neither is evidence of health**, so nothing downstream
could act on the difference.

The same press closed the mirror of the defect, found while writing the tests: a read that FAILED
was being stored as history. `ContainerState()` carries `restart_count=0` — which is also what a
container that has never restarted says — so one unanswered `docker inspect` made the NEXT honest
read look like a count that grew, and a healthy server read as a loop for ten minutes. A failed
read now leaves no trace in the history at all.

Nine mutations, `__pycache__` purged on both sides of each: all nine killed, each by the test that
claims the behaviour.

**Proved back on this box, against a stand-in and against the subject** — `fix-transcript.txt`,
with both probes beside it. `dashprobe.py` puts a busybox container into a real restart loop and
replaces it; the unfixed code answers `restart loop — 0 restarts, this run up 5s` while a dashboard
made fresh at the same moment answers `up`, which is this README's own sentence produced on demand
in forty seconds. `dash81d.py` then does it to `tortoise-mangosd` itself by 8.1d's own recipe — the
database taken away under the world, the world put back with the ordinary `docker compose up -d`,
and then **ten real minutes of waiting** to watch the interlock open on its own rather than
asserting it from the rule. Screenshots `5-`, `6-` and `7-` are the Server tab in all three states.

## What is left

The account has no character, and this server was installed today, so nothing on it does. Character
creation needs the creation screen: three clicks computed from the client window's own rectangle
were tried and the third one logged the client out instead, and the coordinates for this client's UI
have not been measured. **The remaining clause is two minutes of somebody's time:**

```
account  TORTGATE
password t0rt-g@te12
realm    100.78.24.50   (already written into the client's realmlist.wtf)
```

Log in, make any character, enter the world, wait, and log out. The watcher is already running on
`m910q` and writes every CHANGE in what the tab would show to `~/gate81d-watch.log`:

```
ssh m910q 'cat ~/gate81d-watch2.log'
```

## How to re-run it

```
python gate81d.py  a | b | c | d | e | f | watch
```

`d` takes the database away and puts it back; `e` and `f` stop the world. Start it again with
`docker compose up -d` in `~/tortoise-server` — after the first load the equip cache is built and it
comes up in a few minutes rather than eighteen.
