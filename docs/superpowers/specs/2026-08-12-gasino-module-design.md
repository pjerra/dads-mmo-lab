# Gasino Module — design spec

Date: 2026-08-12
Status: approved in brainstorm (this doc is the written record)
Home: private repo `pjerra/mod-gasino` (server Lua + client addon + SQL + tools
versioned together, like mod-city-bots). This spec lives in dads-mmo-lab because
planning happens here; the code does not.

## What it is

An in-game casino for dad's WotLK server: four house games (player vs server)
played through a client addon UI, with all money, odds, and rules held
server-side in an ALE Lua module. Inspired by
[mod-slotmachine-aio-public](https://github.com/kissingers/mod-slotmachine-aio-public),
but deliberately NOT using AIO: ALE has diverged from Eluna and AIO's
addon-message path is the historically broken zone on AzerothCore. Check that
repo's license before porting its payout tables/simulator code; if restrictive,
re-derive our own tables with the simulator (small job).

## Decisions made (brainstorm 2026-08-11/12)

- **House games only** — player vs server. No PvP tables in v1.
- **Real gold** is the stake in v1; DP points as an alternative stake is v2.
- **All four games in v1**: slots, blackjack, dice, deathroll-vs-house, roulette.
- **Entry: both** — Gasino NPC 990000 gossip opens the UI, and `/gasino` works
  anywhere. No proximity requirement.
- **Guardrail: max bet only** — configurable per game. No daily loss cap in v1
  (the ledger is designed so one can be added without schema change).
- **Comms: commands in, addon messages out** (details below).
- **Own private repo** `pjerra/mod-gasino`.

## Architecture

```
mod-gasino/
├── server/                 ALE Lua, deployed to the VM's module lua dir
│   ├── gasino_core.lua     money + session + comms layer (ONLY file touching gold)
│   ├── gasino_slots.lua    per-game rules engines — pure functions + core API
│   ├── gasino_blackjack.lua
│   ├── gasino_dice.lua     dice + deathroll-vs-house
│   └── gasino_roulette.lua
├── addon/Gasino/           client addon, 3.3.5 HD client (C:\wow335ahd)
│   ├── Gasino.toc
│   ├── Core.lua            comms: sends .gasino commands, receives GASINO msgs
│   └── UI/…                lobby frame + one frame per game
├── sql/                    reference DDL for gasino_ledger (table auto-creates)
└── tools/simulator.lua     RTP/odds monte-carlo, runs under plain Lua
```

Division of labor:

- **Addon = dumb terminal.** Renders state, sends intents, holds no money truth.
  Can be modified/deleted/replaced by the client; worst case is placing legal bets.
- **gasino_core = the bank.** Validates bets, moves gold, writes the ledger,
  owns the RNG. Games never call ModifyMoney — only core does, so money
  invariants live in one file.
- **Game files = rules engines.** Given a bet and RNG, produce an outcome.
  Pure enough for the simulator to drive them off-server.

Config (max bets, RTP/payout tables) is a plain Lua table at the top of
`gasino_core.lua`, like the reference module. No conf-file plumbing in v1.

## Comms protocol

**Client → server:** addon sends `.gasino <verb> <args>` via SendChatMessage —
handled by the same `RegisterPlayerEvent(42)` command hook all six dml bridges
use (proven live on this server), but requiring a **non-nil player** (inverse
of dml_whisper's console-only check). Closed verb allowlist:

```
open | balance
bet slots <copper>
bj deal <copper> | bj hit | bj stand | bj double
dice <copper>
deathroll <copper>
roulette <betspec> <copper>
```

Amounts are copper, `^%d+$`-validated server-side. Anything malformed is
silently ignored (the handler sees every command on the server; it must be
paranoid and quiet — same posture as the dml bridges).

**Server → client:** `Player:SendAddonMessage`, prefix `GASINO`, payload =
compact `verb:field,field,…` strings. No serialization library.

**SPIKE GATE (first task, before anything else is built):** confirm ALE exposes
`SendAddonMessage` and the HD client receives it. ~10-line test script on the
VM. Fallback if it fails: tagged system messages (`[GS]…`) the addon parses and
suppresses from chat — uglier, guaranteed to work.

Typing `.gasino` by hand without the addon is not blocked, just unsupported.

## Money invariants

1. **Deduct first, always.** Bet is taken before the RNG rolls. Insufficient
   gold → refuse, nothing rolls.
2. **One payout path.** Wins pay via ModifyMoney; over the gold cap → paid by
   mail instead (ported behavior from the reference module).
3. **Every bet is a ledger row.** `gasino_ledger` (guid, game, bet, payout,
   timestamp, state open/settled) — written at deduct, settled at payout.
   Audit trail, stats source, crash-refund source, and the table a future
   daily-loss-cap reads.
4. **RNG is server-side only.** Seeded once at module load. The client never
   rolls anything.

`gasino_ledger` auto-creates in the **characters DB** at module load
(`CREATE TABLE IF NOT EXISTS`) — gambling history is character data and rides
along in `wow backup` snapshots for free.

## Games

- **Slots** — stateless, one exchange: deduct → 3x3 roll against the ported
  payout table → settle → reply with grid + payout. Reels/paytable from the
  reference module, re-tuned with the simulator.
- **Dice** — stateless: player roll 1-100 vs house roll, higher wins, tie
  pushes (bet returned). Payout slightly under 2x for the edge.
- **Deathroll vs house** — one exchange: server simulates the full roll-down
  (100 → 1-100 → 1-result → … → someone hits 1) and replies with the whole
  sequence so the addon animates it. No open state.
- **Roulette** — stateless. Closed betspec set:
  `red|black|even|odd|1-18|19-36|straight:<n>`. European wheel (single zero),
  standard payouts.
- **Blackjack** — the only stateful game. `bj deal` deducts and opens a hand in
  an in-memory table keyed by player GUID (hand state, bet, ledger row id);
  `hit|stand|double` advance it; dealer stands on 17; settle closes the row.
  v1 rules: no splits, no insurance, double on first two cards only, blackjack
  pays 3:2.

**Open-hand safety:** ledger row stays `open` until settled. Player logout with
an open hand → auto-stand and settle. On module load (= server restart), `open`
rows from before the boot are refunded by mail — nobody loses a bet to a crash.

## Guardrails & errors

- **Max bet** per game, configurable; refused bets get a clear reply, nothing
  deducted.
- **One action in flight per player** (per-GUID busy flag) — command spam while
  a previous action resolves is dropped. Blackjack refuses `deal` while a hand
  is open.
- **Closed-set validation everywhere** — verbs, betspecs, amounts.
- **Every refusal is explicit to the player** via the reply channel; the addon
  surfaces it in the UI.

## Testing

- **The simulator is the test suite for the math.** Monte-carlo each game a few
  million rounds under plain Lua; assert RTP lands in its configured band
  (slots ~0.95; dice/roulette/blackjack at their theoretical edges).
- **Unit asserts in the same runner** for sharp edges: blackjack settle logic
  (3:2, double, dealer-stand-17), roulette betspec parsing, deathroll sequence
  termination.
- **Live gates on the VM, in order:** (1) the SendAddonMessage spike;
  (2) per-game smoke on a throwaway character — bet, win, lose,
  refuse-over-max, blackjack disconnect mid-hand, restart-with-open-hand
  refund.

Constraint that already holds server-wide: `MapUpdate.Threads = 1` while
mod-ale is loaded — the module adds Lua to that same single map thread, so
per-command work must stay small (it is: every game resolves in one exchange
except blackjack, whose steps are trivial).

## Rollout order

Spike → core + slots + minimal addon (lobby + slots frame) → live smoke →
remaining three games → NPC 990000 gossip hook. Playable slots early, not four
games landing at once.

## Out of scope for v1 (recorded so they don't get lost)

DP points as stake; daily loss cap (ledger already supports it); citizens
idle-playing at the casino for atmosphere; PvP tables (family vs family);
stats dashboard / launcher page.
