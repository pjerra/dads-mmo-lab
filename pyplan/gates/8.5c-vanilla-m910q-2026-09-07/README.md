# 8.5c — Browse Bots, WoW Vanilla — **the server-side half PASSED, live**

Run on m910q against `~/vanilla-75b` (CMaNGOS classic) on 2026-09-07 between
**20:59:53Z and 21:05:44Z**, with the stack up nine minutes and its bot pool
already full. Every number on this page has the clock beside it, because the
pool is written by the module at runtime and a reading without a time is not a
measurement.

Clauses 1, 2 and 3 of the definition of done — checklist.md:2498, *"as 8.5b"* —
**passed**. Clause 4 is the client lane's and is **not claimed here**; what this
page owes it is the asking character, the subjects, and the reason the faction
matters, and those are in *"The handover to the client lane"* below.

The previous version of this page carried a banner saying the gate had never
been run. It has now been run, twice end to end; `transcript.txt` is the second
run, whole, from one shell.

## What this folder holds

| File | What it is |
|---|---|
| `gate85c.py` | the gate, run |
| `transcript.txt` | one continuous run, 21:05:07Z–21:05:18Z, header to last stage |
| `1-bots.png` | the Bots tab: `900 bots. Page 1, 50 shown.` — no registry named |
| `2-refused.png` | the blank marker refusing, naming the key and the file |
| `3-warned.png` | a readable marker matching nothing, warning and printing no zero |
| `4-page-two.png` | page 2, `Previous` now live — and `Ashah — level 3 — online` on it |
| `5-filtered.png` | the filter `Ara` → `4 bots. Page 1, 4 shown.` |
| `6-filter-found-nothing.png` | a filtered zero, in **different words** from `3-warned.png` |
| `network-fix.py` | the press that pointed the realm row at a reachable address, through Yu'lon's own Networking feature |
| `network-fix.txt` | that press's report, plus the row and the reachability re-read fresh afterwards from both boxes |
| `client-who-vanilla.ps1` | the client driver — **still never run**, and see the warning about it below |

## The clauses, and what each one actually read

Every stage prints the state *before* its command as well as after, because a
step whose assertion was already true proves nothing — 8.4a's own defect.

### (1) the total equals the same clause run by hand — PASSED, 21:05:08Z

Three readings, and the independent one was typed at a shell before the app was
started (transcript header, 21:05:07Z, password in the environment and never in
argv):

| Reading | Value |
|---|---|
| the Bots tab | `total=900  by_registry=0  by_prefix=900  rows=50` |
| the app's own clause, run by hand | `900` |
| a statement written independently of the app | `900` |
| characters on this server altogether | `901` |
| online at that moment | `500` |

900 ≠ 0 and 900 ≠ 901, so the equality is a constraint and not a tautology. The
identity clause has one arm:
`account IN (SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'RNDBOT%')`.

**900 bot characters, 500 of them online.** `Min = Max = 500` in
`etc/aiplayerbot.conf:51-52` is a number of *sessions*, not of characters: the
module keeps 500 logged in out of a pool of 900 that it has made over this
install's life. The online count read 500 at 20:59:53Z and 500 again at
21:05:07Z, so nothing in this run was measured against a pool still filling.

### (2) an unreadable marker refuses rather than reporting zero — PASSED, 21:05:09Z

Two different refusals live on this path and the gate exercises both, because
"the marker refused" said of a screenshot does not say which:

* through `_BotBrowser.page()` the resolver refuses first —
  *"AiPlayerbot.RandomBotAccountPrefix is blank in aiplayerbot.conf. An empty
  prefix matches every account, which would report every player as a bot, so
  nothing is counted until it is set."* — `total=None`, no rows;
* `botlist.page` given a `Marker(prefix='')` directly refuses in its own words —
  *"…an empty marker would match every account on the server"*.

The ground, printed in the same breath: **the live install answered 900 at the
same moment.** The refusal came from the substituted conf, not from a sick
server.

The substitution is a copy of the install *directory* with `etc/` rebuilt inside
it, not a conf dropped in a temp dir. That was the one defect the adversarial
review found in this gate's plan before it ran, and it was real: `resolve_marker`
opens `<dir>/etc/aiplayerbot.conf`, and a **missing** file is not a refusal here
— `dbreads.py:132-137` falls back to the catalog default, which for wow-vanilla
is `rndbot`, this install's live value. The flat-copy version would have
answered with a working marker and a plausible total, and been photographed as a
refusal.

### (3) a readable marker matching nothing warns — PASSED, 21:05:10Z

`NOSUCHBOT` written into a conf copy and **read back through `resolve_marker`**
first (`Marker(prefix='NOSUCHBOT', source='conf')` — `source='conf'` is the
proof it came off disk), so the resolver, the clause builder, the live server
and the sentence are all in the loop. A `Marker` built in Python would have
exercised a branch CI already covers.

* the tab: *"no character matched the bot marker 'NOSUCHBOT', though this server
  has 901 characters. Page 1, 0 shown."*
* by hand with the same marker: `0`, against `901` characters
* **the live install at the same moment: `total=900`, `warning=''`**

That last line is the one thing this gate gained on its second run. The first
version of `stage_warn` asserted the warning without recording what the
unsubstituted install said at the same moment — so on a server that genuinely
had no bots, the stage would have passed while proving nothing. It now refuses
to run at all unless the live marker is answering.

The warning **leads** and the count is dropped: no `0 bots` appears anywhere in
the sentence, and no registry is named.

### (extra, this box's own) a filtered zero is not blamed on the marker — PASSED, 21:05:12Z

The bug this tree is the second witness to: the name filter is folded into the
same `WHERE` the total is counted over, so before tonight a filter matching
nothing drove `total == 0` down the marker's branch and the tab said *"no
character matched the bot marker 'RNDBOT'"* about a marker that had matched 900
rows a keystroke earlier.

Ground first: **unfiltered, `total=900`, `warning=''`.** Then, with that same
live marker:

| filter | total | the sentence |
|---|---|---|
| `%_` | 0 | *no bot's name begins with `'%_'` — this server has 900 bots* |
| `Zzzqqx` | 0 | *no bot's name begins with `'Zzzqqx'` — this server has 900 bots* |

`%_` is the escaping test as well as the sentence test: unescaped, `%_` matches
every name of at least one character and the total would have been 900. The
escape is `!`, not a backslash, because `NO_BACKSLASH_ESCAPES` is a real MariaDB
mode — and this is the first time that has been exercised against a live server
on this tree rather than a stub.

`3-warned.png` and `6-filter-found-nothing.png` are the two zeros side by side,
in different words, with the filter box visibly holding `%_` in the second. The
denominators differ on purpose: the marker's zero is held against **901
characters**, the filter's against **900 bots**.

### (regression) no registry named on a tree that has none — PASSED, 21:05:13Z

* `has_registry(wow-vanilla) = False`
* the attribution clause: `1 = 0`
* the schemas this install really has:
  `information_schema, characters, classiclogs, logs, realmd, sys, mysql, mangos, performance_schema`
  — **no `playerbots`**, so `registry: null` in the catalog is right
* the summary reads `900 bots. Page 1, 50 shown.` with no `by the playerbots
  registry` clause, and the rows read `Aadira — level 2 — offline` with no
  per-row signal

This is 8.5b's fix meeting its second one-signal tree, and it holds.

### (usability) paging and the filter — PASSED, 21:05:15Z

* page 1: 50 rows, `next_after=('Arader', 40)`
* page 2: 50 rows, **disjoint** from page 1, and `next_after` strictly greater —
  a cursor read, so an overlap would be a real defect and not `OFFSET` drift
* filtered on `Ara`: `total=4`, and the same whole clause plus the same `LIKE`
  run by hand also says `4` — the clause and not the `LIKE` alone, because the
  `LIKE` alone would also count the one character here that is not a bot

## The race → faction map, and how the faction was established

Parsed on the box from `~/vanilla-75b/data/dbc/ChrRaces.dbc` (9 records, 29
fields, 116-byte records), field 8 = `TeamID` per `DBCStructure.h:190`, decoded
7 = ALLIANCE and 1 = HORDE per `Player::TeamForRace` (`Player.cpp:6260-6277`).
**Not inherited from 8.5a**, whose map is AzerothCore's: race 11 does not exist
in this file at all.

| | |
|---|---|
| ALLIANCE | 1 Human, 3 Dwarf, 4 Night Elf, 7 Gnome, 9 Goblin |
| HORDE | 2 Orc, 5 Undead, 6 Tauren, 8 Troll |

The **names** were added after the first run. `races()` answered `race 6: HORDE`
and nothing more, which meant "6 is Tauren" would have been a thing this session
knew only by remembering it — and a faction handed to another lane as a bare
number is a fact nobody downstream can check. Field 17 is `m_name_lang`'s enUS
slot (`DBCStructure.h:199`), an offset into the string block; the gate now reads
it and asserts `1 = Human` and `6 = Tauren` so that a misparse cannot pass as a
race list.

Race 9 reading ALLIANCE is the reason this is parsed rather than assumed. It is
also a caution: field 8 is `m_BaseLanguage`, and Goblin speaks Common in this
file. No character on this server is race 9, so it changes nothing here.

## The handover to the client lane

**The asking character is `Asdff` — guid 901, account `VANGATE` (id 105,
gmlevel 0), race 6 Tauren, HORDE, level 1, `online = 0`.**

It is the **only** non-bot character on this server. The gate asks for every
character whose account is not `RNDBOT%` and got exactly one row back, twice.

Its `at_login` reads **0**, so no rename or customise is pending: this character
should reach the world without the dialog 8.5b's TBC run had to photograph its
way past (`8-client-in-world-renamed.png` in that folder). That is one fewer
unknown between the realm list and the `/who`, and it is read rather than hoped
— the column is in the same row the gate prints.

**`GATE83C` exists as an account and has no character.** `realmd.account` holds
it at id 108 — but with **`gmlevel = 2`**, and that is a trap, not a detail. The
faction filter in `HandleWhoOpcode` lives inside `if (security == SEC_PLAYER)`:
a GM asker skips it, sees both factions, and the opposite-faction shot would
then come back **full** rather than empty for a reason that has nothing to do
with the list. `VANGATE` is `gmlevel 0`, so the filter genuinely applies to it,
and the same-faction subject choice below is necessary rather than decorative.

Subjects, all four confirmed `online = 1` at **21:05:42Z** (re-check
immediately before typing — `gate85c.py still-online <name>`):

| Role | Bot | Level | Race | Why |
|---|---|---|---|---|
| **the clause** | `Ashah` | 3 | 6 Tauren | the asker's **own race**, so same team by construction |
| spare | `Ehpih` | 3 | 6 Tauren | same |
| spare | `Chitwogamit` | 4 | 6 Tauren | same |
| corroboration only | `Acharon` | 4 | 1 Human | the other side, per this install's DBC |

**`Ashah` is photographed as listed-and-online in `4-page-two.png`** — the app's
own claim about the very bot the client is to look for, made before the client
was touched.

How the faction was established, in order of strength: the primary subject
shares the asker's **race**, so it shares the team whatever the DBC decoding
says; the DBC was then parsed anyway, names and all, to choose the corroborating
subject and to say the word "Horde" out loud.

Why `/who` is filtered here at all — a config line, not the emulator:

* `src/mangos-classic/src/game/Entities/MiscHandler.cpp:159`
  `if (pl->GetTeam() != team && !allowTwoSideWhoList) continue;`
* `etc/mangosd.conf:931` `AllowTwoSide.WhoList = 0`

The same handler also drops anyone whose session security exceeds
`GM.InWhoList.Level` (`mangosd.conf:1278` = 3), anyone not `IsInWorld()`, and
anyone outside the **level range the client itself sends** in the packet. The
asker is level 1 and the subjects are 3 and 4, so the range the 1.12.1 client
puts in a `/who` is a thing the client lane has to look at rather than assume.

`VANGATE`'s password is **not known to this lane** and is recorded nowhere in
`pyplan/gates/`. 8.3c gated a password change on this exact tree that the client
could feel, so the app's own Accounts tab is the route — with 8.3c's caveat that
`account set password` **reports a failure when it works** here
(`HandleAccountSetPasswordCommand`).

### The network trap, found live and closed — 21:04:22Z

The realm row really was the trap the brief warned about. Measured, not assumed:

| | |
|---|---|
| before | `1  MaNGOS  192.168.10.134  8085` |
| from the client box, `192.168.10.134:3724` | **unreachable** |
| from the client box, `100.78.24.50:3724` and `:8085` | reachable |
| after | `1  MaNGOS  100.78.24.50  8085` |

Fixed through Yu'lon's own Networking feature and not by hand —
`networking.plan(entry, "lan", lan_ip="100.78.24.50")` then
`services.network_apply(plan)` — so what is proved is the feature that is
supposed to write the row, not the row. The report read *3 done, 1 skipped
(1 refused)*: `ufw allow 3724/tcp`, `ufw allow 8085/tcp`, `realmlist →
100.78.24.50`, and a refusal to run `ufw enable`, which is bug-checklist §39
behaving exactly as designed on a box reached over SSH. **`ufw` is inactive on
m910q**, so the two allows are rules added to a list nothing is enforcing; they
took nothing away from any other lane on the box.

The Tailscale address has to be *said*: `services.network_plan("lan")` detects
`192.168.10.134`, the unreachable one.

## Four citations that were wrong

Every line number in `gate85c.py` was re-checked against this tree after the run,
because the gate had been written blind and its citations were the part nobody
would notice was stale. Four of them were off — all plausible, all pointing a
reader at the wrong lines:

| cited as | really |
|---|---|
| `dbreads.py:132-137`, the catalog-default fallback on a missing conf | `dbreads.py:144-158` |
| `dbreads.py:166-173`, the resolver's blank-marker refusal | `dbreads.py:174-181` |
| `botlist.py:138-144`, `botlist.page`'s own empty-marker refusal | `botlist.py:147-153` |
| `apply.py:499-501`, the refusal to put a statement in argv | `apply.py:500-503` |

Corrected in place. The ones that were right and were checked anyway:
`etc/aiplayerbot.conf:57` (`RNDBOT`) and `:51-52` (Min/Max 500),
`MiscHandler.cpp:159`, `mangosd.conf:931` and `:1278`, `characters.sql:644`
(`race tinyint(3) unsigned`), `DBCStructure.h:190` (TeamID) and `:199` (name),
`Player.cpp:6260` (`TeamForRace`).

## Two hazards in artefacts that have not been run

**`client-who-vanilla.ps1` contradicts the measured client facts.** Its
`-NextField` parameter documents `{TAB}` as moving between the account and
password boxes "on the 2.4.3 and 1.12.1 clients". The lane brief and
`driving-a-wow-client-unattended` both say the opposite for 1.12.1: **TAB does
not move between them and both boxes must be clicked** (`-ClickFields`, with
`AccountY`/`PasswordY`). That comment was inherited from the TBC driver, where
it was true. It is a per-tree fact and it was not re-measured before it was
copied; the client lane should treat the parameter defaults in that file as TBC
values, not Vanilla ones.

## What version of the code this actually gated

The checkout is shared with other lanes and it moved **during** this run, so the
answer is not "HEAD" and had to be measured:

| file | on the box, gated | in the working tree afterwards |
|---|---|---|
| `yulon/botlist.py` | `2b4629d6` | `2b4629d6` — same |
| `yulon/dbreads.py` | `c181ece7` | `c181ece7` — same |
| `yulon/ui/controller_view.py` | `1db1d8b1` | `34945354` — **changed under the run** |

The two files the clauses are about did not move. `controller_view.py` did, and
the diff was read rather than assumed: 36 lines, entirely in the **Modules** tab
(`_module_values` swapping a `_CANCELLED` sentinel for a `(go_ahead, values)`
tuple). Not one line of it mentions `bot_summary`, `bot_list`, `refresh_bots`,
`_bots_listed`, `bot_filter`, `_bot_cursors`, `_BotBrowser` or `botlist`. The
screenshots on this page are therefore still pictures of the current Bots tab.

**`~/gate84c/pylauncher` on the box is a commit behind.** Its
`yulon/ui/controller_view.py` is md5 `5b10b774` against HEAD's `1db1d8b1` — the
manifest-prompt work landed after that copy was taken. Nothing on the bot path
differs, which is why this is worth writing down rather than shrugging at: a
run in that tree would have gated a tab a commit old and said nothing about it.
This gate ran from its own `~/gate85c/pylauncher`, copied from HEAD at 20:58Z,
and `gate85c.py` now says so at the `sys.path` line.

## Still open, and not this lane's

* **Clause 4** — a listed online bot found by the in-game who-list. The client
  driver has never been run against 1.12.1, and 8.3c drove this client only as
  far as the realm list, so the sequence from there to the character screen is
  still unmeasured.
* The catalog declares `databases.extra: ["logs"]` for wow-vanilla while this
  install's volume holds both `logs` and **`classiclogs`** — visible again in
  the schema list above. Not on the bot path (which touches only `characters`
  and `realmd`), but a fact-versus-wiring disagreement of the kind 8.3c found.
* `GATE83C`'s `gmlevel` reads **2** tonight; 8.3c's own transcript recorded it
  as **1** on 2026-09-07. Something changed it between the two runs. Harmless
  here — the account has no character — but it is a shared box.

## How it was run

On m910q, from a copy of HEAD's `pylauncher` at `~/gate85c/pylauncher`, with the
venv that has pydantic and PySide6:

```
~/gate81b-venv/bin/python gate85c.py total | refuse | warn | filterzero | nosplit | page | races | who
~/gate81b-venv/bin/python gate85c.py still-online <name>
```

Read-only against the server throughout. No container was started, stopped or
recreated; the live `~/vanilla-75b/etc/aiplayerbot.conf` was never edited — the
substituted markers live in copies under `/tmp` and only the bot-browsing seam
is pointed at them, with the SQL reader left as the live one so that a refusal
cannot be the database password going missing. The one write to the server was
the realmlist row, through the Networking feature, described above.
