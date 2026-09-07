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

**Fixed later the same morning.** The verdict now forgets what it knows when the container's
restart count goes BACKWARDS: within one container's life that count only ever grows, so a drop is
a different container wearing the same name and every count remembered about the old one is about
something that no longer exists. `.Id` says the same thing more directly and was written first,
then taken out — no verdict differs between the two, because a container fresh enough to have a new
id has a count of zero, and one whose count has grown past the old one really is looping.

The same press closed the mirror of it, found while writing the tests: a read that FAILED was being
stored as history. `ContainerState()` carries `restart_count=0` — which is also what a container
that has never restarted says — so one unanswered `docker inspect` made the NEXT honest read look
like a count that grew, and a healthy server read as a loop for the ten minutes the settle rule
holds. A failed read now leaves no trace in the history at all.

Two new tests, and five mutations of the fix run against the suite with `__pycache__` purged on
both sides: all five killed, each by the test that claims the behaviour.

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
