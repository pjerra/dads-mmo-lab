# 8.5d — Browse Bots, WoW Tortoise — **RUN 2026-09-07 23:13Z–23:19Z on m910q**

> **This file used to be a plan and is now a record.** Every stage below was run
> against the **live** Tortoise stack on m910q (`tortoise-db`, `tortoise-realmd`,
> `tortoise-mangosd`, up 24 minutes when the first stage started). Its output,
> with the clock on every line, is `transcript.txt`; the screenshots the stages
> wrote are `1-bots.png` … `6-filter-found-nothing.png`, pulled off the box
> unedited.
>
> **Three of the four clauses are proved. Clause (4) is not**, and it is not
> proved because this lane held m910q but **not the game client** on `vmhost`.
> Everything the client half needed *from the server* is now settled and written
> down below; the client itself was never started.

**The box.** `pyplan/checklist.md:2499` — *"Browse Bots, WoW Tortoise — as 8.5b
against tw_logon/tw_char. Definition of done: as 8.5b. Gate: m910q; evidence
`pyplan/gates/8.5d-tortoise-m910q-<date>/`. Visible effect: a listed online bot
is found by the in-game who-list."*

| clause | verdict | where |
|---|---|---|
| (1) the total equals the same clause run by hand | **PASSED** | `transcript.txt`, stage `total`, 23:13:20Z and again 23:18:34Z |
| (2) an unreadable marker refuses rather than reporting zero | **PASSED** | stage `refuse`, 23:13:27Z; `2-refused.png` |
| (3) a readable marker matching nothing WARNS rather than reporting zero | **PASSED** | stage `warn`, 23:13:32Z; `3-warned.png` |
| (4) a listed online bot is found by the in-game who-list | **PASSED 2026-09-08 09:58–10:01Z** — three subjects, three answers | `client-t84d-r2.log`; `8-who-found-sietta.png`, `9-who-found-all-three.png` |

> **Clause (4) was closed on 2026-09-08 by the 8.4d lane**, which held m910q and
> `vmhost` at once — the reason the two boxes were run as one job. Everything
> below the line that says "the client was never started" was written on
> 2026-09-07 and is left standing as it was; the run that closed the clause is
> **[the section at the end of this file](#clause-4-closed-2026-09-08)**, and the
> two `.ps1` files in this folder were changed by it.

---

## What the code needed: nothing

A verification lane had already checked all nine catalog claims for
`wow-tortoise`'s observability block against this fork's own source and returned
an empty change list. **This run changed no package code, and the run confirms
that was right:** every stage passed on the tree exactly as committed.

The tree that ran is the **committed `yulon-phase8` tip, `42008131`**, exported
with `git archive` and unpacked to `~/gate85d/pylauncher` on m910q at 23:12Z —
deliberately *not* a copy of the shared checkout, which at that moment carried
another lane's uncommitted 8.9a purge work, and not `~/gate84c/pylauncher`,
which 8.5c already found a commit behind.

```
2b4629d6cf137e0533ca408f269521ae  ~/gate85d/pylauncher/yulon/botlist.py
437ed1fd0b67991161868c0b847f954d  ~/gate85d/pylauncher/yulon/dbreads.py
5b10b774e62b737333440ac19c025922  ~/gate85d/pylauncher/yulon/ui/controller_view.py
eda4a4930ad843df89514e046747dfde  ~/gate85d.py            (this folder's gate85d.py, LF)
```

`botlist.py`'s md5 is the same `2b4629d6` that 8.5c and 8.4c ran — so this is
the *third* tree to gate that file unchanged.

---

## The numbers, with their clocks

All UTC. m910q's local clock is CEST, so the same instants read 01:13–01:19 in
`claude-say`'s output.

### Clause (1) — stage `total`, 23:13:20Z (repeated 23:18:34Z, identical)

```
the marker this install is running with: prefix='RNDBOT' source='conf'
the tab says: total=900 by_registry=0 by_prefix=900 rows=50
the app's identity clause: account IN (SELECT id FROM tw_logon.account
                                       WHERE UPPER(username) LIKE 'RNDBOT%')
by hand, with the app's own clause:            900
by hand, written independently of the app:     900
characters on this server altogether:          901
```

Three readings, one second apart, and they agree. `900` is neither `0` nor
`901`, so the equality is not a tautology — that assertion is in the stage
precisely because *if every character were a bot the clause would prove
nothing*.

**The disk readings taken with the stack down were right.** `tw_char/characters.MYI`'s
header read **901 records / 39 deleted**; `mysqld` says 901. `tw_logon/account.ibd`
held `RNDBOT0 … RNDBOT97`; the live clause counts 900 characters behind them.

### Clause (2) — stage `refuse`, 23:13:27Z — `2-refused.png`

Both refusals on the path were exercised and each is labelled, because "the
marker refused" said of a screenshot does not say which one:

```
a blank marker: total=None rows=0
  problem='AiPlayerbot.RandomBotAccountPrefix is blank in aiplayerbot.conf. An empty prefix
           matches every account, which would report every player as a bot, so nothing is
           counted until it is set.'
and botlist's own refusal, given an empty Marker:
  problem="this install's bot marker could not be read, and an empty marker would match
           every account on the server"
the live install at the same moment: total=900 problem=''
```

The last line is the ground: the live install answered **900 at the same
moment**, through the same seam, so the refusal came from the substituted conf
and not from a sick server or a wrong password. The tab shows **zero rows** and
prints the key that stopped it. It never says "0 bots".

### Clause (3) — stage `warn`, 23:13:32Z — `3-warned.png`

```
the marker READ BACK off the conf copy at /tmp/gate85d-nomatch:
  Marker(prefix='NOSUCHBOTPREFIX', source='conf')
a marker matching nothing: total=0 by_prefix=0 problem=''
  warning="no character matched the bot marker 'NOSUCHBOTPREFIX',
           though this server has 901 characters"
by hand, the same marker: 0; characters on the server: 901
the live install at the same moment: total=900 warning=''
the tab reads: "no character matched the bot marker 'NOSUCHBOTPREFIX', though this server
                has 901 characters. Page 1, 0 shown."
```

*Readable* is the load-bearing word, and it is why the marker was written into a
conf copy and **read back through `resolve_marker`** rather than constructed in
Python. The refusal-recipe trap the plan warned about is real on this tree and
it did not fire: a conf copied to the wrong place resolves to the catalog
default `rndbot`, which on **this** install is a *working* marker, so the stage
would have answered 900 while photographing itself as proof of a warning.
`fake_conf` asserts the copy is where the resolver looks; `warn` asserts the
prefix it reads back is the substituted one.

### The other cause of a zero — stage `filterzero`, 23:13:37Z — `6-filter-found-nothing.png`

```
the live marker, unfiltered: total=900 warning=''
filtered on '%_':     total=0 warning="no bot's name begins with '%_' — this server has 900 bots"
filtered on 'Zzzqqx': total=0 warning="no bot's name begins with 'Zzzqqx' — this server has 900 bots"
```

`%` and `_` are the LIKE wildcards and this is the `!`-escaping exercised against
a live MariaDB. Unescaped, `%_` would match every name of at least one character
and return 900. It returned 0, and it blamed the **filter**, counting against the
900 bots it excluded rather than against the 901-row table. So clause (3)'s
sentence has exactly one cause.

### The inherited 8.5b reading — stage `nosplit`, 23:13:44Z — `1-bots.png`

```
has_registry(wow-tortoise) = False
the attribution clause on this tree: '1 = 0'
the schemas this install really has:
  information_schema,tw_logon,tw_world,tw_logs,tw_char,sys,mysql,performance_schema
summary: '900 bots. Page 1, 50 shown.'
first rows: ['Aberen — level 12 — online', 'Aberio — level 43 — offline', 'Aberus — level 34 — online']
```

Asked of the **running server**, not of the catalog: there is no `playerbots`
schema here, so no registry is named, no split is drawn, and the rows carry no
signal suffix. Fourth tree, third one-signal tree, gated as a regression.

### Paging — stage `page`, 23:13:45Z — `4-page-two.png`, `5-filtered.png`

```
page 1: ['Aberen', 'Aberio', 'Aberus', 'Acie'] … (50 rows), next_after=('Aphina', 245)
page 2: ['Apolo', 'Aradalos', 'Arandel', 'Aranden'] … (50 rows)
filtered on 'Apo': total=1 first=['Apolo']
by hand, the whole clause and the same LIKE: 1
```

A `(name, guid)` cursor, so the two pages are disjoint by construction and an
overlap would be a real defect rather than OFFSET drift. The filter string is
taken from **page two's** rows — data the stage did not pick — and the hand check
uses the app's *whole* WHERE and not the LIKE alone, because the LIKE alone would
also count the one non-bot character and the gate would fail by one on a correct
app.

### The faction map — stage `races`, 23:13:53Z

Parsed from **this install's own** `data/dbc/ChrRaces.dbc`, which has **ten**
races:

| race | name | side | baseLanguage |
|---|---|---|---|
| 1 | Human | ALLIANCE | 7 |
| 2 | Orc | HORDE | 1 |
| 3 | Dwarf | ALLIANCE | 7 |
| 4 | Night Elf | ALLIANCE | 7 |
| 5 | Undead | HORDE | 1 |
| 6 | Tauren | HORDE | 1 |
| 7 | Gnome | ALLIANCE | 7 |
| 8 | Troll | HORDE | 1 |
| **9** | **Goblin** | **HORDE** | 1 |
| **10** | **High Elf** | **ALLIANCE** | 7 |

The Vanilla install 8.5c gated put race 9 on the **Alliance** side. Inheriting a
sibling's map here would have put every faction choice on the wrong team, which
is exactly why it is parsed per tree.

### The who-list rules — stage `wholaw`, 23:13:54Z / 23:18:51Z

```
AllowTwoSide.WhoList = 1   (etc/mangosd.conf:919, column 0)
GM.InWhoList.Level  = 3    (etc/mangosd.conf:1644, column 0)
```

`AllowTwoSide.WhoList = 1` means the faction filter is **off** here — the
opposite of the Vanilla install — so an opposite-faction subject is *expected to
be found*, and an empty answer for one would be a finding about the config or
the client, not a filter working as designed.

### The realm row — stage `realmrow`, 23:13:54Z

```
1  Tortoise WoW  100.78.24.50  8090
```

Already the Tailscale address. 8.5c's trap (Vanilla's row held `192.168.10.134`,
which the Hyper-V host cannot reach) is **not** present here, and no Networking
apply was needed.

**And it was checked from the client box**, which is the reading that actually
settles it. From `ssh vmhost`, a bare `TcpClient.Connect`, 23:17:00Z — the game
client was *not* started:

```
port 3724: True      port 8090: True
```

---

## Clause (4): the route is now known, and the client was never driven

### What the plan could not read from disk, and what the server said

The plan's stated biggest risk was: *"None of those accounts is known to have a
character"*, because `tw_char.characters` is MyISAM and each row's account id
lives inside the record blocks. With `mysqld` up it was one question.

**There is exactly one non-bot character on this server** (23:14:01Z):

```
guid 901  Dorta  race 9 (Goblin, HORDE)  class 1  level 1
map 1  zone 5536  online 0  at_login 0  totaltime 140
account 104 TORTGATE  rank 0
```

`totaltime 140` — it has been in the world before. `at_login 0` — no rename or
customise prompt stands between the character screen and the world. So **no
character has to be created**, and the 1.18.1 creation screen, which no run has
ever driven, is not on the path after all.

### The blocker was routed around, and the plan's own account table was wrong

The password bug is real and unchanged: on the binary now on m910q,
`account set password` stores an empty-name hash for any account that has logged
in, so 8.3a's password button may not be used. The plan's answer was to log in
as `GATE83D`, whose password is known — **but `GATE83D` owns no character.** The
only account that owns one is `TORTGATE`, and the plan's table said its password
was *"not recorded anywhere in `pyplan/gates/`"*.

**That claim is false.** It is recorded, in
`pyplan/gates/8.1d-tortoise-m910q-2026-09-07/README.md:52`, as `T0RT-G@TE12`.
Checked against the live row, read-only, at 23:15:04Z:

| | |
|---|---|
| `sha_pass_hash = UPPER(SHA1('TORTGATE:T0RT-G@TE12'))` | **1** |
| `sha_pass_hash = UPPER(SHA1(':T0RT-G@TE12'))` (the empty-name hash) | 0 |
| `LENGTH(v)`, `LENGTH(s)` | 64, 64 — the SRP6 verifier is already derived |
| `rank`, `last_login` | 0, `2026-09-07 06:31:47` |

So the account is **not bricked**, needs **no password change**, and will not
even hit 8.1d's "refused on the first login, accepted on the second" wart,
because its `v`/`s` were derived during that first refusal on 2026-09-07 and are
still there. The route to clause (4) is `TORTGATE` / `T0RT-G@TE12` → `Dorta`.

The corrected account table:

| account | password | state | use for 8.5d |
|---|---|---|---|
| **`TORTGATE`** | **`T0RT-G@TE12`** (8.1d README:52) | hash correct, not empty-name, `v`/`s` derived, rank **0** | **the only account with a character — use this one** |
| `GATE83D` | `n3w-p@ss34` | hash correct, rank 4, reached the realm list on 2026-09-07 | **no character**; only useful if one is created |
| `CTRLONE`, `CTRLTWO` | `r0und-two22` | hash correct, `last_login` still `0000-00-00` | spares, no characters |
| `SHAPROBE` | `l0ck-p@ss77` | hash correct, never logged in, rank 3 | spare, no character |
| `GATE83E`, `TREATONE`, `TREATTWO` | — | BRICKED by the password bug | do not use |

**The rule stands: never press the password button on any of them.** One change
on `TORTGATE` would brick it and take the only route to clause (4) with it.

### The consequence of the asker being rank 0

`TORTGATE` is `rank 0` = `SEC_PLAYER`, so **the 30-second `/who` cooldown fully
applies to this asker**. The plan had assumed a rank-4 asker that bypasses it.
A second `/who` inside 30 s returns *nothing at all* and photographs exactly like
a miss, so the next turn must ask about **one name at a time, ≥35 s apart**, and
re-run `gate85d.py still-online <name>` immediately before each.

### The subjects the app chose — stage `who`, 23:14:02Z

The app listed **190 online bots** across the pages it was asked for. Of those,
the ones sharing the asker's own race (9, Goblin — so the asker's own team by
construction, no faction map needed), nearest its level 1 first:

```
Caterinny  7  9        Berino  9  9        Deronimo  9  9
Doloreny  16  9        Arineri 17  9       Arinarine 19  9   …
```

and one on the other side, for corroboration only: `Aberus`, level 34, race 10
(High Elf, ALLIANCE) — which on this install, with `AllowTwoSide.WhoList = 1`,
is **expected to be found**.

**None of these names was ever asked of the client.** The bot pool rotates, so
the next turn re-runs `who` and then `still-online <name>` rather than reusing
these.

### What the next turn has to do

1. Take the client on `vmhost` (`C:\clients\TurtleWoW`, `WoW.exe` — *not*
   `TurtleWoW.exe`, which is the installer, nor `turtle-wow.exe`, the launcher).
2. Drive it with `drive-who-turtle.ps1 -Realmlist 100.78.24.50 -Account TORTGATE
   -Password 'T0RT-G@TE12'` — **not** `GATE83D`, which has no character.
   Both login boxes are **clicked** (TAB and ENTER do not move between them on
   this client; realmd logged a password in the username field when 8.3d tried).
   `SET gxWindow "1"` and `SET movie "0"` first — **ESC quits this client**
   rather than skipping, so the driver sends ESC nowhere.
3. Enter the world as `Dorta`, ask about **one** same-race subject, wait ≥35 s
   before any second name, and photograph the client's own `/who` answer with the
   process-alive line beside it.
4. The scheduled task now deletes itself. **`drive-who-turtle.ps1` was fixed in
   this run**: as written it created `yulon-who-turtle` with `/sc once /st 23:59`
   and never removed it, which is exactly how three stale tasks re-fired at 23:59
   and stomped a live session on 2026-09-07. The body is now wrapped in
   `try { … } finally { schtasks /end; schtasks /delete /f }`, both through
   `cmd` with output swallowed, so a timeout or an exception still takes the task
   away. Both `.ps1` files were parse-checked with `PSParser::Tokenize` under
   Windows PowerShell 5.1 at 23:20Z — **neither has been executed.**

---

## The four predictions that were wrong

Written blind, checked on the box. None of them changed a verdict, and that is
the point of writing them down: the plan warned that 8.5c re-checked its own
citations after its run and found four wrong, all plausible. This one found four
too.

1. **`TORTGATE`'s password was recorded.** The plan's table said "not recorded
   anywhere in `pyplan/gates/`" and it is in `8.1d/README.md:52`. Had that stood,
   the only route to clause (4) would have been ruled out and a character would
   have been created on `GATE83D` through a screen no run has ever driven.
2. **`MiscHandler.cpp:140` is not the level-range line — `:142` is.** The line
   is `if (lvl < level_min || lvl > level_max)`. `:125` (faction) and `:129` (GM
   level) are right, and both sit inside `if (security == SEC_PLAYER)` at `:122`.
3. **The cooldown's mechanism was cited at the wrong line.** `:253` is the
   *check*, and it applies to **every** asker unless the player carries
   `CUSTOM_PLAYER_FLAG_BYPASS_WHO_COOLDOWN`. What is `SEC_PLAYER`-only is the
   *stamp*, `:326-327`: `if (GetSecurity() == SEC_PLAYER) m_lastWhoRequest =
   time(nullptr);`. A GM asker therefore never refreshes the timestamp and always
   passes — the plan's conclusion was right, its line was not. Moot for this run,
   whose asker is rank 0.
4. **Every Python line number in the plan was off.** `controller_view.py:992 /
   911 / 612 / 775 / 888 / 2899 / 2999`, `dbreads.py:144-158 / 174-181`,
   `botlist.py:147-153` were read off the **shared checkout**, which carries
   another lane's uncommitted 8.9a purge code. In the tree that actually ran
   (`yulon-phase8` @ `42008131`) they are:

   | claim | in the tree that ran |
   |---|---|
   | `bots=_BotBrowser(...)` — WotLK / TBC / Vanilla / Tortoise | `:602` / `:765` / `:878` / `:961` |
   | `def _for_tortoise` | `:901` |
   | `_build_bots_tab` | `:2764` |
   | `botlist.has_registry(self.entry)` (the split) | `:2864` |
   | `dbreads` — `except FileNotFoundError` | `:136` |
   | `dbreads` — the blank-prefix sentence | `:169` |
   | `botlist` — `PAGE_SIZE = 50` | `:66` |
   | `botlist` — "every account on the server" | `:151` |
   | `botlist` — `_why_zero` / `has_registry` / `_registry_clause` | `:215` / `:268` / `:288` |

The citations that **held** were re-read on the box at 23:16Z and are in
`transcript.txt`: `etc/aiplayerbot.conf:57,58,63,64`; `etc/mangosd.conf:919,1644`;
`etc/realmd.conf:88,92,96`; `DBCStructure.h:147,156`; `Player.cpp:7943`;
`PlayerbotAIConfig.cpp:545`; `PlayerbotAIConfig.h:16`;
`sql/create_databases.sql:23,963` — with the correction that that file lives at
`src/tortoise-wow/sql/`, not `~/tortoise-server/sql/`.

---

## What was left as it was found

Nothing was started, stopped or reconfigured. The live
`~/tortoise-server/etc/aiplayerbot.conf` was never edited — the substituted
markers live in copies under `/tmp/gate85d-blank` and `/tmp/gate85d-nomatch`, and
only the bot-browsing seam was pointed at them; the SQL reader stayed the live
one. The Tortoise stack is **still up**, as this lane found it. On `vmhost`
nothing was created: one outbound TCP connect, no process, no scheduled task.

## What this record does **not** claim

* **Clause (4) is not proved.** No `/who` was typed, no client was started, and
  the four names above are candidates, not answers.
* The bot pool moves at runtime — `AiPlayerbot.MinRandomBots = MaxRandomBots =
  500`, `RandomBotAccountCount = 100` — so "190 online" and "`Caterinny` is
  online" are readings at 23:14:02Z and nothing more.
* **`--checks` was run from this lane's own worktree, not from the shared
  checkout**, and that worktree's base is the Phase 7 merge `ef113d2b` — which
  predates `botlist.py` entirely.
  `YULON_TEST_BOX=yulon-fedora bash ~/run-tests-vm.sh --checks`, 2026-09-07
  23:21Z: **2810 passed, 4 skipped**, mypy clean on all three platforms, ruff
  clean, black clean over 140 files, ending `=== --checks: ALL GREEN ===`.
  Compare the plan's two RED runs at 00:25Z and 00:33Z, whose 3362 and 3381
  tests were the shared checkout's and whose every failure named another lane's
  in-flight `yulon/purge.py`; running from an isolated worktree is what removed
  them.
  What that green **means** is narrow and worth saying plainly: this folder
  breaks nothing. It does **not** re-say that `botlist.py`/`dbreads.py` pass CI —
  those files are not in the tree it ran over. Their md5s are recorded above and
  are the ones 8.5b and 8.5c already gated green, and whoever merges this box
  should re-run `--checks` on a tree that actually contains them.
* Every line number here is a claim like any other. The ones above were re-read
  on the box on 2026-09-07 at 23:16Z; re-check them again before quoting this
  file, because the Python ones are bound to commit `42008131` and will move.

---

## Clause (4), closed 2026-09-08

Run by the **8.4d lane**, which needed the same server and the same client
session, on `m910q` (server) and `vmhost` (client). Three client sessions, all
driven through `schtasks /it`; each deleted its own task, and `schtasks /query`
showed no `yulon-who-turtle` afterwards. The full driver logs are
`client-t84d-dry.log`, `client-t84d-who.log` and `client-t84d-r2.log`, and every
capture in them carries the client's alive/dead reading beside it — **the client
was alive at all 21 captures of the run that proved this.**

### The answers, in the client's own words

`9-who-found-all-three.png`, 10:00:51Z — three `/who` answers still on the chat
frame at once, 35 s and 55 s apart:

```
1 player total
[Caterinny]: Level 7 Goblin Hunter <Within Reason> - Durotar
1 player total
[Gwenora]: Level 5 High Elf Warrior - Elwynn Forest
1 player total
```

and `8-who-found-sietta.png`, 09:59:05Z, the first of them with the line that
made it readable:

```
Left Channel: [4. World]
[Sietta]: Level 5 Goblin Rogue <Tainted Bunnies> - Durotar
1 player total
```

All three had been **listed as online by the app** and re-checked through
`gate85d.py still-online` at 07:55:2xZ, minutes before the client was started.
The asking character is `Dortagate` (guid 901, the renamed `Dorta`), a level-1
Goblin — `rank 0`, so the 30-second cooldown fully applied and the driver's
35-second gap is what the three separate answers rest on.

### Three predictions this run settled

1. **`AllowTwoSide.WhoList = 1` holds in the client.** `Gwenora` is a High Elf
   (race 10, ALLIANCE) and was found by a HORDE asker — the reverse of the
   Vanilla install 8.5c gated, and what stage `wholaw` predicted from
   `etc/mangosd.conf:919`. So the faction filter really is a config line on
   these trees and not a property of the family.
2. **`/who` on 1.18.1 is not level-filtered for a bare name.** A level-**1**
   asker found subjects at levels 5, 5 and 7. `MiscHandler.cpp`'s level-range
   check is real; what this client puts in a bare `/who Name` does not exclude
   them.
3. **`1 player total` is this client's wording too.** 8.5b measured that string
   on 2.4.3 and this record refused to assert it in advance. It is the same.

### What went wrong first, and why the fix is in the driver

**The first session found all three names and photographed none of the
answers.** `client-t84d-who.log`, 09:50–09:52Z: three `/who` commands sent, three
frames taken 12 s later, and every frame shows `[4. World]` bot small-talk with
no answer in it. **500 playerbots talk in the World channel at about eight lines
every twelve seconds**, so each answer had scrolled off the chat frame before the
shutter. `7b-the-first-run-drowned-by-world-chat.png` is one of those frames, kept
because it is exactly what a miss looks like and is not one.

Two changes to `client-who-turtle.ps1`, both live-proved by the second session:

* **`-LeaveChannels`** sends `/leave <channel>` before asking. `-LeaveChannels
  World` produced `Left Channel: [4. World]` in the frame, and the chat was quiet
  enough that three answers 55 s apart were all still on screen at the end.
* **two shots per name, at 4 s and at 12 s.** The early one is the evidence and
  the late one is the control: an answer in the early frame and none in the late
  one is the chat scrolling rather than the server refusing — which is the pair
  the first run did not have and could not have told apart.

### The realm flow past the realm list, measured at last

The record's own note said *"everything past the realm list on 1.18.1 is
UNMEASURED"*. One `-DryRealm` run bought it (`client-t84d-dry.log`, 09:43–09:46Z):

* **there is no realm-selection dialog and no language panel on this client.**
  Login goes **straight to the character screen** — see
  `../8.4d-tortoise-m910q-2026-09-08/client-1-realm-flow-is-the-character-screen.png`.
  `-RealmStyle` (the 2.4.3 language + Suggest Realm + Accept sequence) is
  therefore wrong here and was never used.
* the driver's "Okay on the realm dialog" click at `0.616, 0.788` lands on
  **empty floor** on that screen and is harmless. It was left in rather than
  removed, because it is the one step no run has had to press here and removing
  it on one client's evidence is how the next fork's run loses an evening.
* login to character screen took **62 s**; character screen to standing in the
  world took **44 s**.
* `ENTER` on the character screen enters the world, as assumed.

### What was left on each box

* **`vmhost`**: no client running, no scheduled task (`yulon-who-turtle` deleted
  by the driver's own `finally`, confirmed absent), 21 frames and three logs
  under `C:\Users\PK\client-login\` with the `t84d-` prefix. The stale
  `yulon-*` tasks that were there before this run (`yulon-anykey`,
  `yulon-login`, `yulon-van84c` and nine more) are **other lanes' and were not
  touched**.
* **`m910q`**: the Tortoise stack up, as this lane found it.
* **the character**: `Dorta` is now **`Dortagate`** — renamed *in the client*, by
  8.4d's own button, to prove that box's rename clause. Guid 901, account
  `TORTGATE`, password unchanged. **The next lane that wants this route asks for
  `Dortagate`.**
