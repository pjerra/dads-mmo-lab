# 8.1c — Observability, WoW Vanilla — live gate

**Where and when.** `m910q`, 2026-09-06 21:05Z–21:17Z, against the Vanilla install at
`~/vanilla-75b`. Code at `d727c065`, in its own worktree `~/gate81c`, under the Python 3.11 venv
built for 8.1b. Every action was announced on that box's activity terminal first. The server was
**started for this gate and stopped again afterwards** — the owner's rule of 2026-09-06 is that
m910q runs no server it is not currently testing.

Owner answer 12 said to gate this tree "against the server already running on m910q". What that
answer bought was avoiding a **recompile**, and it still holds: the install was there and only its
containers needed starting.

## What was proved

| Definition of done | Result |
|---|---|
| The counts equal the same queries run by hand, on this tree's own tables | **equal** — app `players=0 bots=150`, by hand `players=0 bots=150`, both at 21:05:18Z, while the bots were still logging in |
| The marker is resolved from this install's own configuration | `Marker(prefix='RNDBOT', source='conf')` from `etc/aiplayerbot.conf:57` |
| The clause is this tree's | `account IN (SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'RNDBOT%')` — one arm, `realmd`, no registry |
| A blanked marker refuses rather than reporting zero | refused, naming the key and the file |
| A marker matching nothing warns while characters exist | `players=230 bots=0 characters=900` with the warning |
| Every stop leaves a log whose contents match the Console tab | console tail byte-identical inside the saved file; 137 562 bytes |
| A snapshot that fails is reported and the stop still happens | `problem=…File exists`, `stop() -> True`, `running` → `exited` |
| A crash-looping world reads as a loop | `restart_loop`, `stable=False`, 7 → 10 restarts across six polls |
| The first poll is not late | at construction: `status: db up, auth up, world up` with the verdict line already filled |
| **Visible effect:** the player count moves when a client logs in and out | **0 → 1 → 0** across 21:16–21:17Z, against a full 500 bots |

## The visible in-game effect

The owner's 1.12.1 client, at `C:\Users\perzi\Desktop\dadmmolab\Clients\WoW-Client-1.12.1`. No
tunnel: m910q is on Tailscale, so the realm advertised `100.78.24.50` for the duration and
`192.168.10.134` was restored afterwards.

```
[21:16:11Z] up — 0 players, 500 bots, up 3m
[21:16:25Z] up — 1 players, 500 bots, up 3m      <- the character entered the world
[21:17:21Z] up — 0 players, 500 bots, up 4m      <- and left
```

The server's own rows, read while the character was in the world:

| | |
|---|---|
| `realmd.account` | `105  VANGATE  gmlevel 0` — created through the Server tab's own seam |
| `characters.characters` | `901  "Asdff"  level 1  online=1  account=105` |
| the player clause by hand | `1`, with `online total` **501** |
| both after the logout | `online=0`, players `0` |

`501 = 1 player + 500 bots`, and the clause put the character on the right side of that split on a
tree whose **only** marker is the account-name prefix — there is no registry to fall back on here.

**This client keeps its `realmlist.wtf` in the client ROOT**, not under `Data/<locale>/` as the 2.4.3
and 3.3.5a clients do. It read `100.101.205.6` before this run and was put back.

## What is this tree's own, measured rather than inherited

Vanilla's facts were read from **Vanilla's own section** of `pyplan/phase8-reads/cmangos.md` — which
says in its own words that bot accounts here are marked only by the `account.username` prefix, and
names `characters` and `realmd` on its own citations — and then checked against the install before
the catalog block was written.

**Where it differs from TBC, which is the tree next door:**

| | TBC | Vanilla |
|---|---|---|
| shutdown | asserts and dies: `~Object(): false`, exit 139 | **halts gracefully**: `Halting process...` |
| snapshot line count | 3114 lines from `--tail 2000` | 4228 lines from the same cap |

The halt line is the one 8.1b could not record, because that tree crashes instead of printing one.
Here it is present — and note *where* it comes from: the snapshot holds the tail of the container's
whole log across restarts, so those two `Halting process...` lines are from an **earlier** run's
shutdown. A pre-stop snapshot still cannot contain the stop it precedes. That limit is unchanged and
is stated in 8.1b's page.

## The box afterwards

Every server on m910q is stopped — `vanilla-db`, `vanilla-realmd`, `vanilla-mangosd` and the TBC
three — with their containers kept, so the next start is staged. The only thing running is `r6`,
which is not a game server. The realm address is back to `192.168.10.134`, and the `VANGATE` account
and its character are left in place as this run's evidence.
