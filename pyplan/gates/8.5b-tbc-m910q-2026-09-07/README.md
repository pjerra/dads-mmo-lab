# 8.5b — Browse Bots, WoW TBC — live gate

**Where and when.** `m910q`, 2026-09-07 19:34Z – 19:35Z, against the running
native CMaNGOS TBC install at `~/tbc-7.4c` (`tbc-realmd`, `tbc-mangosd`,
`tbc-db` on MariaDB 11.8.9). Code: laptop `6277ad6c` plus this box's own edits,
copied to `~/gate85b` and run under `~/gate81b-venv/bin/python` (3.11.15,
PySide6 6.11.2) — **not** from `~/dads-mmo-lab`, which is detached at a commit
that predates `botlist.py` and would have gated 2026-09-05 code. The who-list
half ran on the Hyper-V host with `C:\clients\WoW-TBC-2.4.3\WoW-Client-2.4.3`,
in the interactive session, using **8.4b's** recipe rather than 8.5a's.

Nothing was started, stopped or reconfigured on the server, and the live
`etc/aiplayerbot.conf` was never edited: the two substituted markers live in
copies under `/tmp` and only the bot-browsing seam is pointed at them.

## What was proved

| Definition of done | Result | Where |
|---|---|---|
| The total equals the same clause run by hand | **900 = 900 = 900**, of 901 characters — the app, the app's own clause run by hand, and a statement typed out with no reference to the code under test | `total`, `1-bots.png` |
| An unreadable marker refuses to answer rather than reporting zero | `total=None`, no rows, and the sentence *"AiPlayerbot.RandomBotAccountPrefix is blank in aiplayerbot.conf…"* — while the live install answered **900 at the same moment** | `refuse`, `2-refused.png` |
| A readable marker matching nothing while characters exist warns, neither reporting zero | *"no character matched the bot marker 'NOSUCHBOTPREFIX', though this server has 901 characters. Page 1, 0 shown."* — the warning leads and no count is printed | `warn`, `3-warned.png` |
| **As 8.5a minus the type split** | `900 bots. Page 1, 50 shown.` and rows reading `Adilad — level 57 — online`: no registry named, no per-row source | `nosplit`, `1-bots.png` |
| A listed online bot is found by the in-game who-list | **`[Ellalanea]: Level 25 Night Elf Druid <Cat Clan> - Wetlands`, "1 player total"** | `6-who-list.png` |
| (paging, which the list needs to be usable at all) | two pages of 50 with no overlap, cursor `('Arlus', 644)`; a three-letter filter returned 3, equal to the whole clause `AND name LIKE 'Arm%'` run by hand | `page`, `4-page-two.png`, `5-filtered.png` |

## What the machine settled that WotLK could not

**The warn path is reachable here, and it was shown on a live server.** 8.5a's
entry defers this clause to "8.5c and 8.5d, where those trees have only the
prefix"; this box turns out to be the first such tree, so **the deferral is
discharged here**. Asked with `NOSUCHBOTPREFIX`, WotLK still returned 1000
because its registry arm answers whether or not the prefix does. On TBC there is
no second arm, the count really is zero, and a hand query for the same marker
agreed: `0`, against `901` characters.

**"Neither reporting zero" was taken at its word, and cost a third edit.** The
first run of the `warn` stage read:

```
0 bots. Page 1, 0 shown. no character matched the bot marker 'NOSUCHBOTPREFIX', …
```

— the number a person reads first was the one the sentence after it exists to
contradict, which is 8.1a's confident-lie shape with the correction stapled to
the end. The tab now leads with the warning and prints no count at all; the
count is dropped rather than moved, because the warning only ever fires on a
zero and there is no other number to lose. This tree is the one the checklist
nominates to settle the clause, so **8.5c and 8.5d inherit this reading**.

**The split reads (0, 900) on every read here, and therefore carries nothing.**
`has_registry('wow-tbc')` is False and the attribution clause is the constant
`1 = 0`. The schema list this install really has was asked for rather than
assumed: `information_schema, characters, logs, realmd, sys, mysql, mangos,
performance_schema` — no `playerbots` anywhere. Before this box the tab read
*"900 bots: 0 by the playerbots registry, 900 by the account prefix"*, naming a
table this install has not got and putting a zero beside it that a person would
go looking for.

**"Minus the type split" was read as "the split must not be drawn", and that is
an interpretation.** The plainest reading of the checklist line is that the
split clause is simply not part of this box's proof; this gate went further and
deleted the split — and the per-row `— prefix` suffix with it — on every tree
that has no registry. It is defensible (naming `playerbots` on an install whose
schema list does not contain it is a lie-shaped output) but it is a choice, and
it is what turned a one-clause-lighter box into a production change. **Vanilla
and Tortoise get that change here**, both being registry-less; 8.5c and 8.5d
therefore gate behaviour that shipped on this box, and their steps should be
written as regressions rather than as new work. WotLK's split is untouched, and
`test_the_bot_list_shows_the_rows_and_the_split_by_signal` is what stops this
box quietly un-shipping 8.5a.

**`1 = 0` is not the Rust regression, and the 900 is what says so.** rust-main
recorded an incident (`botid.rs:178-190`) where the *identity* clause degraded
to `0 = 1` on a registry-less tree and reported a server full of bots as having
none. Here the identity clause has one arm and answers 900; the constant is the
*attribution* clause, which is a different question. 8.5c and 8.5d must
re-measure that rather than inherit it from this page.

## What was measured on the client, rather than inherited from 8.5a

**`/who` on this 2.4.3 CMaNGOS client is NOT filtered by the asker's level.**
The asking character is level 25; both an online Alliance bot at level 25 and
one at level 70 came back, one player each:

```
[Ellalanea]: Level 25 Night Elf Druid <Cat Clan> - Wetlands      1 player total
[Fika]:      Level 70 Gnome Mage - Stormwind City                1 player total
```

The subjects were chosen nearest-level-first for exactly this reason, so that a
zero on the second would have been a level filter and a zero on both something
else. Both rows agree with the database read back afterwards —
`Ellalanea 25 race 4 class 11`, `Fika 70 race 7 class 8` — which is the client's
answer corroborated by a second source rather than read only off a screen.

**"1 player total" is this client's wording too**, but it was read off the
screenshot rather than asserted from 8.5a's 3.3.5a string. Faction was still
respected as the choosing rule: both subjects are Alliance races and the asker
is a Draenei.

Those two chat lines are **not** in `transcript.txt` and cannot be: the client
prints them into its own chat frame and the driver can only photograph the
screen. `6-who-list.png` and `7-who-list-level-70.png` are the primary evidence
for that clause; the database read-back at the end of the transcript is the
corroboration.

**The character was renamed to `Ddsgate`.** `Ddsasd` still carried
`at_login = 1` from 8.4b and cannot leave the character screen until it is
renamed. The row now reads `901  Ddsgate  11  25  0  0  0  12`
(guid, name, race, level, online, at_login, map, zone) — renamed, level 25,
offline, flag consumed. **The next box on this tree must look for `Ddsgate`.**
It appears once in the transcript as `online = 1`, in the `who` stage that ran
about fifty seconds after the client was force-closed: the driver kills the
process rather than logging out, so the world had not yet written the flag back.
The final read-back has it at 0.

**The rename prompt comes up at Enter World on this client**, not at the
character screen, so the name is typed after the first Enter World and a second
one is pressed afterwards. 8.4b photographed that prompt without answering it;
this run answered it (`8-client-in-world-renamed.png`).

## Two of the gate's own mistakes, recorded

1. **`-Who 'Ellalanea,Fika'` arrived as ONE string.** The driver hands its
   arguments through a `.cmd` file (schtasks `/tr` is capped at 261 characters),
   and the unquoted list did not bind as an array: the client was asked
   `/who Ellalanea,Fika` and answered **"0 players total"** — which is exactly
   what a real miss looks like, and would have been written up as "the who-list
   found nothing". The names are split inside `client-who-tbc.ps1` now, and both
   runs are in the transcript.
2. **`ORDER BY ABS(level - 25)` threw.** `characters.level` is UNSIGNED on this
   tree, so subtracting 25 from a level-1 bot gave *"BIGINT UNSIGNED value is out
   of range"*. Cast, not clamped.

And one thing the review asked for that the run confirmed was needed: the two
substituted-marker stages build services against the **real** install and swap
only `services.bots`. `ControllerServices.for_entry(ENTRY, '/tmp/…')` would read
the database password out of `/tmp`, find none, and fail every query — and
`total is None` is true of that failure too, so the refusal would have been
photographed for the wrong reason.

## The screenshots

| File | What it shows |
|---|---|
| `1-bots.png` | the Bots tab on the live install: `900 bots. Page 1, 50 shown.` and fifty rows with no signal named |
| `2-refused.png` | a blank marker: the sentence naming the conf key and the file, and no rows |
| `3-warned.png` | a marker matching nothing: the warning leading, no count, no registry |
| `4-page-two.png` | the next page, read from where the first one ended |
| `5-filtered.png` | a three-letter filter and the three rows it matched |
| `6-who-list.png` | the client in the world, `/who Ellalanea` → the bot's row and "1 player total" |
| `7-who-list-level-70.png` | the same for a level-70 bot, which settles the level-filter question |
| `8-client-in-world-renamed.png` | the character in Elwynn Forest as `Ddsgate`, from the run that renamed it |

## How to re-run it

On `m910q`, with the tree copied to `~/gate85b`:

```
~/gate81b-venv/bin/python gate85b.py  total | refuse | warn | nosplit | page | who
~/gate81b-venv/bin/python gate85b.py  still-online <name>
```

and on the Hyper-V host, after copying `client-who-tbc.ps1` and
`drive-who-tbc.ps1` into `C:\Users\PK`:

```
powershell -File C:\Users\PK\drive-who-tbc.ps1 `
  -ClientDir 'C:\clients\WoW-TBC-2.4.3\WoW-Client-2.4.3' -Realmlist 100.78.24.50 `
  -Account TBCGATE -Password '<the one 8.3a's button set>' -Label who85b2 `
  -Who 'Name1,Name2' -WorldSeconds 30
```

The realm row already reads `100.78.24.50:8085`, written by Yu'lon's own
Networking apply during 8.4b, so that fix does not need repeating.
