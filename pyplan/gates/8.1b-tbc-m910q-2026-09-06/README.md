# 8.1b — Observability, WoW TBC — live gate

**Where and when.** `m910q`, 2026-09-06 19:59Z–20:12Z, against the TBC install at `~/tbc-7.4c`
(the one 7.4c built). Vanilla holds the same ports and was stopped for the duration and started
again afterwards; every action was announced on that box's activity terminal first. Code at
`769f56e8`, in its own worktree `~/gate81b`, under a **Python 3.11** venv built for the run — the
box's existing venvs are 3.10 and this project targets 3.11.

The runner is `gate81b.py`, committed beside this file. It is deliberately **not** 8.1a's runner:
every fact it touches is this tree's own — `characters.characters`, `realmd.account`, a bot marker
with no registry, and a generated database password that lives in a file under the server dir
rather than being a constant.

## What was proved

| Definition of done | Result |
|---|---|
| The counts equal the same queries run by hand, on this tree's own tables | **equal** — app `players=0 bots=500`, by hand `players=0 bots=500`, both at 19:59:37Z |
| The marker is resolved from this install's own configuration | `Marker(prefix='RNDBOT', source='conf')`, read from `etc/aiplayerbot.conf:57` |
| The clause is this tree's, with no registry arm | `account IN (SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'RNDBOT%')` — one arm, `realmd`, no `playerbots_account_type` |
| A blanked marker refuses rather than reporting zero | refused, naming the key and the file: "…is blank in aiplayerbot.conf" |
| A marker matching nothing warns while characters exist | `players=500 bots=0 characters=900` **with** the warning — and on this tree it needed **no fabrication**, see below |
| Every stop leaves a log whose contents match the Console tab | the last five console lines byte-identical inside the saved file; snapshot at 21:59:57 local, stop finished 22:00:01 |
| A snapshot that fails is reported and the stop still happens | `problem=…File exists: '/tmp/gate81b-unwritable'`, `stop() -> True`, `running` → `exited` |
| A crash-looping world reads as a loop | `restart_loop`, `stable=False`, count climbing 7 → 10 across six polls |
| The first poll is not late | at construction: `status: db up, auth up, world up` with the verdict line already filled |

## Not proved here

**The visible in-game effect.** The box asks for the player count to move when a client logs in and
out of *this* server. That needs a 2.4.3 client on a machine with a screen; the only client on the
laptop is 3.3.5a, and `m910q` — which has 2.4.3 under `~/clients` — is headless. Named rather than
skipped; the box stays unticked until it is done.

## Where this tree differs from WotLK, measured rather than assumed

**The marker comes from a file here, and from the compiled default there.** An AzerothCore install
has no `playerbots.conf` at all (8.1a's gate measured that, and the server says so in its log). A
CMaNGOS install *does* have `etc/aiplayerbot.conf` — Yu'lon materialises it out of the image and
patches it — with the key live at column 0. Same question, two routes, and `resolve_marker` reports
which one answered.

**The "matched nothing" state is reachable with the real configuration.** On WotLK the registry arm
found all 500 bots even with a nonsense prefix, so that gate had to fabricate a registry-less marker
to exercise the warning. This tree *has* no registry, so setting the prefix to something no account
matches is enough: `players=500, bots=0` on 900 characters, with the warning naming the marker. The
2026-08-01 incident, reproduced on a real database with nothing arranged.

## The two things the machine refuted, both of which changed the code

**1. `stable` said yes about a server nobody could play on.** Take the database away from an
AzerothCore worldserver and it exits; the daemon restarts it and the verdict reads `restart_loop`.
Take it away from a CMaNGOS one **that has already connected** and the process stays up, retrying:
six polls over a minute, every one `up` with `RestartCount 0` and `stable=True`. That is the value
8.2a's command interlock keys off.

`stable` now also requires that the read answered, through a new `database_unreachable` field that
is set **only** when the query failed — never when the bot marker was the problem, because refusing
every command over a blank conf key would be the mirror of the same bug. There is a test for each
direction.

Both behaviours are real and they are different cases, so both are recorded:

| | AzerothCore | CMaNGOS TBC |
|---|---|---|
| database removed from a **connected** world | exits → restarts → `restart_loop` | stays `running`, retries → `up`, and now `stable=False` |
| world **started** with no database | `restart_loop` | `restart_loop` — measured, 7 → 10 restarts |

**2. `--tail 2000` bounds entries, not lines.** The saved snapshot holds **3114** newline-terminated
lines and no carriage returns: the json-file driver stores one entry per write, and a server that
writes a multi-line message in one call spends one entry on several lines. `LOG_TAIL_LINES` claimed
"lines"; it now says what it does, with this measurement beside it. The byte cap is what actually
bounds the file, and it did — 178 886 bytes.

## One thing this design cannot do, stated rather than left to be discovered

**A pre-stop snapshot can never contain the server's own shutdown lines.** It is taken before the
stop, by construction — because `remove()` and the uninstall destroy the container, and after them
there is nothing left to read. On this tree that matters more than on WotLK: TBC's shutdown is not
graceful. Captured from the container after the stop:

```
Object::~Object (GUID: 181646 TypeId: 5) deleted but still in world!!
Critical Error: A condition which must never be false was found to be false.
Server was shut down to protect data integrity.
~Object(): false
```

That is exit 139, and it is a **pre-existing** CMaNGOS shutdown assertion this box already had on
record — not something 8.1b introduced. But it means the one log a user would most want after a bad
stop is the one the snapshot cannot hold. Named here as an open question for 8.9, where the
uninstall path decides what to keep, rather than quietly left out.

## The box afterwards

TBC stopped with its containers kept, Vanilla started again and healthy — the state it was in
before this run. One snapshot in `~/.local/share/yulon/logs/`. Nothing else on the box changed.
