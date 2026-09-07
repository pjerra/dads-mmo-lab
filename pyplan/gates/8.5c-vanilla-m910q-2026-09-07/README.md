# 8.5c — Browse Bots, WoW Vanilla — **THE LIVE GATE HAS NOT BEEN RUN**

**Read this line before anything else on this page.** No stage of `gate85c.py`
has been executed and no screenshot exists. There is no `transcript.txt` here
because there is no transcript: the Vanilla stack at `~/vanilla-75b` is stopped,
its three containers no longer exist, and the session that prepared this box was
forbidden to start, stop or reconfigure any server on m910q. **8.5c is not
ticked and must not be ticked from this folder.** What follows is what was
measured with the server DOWN, and what the run still owes.

An earlier lesson on this project is that an incomplete gate capture reads as
fact. This page is the incomplete capture; the banner is its label.

## What this folder holds

| File | What it is |
|---|---|
| `gate85c.py` | the gate, written and syntax-checked, never run |
| `client-who-vanilla.ps1` | 8.5b's client driver with this client's header and a liveness line per shot; never run |
| `README.md` | this page |

## What was measured with the server down, 2026-09-07

Every line here was read off `~/vanilla-75b` over ssh. Nothing was started.

| Fact | Value | Where it was read |
|---|---|---|
| the bot marker in force | `RNDBOT` | `etc/aiplayerbot.conf:57` |
| the bot population configured | Enabled 1, Min 500, Max 500, 100 accounts | `etc/aiplayerbot.conf:9,51,52,58` |
| whether `characters.characters` has a `race` column | **yes**, `tinyint(3) unsigned` | `src/mangos-classic/sql/base/characters.sql:644` |
| why `/who` is faction-filtered here | a config line, not the emulator | `MiscHandler.cpp:159` `if (pl->GetTeam() != team && !allowTwoSideWhoList) continue;` + `etc/mangosd.conf:931 AllowTwoSide.WhoList = 0` |
| the who-list's other filters | GM visibility level 3, plus the client's own level range | `etc/mangosd.conf:1278`, same handler |
| the race → faction map **on this install** | Alliance 1 Human, 3 Dwarf, 4 Night Elf, 7 Gnome, **9 Goblin**; Horde 2 Orc, 5 Undead, 6 Tauren, 8 Troll | parsed from `data/dbc/ChrRaces.dbc` (9 records, 29 fields, 116-byte records), field 8 = TeamID per `DBCStructure.h:190`, decoded 7=ALLIANCE 1=HORDE per `Player.cpp:6269-6272` |
| the four host ports this stack wants | `127.0.0.1:3306`, `0.0.0.0:3724`, `0.0.0.0:8085`, `127.0.0.1:7878` | `docker-compose.yml:46,79,107` and `docker-compose.override.yml:25`, with `.env` setting only `DB_ROOT_PASSWORD` and `DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878` |
| who holds those ports right now | the TBC stack — `tbc-realmd`, `tbc-mangosd`, `tbc-db`, up 3 hours | `ss -lnt` and `docker ps -a` on m910q |
| whether the Vanilla containers still exist | **no** — image and volume are there, the three containers are gone | `docker ps -a` |

Two of those were listed as unmeasurable in the plan and are not: the `race`
column and the port collision are both settled by files on disk, so the plan's
SOAP-console fallback for reading faction is unnecessary and was deleted from
the gate, and the port claim is now read from this install's generated compose
rather than inferred from `.env` plus a shared template's defaults.

The race map is the one that mattered most. It is written into the gate as a
`ChrRaces.dbc` **parse** rather than as a table, because `Player::TeamForRace`
takes TeamID from that file: the map is a property of this install's extracted
client data, not of the expansion. 8.5a's numbers are AzerothCore's — race 11
does not exist in this file at all, which has 9 records — and this file makes
race 9 (Goblin) ALLIANCE, which nobody would have guessed.

## What 8.1c already proved on this exact install, and what it does not

`pyplan/gates/8.1c-vanilla-m910q-2026-09-06/` gated observability here on
2026-09-06 and settles three things this box would otherwise owe:

* the marker reads `RNDBOT` from `etc/aiplayerbot.conf`, live;
* the clause is this tree's one-arm `realmd` clause, with no registry;
* the app's counts equalled the same queries run by hand — `players=0 bots=150`
  at 21:05:18Z, climbing to 500 as the pool filled.

It does **not** prove any of: the list itself (rows, levels, online flags), the
paging, the filter, the summary sentence, the who-list visible effect, or the
warn path *as this list reports it* — 8.1c's warning is `dbreads.population()`'s
(`dbreads.py:280-287`), a different function with a different sentence from
`botlist.page`'s. It also predates the 8.5a code entirely: 8.1c ran at
`d727c065`, when `botlist.py` did not exist.

## The clauses, and where each one stands

| Definition of done (8.5b's, which is 8.5a's minus the type split) | Stage | State |
|---|---|---|
| the total equals the same clause run by hand | `total` | NOT RUN |
| an unreadable marker refuses rather than reporting zero | `refuse` | NOT RUN |
| a readable marker matching nothing while characters exist warns | `warn` | NOT RUN |
| a listed online bot is found by the in-game who-list | `who` + the client driver | NOT RUN |
| (regression) no registry named on a tree that has none | `nosplit` | NOT RUN |
| (usability) paging and the filter | `page` | NOT RUN |
| (this box's own addition) a filtered zero is not blamed on the marker | `filterzero` | NOT RUN |

## Three things the gate was rewritten to avoid, before it ever ran

**A blank-marker stage that could not refuse.** The plan's recipe copied
`~/vanilla-75b/etc` to `/tmp/85c-blank-etc`, which puts the file at
`/tmp/85c-blank-etc/aiplayerbot.conf` while `resolve_marker` opens
`<dir>/etc/aiplayerbot.conf`. A missing file is **not** a refusal here:
`dbreads.py:132-137` falls back to the catalog default, and wow-vanilla's
default is `rndbot` — the live value. The stage would have answered with a
working marker and a plausible total. `fake_conf()` builds the `etc/`
subdirectory, which is `gate81c.py:93-107`'s own shape on this same install.

**A warn stage whose marker never touched the disk.** The clause says a
*readable* marker matching nothing, and a `Marker(prefix='NOSUCHBOT')` built in
Python exercises the same branch `test_a_marker_matching_nothing_while_characters_exist_warns`
already covers in CI. The stage writes `NOSUCHBOT` into a conf copy and reads it
back through `resolve_marker` first, so the resolver, the clause builder, the
live server and the sentence are all in the loop.

**A filter that made the app lie in the same words.** `botlist.page` folds the
name filter into the WHERE the total is counted over, so a filter matching
nothing drove `total == 0` into the marker warning: the tab said *"no character
matched the bot marker 'RNDBOT'"* about a marker that had matched every bot a
keystroke earlier. That is false on its own, and worse in this box — it is the
same sentence the `warn` stage photographs as the whole proof of the deferred
clause, so the evidence could not have identified its own cause. Fixed in
`botlist.py` (`_why_zero`) before the gate was written; `filterzero` is the
stage that shows the two sentences apart on a live server.

`gate85a.py` was **not** copied. It still pages with `browser.page(offset=…)`,
a signature its own box's adversarial review replaced with the `(name, guid)`
cursor; copying it produces a TypeError mid-gate on a box where the server had
to be started for the run. `gate85c.py` follows `gate85b.py`, which is
cursor-aware. Nobody should copy `gate85a.py` again.

## What the run will have to decide first

Starting this stack means **stopping the TBC one**: both default to the same
four host ports and TBC holds all four, which is also the owner's
one-server-at-a-time rule. `docker compose up -d` recreates the three containers
from the image and volume still on the box — no rebuild — and a recreate is the
moment any conf change takes effect, so the marker must be re-read *after* the
stack is up. 8.1c watched the pool climb 150 → 500; the total will move while
the tab is open, which is why every reading in the transcript is timestamped.

The asking character is the other open question. 8.1c's `Asdff` (guid 901,
account `VANGATE`) was last seen 2026-09-06 and 8.3c's `GATE83C` never entered
the world, so there may be no non-bot character here at all. `stage_who` asserts
that with a sentence that says so; making one through the 1.12.1 creation screen
unattended is its own piece of work and is not budgeted anywhere.

## How to run it, once somebody owns those decisions

On m910q, with the tree copied to `~/gate85c` (not `~/dads-mmo-lab`, which is
detached at a commit that predates `botlist.py`):

```
<venv>/bin/python gate85c.py total | refuse | warn | filterzero | nosplit | page | races | who
<venv>/bin/python gate85c.py still-online <name>
```

The genuinely independent hand query — the one that shares nothing with the code
under test — goes in `transcript.txt` and is written with the password in the
ENVIRONMENT, never in argv, because m910q is a shared box:

```
MYSQL_PWD=$(cat ~/vanilla-75b/.db_password) \
  docker exec -e MYSQL_PWD -i vanilla-db mariadb -uroot -N -B \
  -e "SELECT COUNT(*) FROM characters.characters WHERE account IN
      (SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'RNDBOT%');"
```

Then on the Hyper-V host, after copying `client-who-vanilla.ps1` beside 8.5b's
`drive-who-tbc.ps1`, against `C:\clients\WoW-Vanilla-1.12.1\WoW-Client-1.12.1`,
**without** `-RealmStyle` until the 1.12.1 realm screen has been looked at.

## One thing to file rather than lose

The catalog declares `databases.extra: ["logs"]` for wow-vanilla while the
install's volume holds both `logs` and `classiclogs`. Not this box's clause and
not on its path — the bot clause touches only `characters` and `realmd` — but it
is a fact-versus-wiring disagreement of the kind 8.3c found, and it belongs
somewhere other than a subagent's scrollback.
